use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
  error::Error,
  state::{AppState, Session},
  sv::Stats,
};

#[derive(Debug, Deserialize)]
pub struct HeartbeatReq {
  pub key: String,
  pub machine_id: String,
  pub session_id: String,
  /// Optional compressed stats payload (gzip)
  #[serde(default)]
  pub stats: Option<String>,
}

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
    Self { success: true, message: None, magic_token: Some(magic) }
  }

  pub fn invalid(message: impl Into<String>) -> Self {
    Self { success: false, message: Some(message.into()), magic_token: None }
  }
}

fn generate_magic(session_id: &str, secret: &str) -> i64 {
  let combined = format!("{}{}", session_id, secret);
  let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
  for byte in combined.bytes() {
    hash ^= byte as u64;
    hash = hash.wrapping_mul(0x100000001b3); // FNV-1a prime
  }
  hash as i64
}

pub async fn heartbeat(
  State(app): State<Arc<AppState>>,
  Json(req): Json<HeartbeatReq>,
) -> (StatusCode, Json<HeartbeatRes>) {
  let now = Utc::now().naive_utc();
  let magic = generate_magic(&req.session_id, &app.secret);

  if let Some(mut sessions) = app.sessions.get_mut(&req.key)
    && let Some(sess) =
      sessions.iter_mut().find(|s| s.session_id == req.session_id)
  {
    sess.last_seen = now;
    return (StatusCode::OK, Json(HeartbeatRes::ok(magic)));
  }

  let license = match app.sv().license.validate(&req.key).await {
    Ok(license) => license,
    Err(Error::LicenseNotFound) => {
      app.drop_sessions(&req.key);
      return (
        StatusCode::UNAUTHORIZED,
        Json(HeartbeatRes::invalid("Invalid license")),
      );
    }
    Err(Error::LicenseInvalid) => {
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

  let mut entry = app.sessions.entry(req.key.clone()).or_insert_with(Vec::new);
  entry.retain(|s| {
    (now - s.last_seen).num_seconds() < app.config.session_lifetime
  });

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

  entry.push(Session {
    session_id: req.session_id,
    hwid_hash: Some(req.machine_id),
    last_seen: now,
  });

  if let Some(stats_b64) = req.stats
    && let Some(compressed) = base64_decode(&stats_b64)
    && let Ok(stats) = Stats::decompress_stats(&compressed)
  {
    let active = entry.len() as u32;
    let _ = (app.sv().stats)
      .update_from_telemetry(license.tg_user_id, &stats, active)
      .await;
  }

  (StatusCode::OK, Json(HeartbeatRes::ok(magic)))
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
  base64::prelude::BASE64_STANDARD.decode(input).ok()
}

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
  let license = match app.sv().license.validate(&req.key).await {
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

  match base64_decode(&req.data) {
    Some(compressed) => match Stats::decompress_stats(&compressed) {
      Ok(stats) => {
        let active = app.sessions.get(&req.key).map(|s| s.len()).unwrap_or(0);

        if let Err(e) = (app.sv().stats)
          .update_from_telemetry(license.tg_user_id, &stats, active as u32)
          .await
        {
          tracing::warn!("Failed to update stats: {}", e);
        }

        (StatusCode::OK, Json(StatsRes { success: true, message: None }))
      }
      Err(e) => (
        StatusCode::BAD_REQUEST,
        Json(StatsRes {
          success: false,
          message: Some(format!("Invalid stats data: {}", e)),
        }),
      ),
    },
    None => (
      StatusCode::BAD_REQUEST,
      Json(StatsRes {
        success: false,
        message: Some("Invalid base64 encoding".into()),
      }),
    ),
  }
}

pub async fn health() -> &'static str {
  "OK"
}
