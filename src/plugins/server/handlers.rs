use std::{path::Path, sync::Arc};

use axum::{
  Json,
  body::Body,
  extract::{Query, State},
  http::{StatusCode, header},
  response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

use crate::{
  prelude::*,
  state::{AppState, Session},
};

#[derive(Debug, Deserialize)]
pub struct HeartbeatReq {
  pub key: String,
  pub machine_id: String,
  pub session_id: String,
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

  (StatusCode::OK, Json(HeartbeatRes::ok(magic)))
}

#[derive(Debug, Deserialize)]
pub struct MetricsReq {
  pub stats: String,
}

pub async fn submit_metrics(
  State(app): State<Arc<AppState>>,
  Json(req): Json<MetricsReq>,
) -> Result<()> {
  app.sv().stats.process_metric(&req.stats).await?;
  Ok(())
}

pub async fn health() -> &'static str {
  "OK"
}

#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
  pub token: String,
}

pub async fn download(
  State(app): State<Arc<AppState>>,
  Query(query): Query<DownloadQuery>,
) -> impl IntoResponse {
  let version = match app.validate_download_token(&query.token) {
    Some(v) => v,
    None => {
      return Err((
        StatusCode::UNAUTHORIZED,
        "Invalid or expired download token",
      ));
    }
  };

  let build = match app.sv().build.by_version(&version).await {
    Ok(Some(b)) if b.is_active => b,
    _ => {
      return Err((StatusCode::NOT_FOUND, "Build not found"));
    }
  };

  let path = Path::new(&build.file_path);
  if !path.exists() {
    return Err((StatusCode::NOT_FOUND, "Build file not found"));
  }

  let file = match tokio::fs::File::open(path).await {
    Ok(f) => f,
    Err(_) => {
      return Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to open file"));
    }
  };

  let filename = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("download.bin")
    .to_string();

  let stream = ReaderStream::new(file);
  let body = Body::from_stream(stream);

  // Increment download counter
  let _ = app.sv().build.increment_downloads(&version).await;

  let headers = [
    (header::CONTENT_TYPE, "application/octet-stream".to_string()),
    (
      header::CONTENT_DISPOSITION,
      format!("attachment; filename=\"{}\"", filename),
    ),
  ];

  Ok((headers, body))
}
