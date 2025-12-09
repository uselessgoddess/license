use chrono::NaiveDateTime as DateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct License {
  /// License key
  pub key: String,
  /// Telegram id of license owner
  pub tg_user_id: i64,
  pub expires_at: DateTime,
  pub is_blocked: bool,
}

#[derive(Debug, Clone)]
pub struct Session {
  pub session_id: String,
  pub last_seen: DateTime,
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatReq {
  pub key: String,
  #[allow(dead_code)]
  pub machine_id: String,
  pub session_id: String,
}
