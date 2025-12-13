//! API handlers with rate limiting

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::{LicenseService, StatsService};
use crate::state::{AppState, Session};

/// Heartbeat request from CS2 panel
#[derive(Debug, Deserialize)]
pub struct HeartbeatReq {
  pub key: String,
  pub machine_id: String,
  pub session_id: String,
  /// Optional compressed stats payload (gzip)
  #[serde(default)]
  pub stats: Option<String>,
}

/// Heartbeat response
#[derive(Debug, Serialize)]
pub struct HeartbeatRes {
  pub success: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub magic_token: Option<i64>,
}

impl HeartbeatRes {
  pub fn ok(magic: i64) -> Self {
    Self {
      success: true,
      message: None,
      magic_token: Some(magic),
    }
  }

  pub fn invalid(message: impl Into<String>) -> Self {
    Self {
      success: false,
      message: Some(message.into()),
      magic_token: None,
    }
  }
}

/// Generate magic token for session validation
fn generate_magic(session_id: &str, secret: &str) -> i64 {
  let combined = format!("{}{}", session_id, secret);
  let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
  for byte in combined.bytes() {
    hash ^= byte as u64;
    hash = hash.wrapping_mul(0x100000001b3); // FNV-1a prime
  }
  hash as i64
}

/// Heartbeat endpoint - called every minute by CS2 panel
pub async fn heartbeat(
  State(app): State<Arc<AppState>>,
  Json(req): Json<HeartbeatReq>,
) -> (StatusCode, Json<HeartbeatRes>) {
  let now = Utc::now().naive_utc();
  let magic = generate_magic(&req.session_id, &app.secret);

  // Check if session already exists (fast path)
  if let Some(mut sessions) = app.sessions.get_mut(&req.key) {
    if let Some(sess) = sessions.iter_mut().find(|s| s.session_id == req.session_id) {
      sess.last_seen = now;
      return (StatusCode::OK, Json(HeartbeatRes::ok(magic)));
    }
  }

  // Validate license with database
  let license = match LicenseService::validate(&app.db, &req.key).await {
    Ok(l) => l,
    Err(AppError::LicenseNotFound) => {
      app.drop_sessions(&req.key);
      return (
        StatusCode::UNAUTHORIZED,
        Json(HeartbeatRes::invalid("Invalid license")),
      );
    }
    Err(AppError::LicenseInvalid) => {
      app.drop_sessions(&req.key);
      return (
        StatusCode::FORBIDDEN,
        Json(HeartbeatRes::invalid("License expired or blocked")),
      );
    }
    Err(_) => {
      return (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(HeartbeatRes::invalid("Internal error")),
      );
    }
  };

  // HWID locking check
  if let Some(ref bound_hwid) = license.hwid_hash {
    if bound_hwid != &req.machine_id {
      return (
        StatusCode::FORBIDDEN,
        Json(HeartbeatRes::invalid("HWID mismatch")),
      );
    }
  } else {
    // First use - bind HWID
    let _ = LicenseService::bind_hwid(&app.db, &req.key, &req.machine_id).await;
  }

  // Get or create session list
  let mut entry = app.sessions.entry(req.key.clone()).or_insert_with(Vec::new);

  // Thin GC - remove stale sessions
  let timeout = app.config.session_timeout_secs;
  entry.retain(|s| (now - s.last_seen).num_seconds() < timeout);

  // Check session limit
  let max_sessions = license.max_sessions as usize;
  if entry.len() >= max_sessions {
    return (
      StatusCode::CONFLICT,
      Json(HeartbeatRes::invalid(format!(
        "Session limit reached ({}/{})",
        entry.len(),
        max_sessions
      ))),
    );
  }

  // Add new session
  entry.push(Session {
    session_id: req.session_id,
    hwid_hash: Some(req.machine_id),
    last_seen: now,
  });

  // Process stats if provided
  if let Some(stats_b64) = req.stats {
    if let Ok(compressed) = base64_decode(&stats_b64) {
      if let Ok(stats) = StatsService::decompress_stats(&compressed) {
        let active = entry.len() as i32;
        let _ = StatsService::update_from_telemetry(&app.db, license.tg_user_id, &stats, active)
          .await;
      }
    }
  }

  (StatusCode::OK, Json(HeartbeatRes::ok(magic)))
}

fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
  // Simple base64 decode without external dependency
  const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

  let input = input.trim_end_matches('=');
  let mut output = Vec::with_capacity(input.len() * 3 / 4);

  let mut buffer = 0u32;
  let mut bits = 0;

  for c in input.bytes() {
    let val = ALPHABET.iter().position(|&x| x == c).ok_or(())? as u32;
    buffer = (buffer << 6) | val;
    bits += 6;

    if bits >= 8 {
      bits -= 8;
      output.push((buffer >> bits) as u8);
      buffer &= (1 << bits) - 1;
    }
  }

  Ok(output)
}

/// Stats submission endpoint
#[derive(Debug, Deserialize)]
pub struct StatsReq {
  pub key: String,
  pub session_id: String,
  /// Compressed JSON stats (gzip + base64)
  pub data: String,
}

#[derive(Debug, Serialize)]
pub struct StatsRes {
  pub success: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}

pub async fn submit_stats(
  State(app): State<Arc<AppState>>,
  Json(req): Json<StatsReq>,
) -> (StatusCode, Json<StatsRes>) {
  // Validate license
  let license = match LicenseService::validate(&app.db, &req.key).await {
    Ok(l) => l,
    Err(_) => {
      return (
        StatusCode::UNAUTHORIZED,
        Json(StatsRes {
          success: false,
          message: Some("Invalid license".into()),
        }),
      );
    }
  };

  // Verify session exists
  let session_valid = app
    .sessions
    .get(&req.key)
    .map(|sessions| sessions.iter().any(|s| s.session_id == req.session_id))
    .unwrap_or(false);

  if !session_valid {
    return (
      StatusCode::UNAUTHORIZED,
      Json(StatsRes {
        success: false,
        message: Some("Invalid session".into()),
      }),
    );
  }

  // Decompress and process stats
  match base64_decode(&req.data) {
    Ok(compressed) => match StatsService::decompress_stats(&compressed) {
      Ok(stats) => {
        let active = app
          .sessions
          .get(&req.key)
          .map(|s| s.len())
          .unwrap_or(0) as i32;

        if let Err(e) =
          StatsService::update_from_telemetry(&app.db, license.tg_user_id, &stats, active).await
        {
          tracing::warn!("Failed to update stats: {}", e);
        }

        (
          StatusCode::OK,
          Json(StatsRes {
            success: true,
            message: None,
          }),
        )
      }
      Err(e) => (
        StatusCode::BAD_REQUEST,
        Json(StatsRes {
          success: false,
          message: Some(format!("Invalid stats data: {}", e)),
        }),
      ),
    },
    Err(_) => (
      StatusCode::BAD_REQUEST,
      Json(StatsRes {
        success: false,
        message: Some("Invalid base64 encoding".into()),
      }),
    ),
  }
}

/// Health check endpoint
pub async fn health() -> &'static str {
  "OK"
}
