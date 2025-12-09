use std::collections::HashSet;
use std::path::Path;

use dashmap::DashMap;
use sqlx::SqlitePool;
use teloxide::Bot;
use teloxide::prelude::*;
use teloxide::types::InputFile;
use tokio::fs;

use crate::model::{License, Session};
use crate::{DateTime, Duration, Utc};

pub type Sessions = DashMap<String, Vec<Session>>;

#[derive(Clone)]
pub struct App {
  pub db: SqlitePool,
  pub bot: Bot,
  pub admins: HashSet<i64>,
  pub sessions: Sessions,
  pub secret: String,
}

impl App {
  pub async fn new(
    db_url: &str,
    bot_token: &str,
    admins: HashSet<i64>,
    secret: String,
  ) -> Self {
    let db = SqlitePool::connect(db_url).await.expect("DB fail");

    sqlx::migrate!("./migrations").run(&db).await.expect("Migrations failed");

    Self {
      db,
      sessions: DashMap::new(),
      bot: Bot::new(bot_token),
      admins,
      secret,
    }
  }

  pub async fn perform_backup(
    &self,
    chat_id: teloxide::types::ChatId,
  ) -> anyhow::Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("backup_{}.db", timestamp);

    let query = format!("VACUUM INTO '{}'", filename);
    sqlx::query(&query).execute(&self.db).await?;

    let path = Path::new(&filename);
    let doc = InputFile::file(path);
    self.bot.send_document(chat_id, doc).await?;

    let _ = fs::remove_file(path).await;

    Ok(())
  }

  pub async fn create_license(&self, user_id: i64) -> sqlx::Result<String> {
    let key = uuid::Uuid::new_v4().to_string();
    let exp = Utc::now().naive_utc();

    sqlx::query!(
      "INSERT INTO licenses (key, tg_user_id, expires_at) VALUES (?, ?, ?)",
      key,
      user_id,
      exp
    )
    .execute(&self.db)
    .await?;

    Ok(key)
  }

  pub async fn extend_license(
    &self,
    key: &str,
    days: i64,
  ) -> Result<DateTime, String> {
    let mut tx = self.db.begin().await.map_err(|e| e.to_string())?;

    let Some(rec) =
      sqlx::query!("SELECT expires_at FROM licenses WHERE key = ?", key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?
    else {
      return Err("Key not found".to_string());
    };

    let now = Utc::now().naive_utc();

    let base_time = if rec.expires_at < now { now } else { rec.expires_at };
    let new_exp = base_time + Duration::from_hours(24 * days as u64);

    sqlx::query!(
      "UPDATE licenses SET expires_at = ?, is_blocked = FALSE WHERE key = ?",
      new_exp,
      key
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(new_exp)
  }

  pub async fn set_ban(&self, key: &str, blocked: bool) -> sqlx::Result<()> {
    sqlx::query!(
      "UPDATE licenses SET is_blocked = ? WHERE key = ?",
      blocked,
      key
    )
    .execute(&self.db)
    .await?;

    if blocked {
      self.sessions.remove(key);
    }

    Ok(())
  }

  pub async fn license_info(&self, key: &str) -> sqlx::Result<Option<License>> {
    sqlx::query_as!(License, "SELECT * FROM licenses WHERE key = ?", key)
      .fetch_optional(&self.db)
      .await
  }
}
