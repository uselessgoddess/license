//! Application state with dependency injection

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use dashmap::DashMap;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use teloxide::Bot;
use teloxide::prelude::*;
use teloxide::types::InputFile;
use tokio::fs;
use tracing::{debug, error, info};

use crate::migration::Migrator;

/// Session tracking for active connections
#[derive(Debug, Clone)]
pub struct Session {
  pub session_id: String,
  pub hwid_hash: Option<String>,
  pub last_seen: chrono::NaiveDateTime,
}

pub type Sessions = DashMap<String, Vec<Session>>;

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
  pub max_sessions_per_license: i32,
  pub session_timeout_secs: i64,
  pub backup_interval_hours: u64,
  pub builds_directory: String,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      max_sessions_per_license: 5,
      session_timeout_secs: 120,
      backup_interval_hours: 1,
      builds_directory: "./builds".to_string(),
    }
  }
}

/// Main application state
pub struct AppState {
  pub db: DatabaseConnection,
  pub bot: Bot,
  pub admins: HashSet<i64>,
  pub sessions: Sessions,
  pub secret: String,
  pub config: Config,
  // Backup deduplication
  backup_hash: AtomicU64,
}

fn hash_of(bytes: &[u8]) -> u64 {
  let mut hasher = DefaultHasher::new();
  bytes.hash(&mut hasher);
  hasher.finish()
}

impl AppState {
  pub async fn new(
    db_url: &str,
    bot_token: &str,
    admins: HashSet<i64>,
    secret: String,
  ) -> Self {
    Self::with_config(db_url, bot_token, admins, secret, Config::default()).await
  }

  pub async fn with_config(
    db_url: &str,
    bot_token: &str,
    admins: HashSet<i64>,
    secret: String,
    config: Config,
  ) -> Self {
    info!("Connecting to database...");
    let db = Database::connect(db_url).await.expect("Failed to connect to database");

    info!("Running migrations...");
    Migrator::up(&db, None).await.expect("Failed to run migrations");

    Self {
      db,
      sessions: DashMap::new(),
      bot: Bot::new(bot_token),
      admins,
      secret,
      config,
      backup_hash: AtomicU64::new(0),
    }
  }

  /// Perform smart backup (only if DB changed)
  pub async fn perform_smart_backup(&self) -> anyhow::Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("backup_{}.db", timestamp);
    let path = Path::new(&filename);

    if path.exists() {
      let _ = fs::remove_file(path).await;
    }

    // SQLite VACUUM INTO for safe backup
    let query = format!("VACUUM INTO '{}'", filename);
    self
      .db
      .execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        query,
      ))
      .await?;

    let content = fs::read(path).await?;

    let new_hash = hash_of(&content);
    let old_hash = self.backup_hash.load(Ordering::Relaxed);

    self.backup_hash.store(new_hash, Ordering::Relaxed);

    // Skip if unchanged or first run
    if new_hash == old_hash || old_hash == 0 {
      debug!("No changes in DB, skipping backup notification");
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

  /// Force backup to specific chat
  pub async fn perform_backup(&self, chat_id: ChatId) -> anyhow::Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("manual_backup_{}.db", timestamp);

    let query = format!("VACUUM INTO '{}'", filename);
    self
      .db
      .execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        query,
      ))
      .await?;

    let path = Path::new(&filename);
    let _ = self.bot.send_document(chat_id, InputFile::file(path)).await;
    let _ = fs::remove_file(path).await;

    Ok(())
  }

  /// Clean up stale sessions
  pub fn gc_sessions(&self) {
    let now = Utc::now().naive_utc();
    let timeout = self.config.session_timeout_secs;

    self.sessions.retain(|_key, sessions| {
      sessions.retain(|s| (now - s.last_seen).num_seconds() < timeout);
      !sessions.is_empty()
    });
  }

  /// Drop all sessions for a license key
  pub fn drop_sessions(&self, key: &str) {
    self.sessions.remove(key);
  }
}

/// Promo result enum for backwards compatibility
#[derive(Debug)]
pub enum Promo {
  Key(String),
  Err(&'static str),
}
