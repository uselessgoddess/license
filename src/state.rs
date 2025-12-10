use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use teloxide::Bot;
use teloxide::prelude::*;
use teloxide::types::InputFile;
use tokio::fs;

use crate::model::{License, Session};
use crate::prelude::*;

pub type Sessions = DashMap<String, Vec<Session>>;

pub struct App {
  pub db: SqlitePool,
  pub bot: Bot,
  pub admins: HashSet<i64>,
  pub sessions: Sessions,
  pub secret: String,
  // meta
  pub backup_hash: AtomicU64,
}

fn hash_of(bytes: &[u8]) -> u64 {
  let mut hasher = DefaultHasher::new();
  bytes.hash(&mut hasher);
  hasher.finish()
}

impl App {
  pub async fn new(
    db_url: &str,
    bot_token: &str,
    admins: HashSet<i64>,
    secret: String,
  ) -> Self {
    let options = SqliteConnectOptions::from_str(db_url)
      .expect("invalid `DATABASE_URL`")
      .create_if_missing(true)
      .journal_mode(SqliteJournalMode::Wal);

    info!("Connecting to database...");
    let db = SqlitePool::connect_with(options).await.expect("DB fail");

    info!("Running migrations...");
    sqlx::migrate!("./migrations").run(&db).await.expect("Migrations failed");

    Self {
      db,
      sessions: DashMap::new(),
      bot: Bot::new(bot_token),
      admins,
      secret,
      backup_hash: AtomicU64::new(0),
    }
  }

  pub async fn perform_smart_backup(&self) -> anyhow::Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("backup_{}.db", timestamp);
    let path = Path::new(&filename);

    if path.exists() {
      let _ = fs::remove_file(path).await;
    }

    let query = format!("VACUUM INTO '{}'", filename);
    sqlx::query(&query).execute(&self.db).await?;

    let content = fs::read(path).await?;

    let new_hash = hash_of(&content);
    let old_hash = self.backup_hash.load(Ordering::Relaxed);

    self.backup_hash.store(new_hash, Ordering::Relaxed);

    // FIXME: 0 is hardcoded as fresh start hash
    if new_hash == old_hash || old_hash == 0 /* fresh start */ {
      debug!("No changes in DB, skipping backup");
    } else {
      for &admin in self.admins.iter() {
        let doc = InputFile::file(path);
        let caption = format!(
          "📦 <b>Database Backup</b>\nChanges detected.\nTime: {}",
          timestamp
        );

        let _ = self
          .bot
          .send_document(ChatId(admin), doc)
          .caption(caption)
          .parse_mode(teloxide::types::ParseMode::Html)
          .await;
      }
    }
    let _ = fs::remove_file(path).await;

    Ok(())
  }

  pub async fn perform_backup(
    &self,
    chat_id: teloxide::types::ChatId,
  ) -> anyhow::Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("manual_backup_{}.db", timestamp);

    sqlx::query(&format!("VACUUM INTO '{}'", filename))
      .execute(&self.db)
      .await?;

    let path = Path::new(&filename);
    let _ = self.bot.send_document(chat_id, InputFile::file(path)).await;
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
