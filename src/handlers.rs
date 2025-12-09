use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{NaiveDateTime as DateTime, Utc};
use serde::Serialize;

use crate::model::*;
use crate::state::App;

#[derive(Serialize)]
pub struct Status {
  pub success: bool,
  pub message: Option<String>,
  pub magic_token: Option<i64>,
}

impl Status {
  pub fn ok(magic: i64) -> Self {
    Status { success: true, message: None, magic_token: Some(magic) }
  }

  pub fn invalid(message: impl Into<String>) -> Self {
    Self { success: false, message: Some(message.into()), magic_token: None }
  }
}

fn generate_magic(session_id: &str, secret: &str) -> i64 {
  let combined = format!("{}{}", session_id, secret);
  let mut hash: u64 = 0xcbf29ce484222325;
  for byte in combined.bytes() {
    hash ^= byte as u64;
    hash = hash.wrapping_mul(0x100000001b3);
  }
  hash as i64
}

pub async fn heartbeat(
  State(app): State<Arc<App>>,
  Json(req): Json<HeartbeatReq>,
) -> (StatusCode, Json<Status>) {
  let now = Utc::now().naive_utc();
  let magic = generate_magic(&req.session_id, &app.secret);

  if let Some(mut sessions) = app.sessions.get_mut(&req.key) {
    if let Some(sess) =
      sessions.iter_mut().find(|sess| sess.session_id == req.session_id)
    {
      sess.last_seen = now;
      return (StatusCode::OK, Json(Status::ok(magic)));
    }
  }

  let Ok(Some(license)) =
    sqlx::query_as!(License, "SELECT * FROM licenses WHERE key = ?", req.key)
      .fetch_optional(&app.db)
      .await
  else {
    app.sessions.remove(&req.key);
    return (
      StatusCode::UNAUTHORIZED,
      Json(Status::invalid("Invalid license")),
    );
  };

  if license.is_blocked || license.expires_at < now {
    app.sessions.remove(&req.key);
    return (
      StatusCode::FORBIDDEN,
      Json(Status::invalid("Expired or blocked")),
    );
  }

  let mut entry = app.sessions.entry(req.key.clone()).or_insert_with(Vec::new);

  // thin gc
  entry.retain(|s| (now - s.last_seen).num_seconds() < 60);

  // TODO: configure max value
  if entry.len() >= 5 {
    return (StatusCode::CONFLICT, Json(Status::invalid("Limit reached")));
  } else {
    entry.push(Session { session_id: req.session_id, last_seen: now });
  }

  (StatusCode::OK, Json(Status::ok(magic)))
}
