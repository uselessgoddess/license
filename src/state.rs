use std::{
  collections::HashSet,
  hash::{DefaultHasher, Hash, Hasher},
  path::Path,
  sync::atomic::{AtomicU64, Ordering},
};

use migration::Migrator;
use teloxide::{
  Bot,
  prelude::*,
  types::{InputFile, ParseMode},
};
use tokio::fs;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{entity::license, prelude::*, sv};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Session {
  pub session_id: String,
  pub hwid_hash: Option<String>,
  pub last_seen: DateTime,
}

pub type Sessions = DashMap<String, Vec<Session>>;

/// Banned session stored in DashMap with expiry (for recently logged out sessions)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BannedSession {
  pub key: String,
  pub banned_at: DateTime,
}

/// Maps session_id to BannedSession
pub type BannedSessions = DashMap<String, BannedSession>;

/// Download token stored in DashMap with expiry
#[derive(Debug, Clone)]
pub struct DownloadToken {
  pub version: String,
  pub created_at: DateTime,
}

pub type DownloadTokens = DashMap<String, DownloadToken>;

#[derive(Debug, Clone)]
pub struct Config {
  pub builds_directory: String,
  pub session_lifetime: i64,
  pub banned_session_lifetime: i64,
  pub backup_hours: u64,
  pub download_token_lifetime: i64,
  pub base_url: String,
  pub gc_min_free_space: u64,
  pub gc_check_interval_secs: u64,
  pub proxy_port: u16,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      builds_directory: String::from("./builds"),
      session_lifetime: 120,
      banned_session_lifetime: 30 * 60,
      backup_hours: 1,
      download_token_lifetime: 10 * 60,
      base_url: String::from("http://localhost:3000"),
      gc_min_free_space: 500 * 1024 * 1024, // 500MB
      gc_check_interval_secs: 60,
      proxy_port: 1080,
    }
  }
}

#[allow(dead_code)]
pub struct Services<'a> {
  pub user: sv::User<'a>,
  pub stats: sv::Stats<'a>,
  pub build: sv::Build<'a>,
  pub license: sv::License<'a>,
  pub steam: sv::Steam<'a>,
  pub referral: sv::Referral<'a>,
  pub balance: sv::Balance<'a>,
  pub payment: sv::Payment<'a>,
  pub cryptobot: Option<&'a sv::cryptobot::CryptoBot>,
}

pub struct AppState {
  pub db: DatabaseConnection,
  pub bot: Bot,
  pub admins: HashSet<i64>,
  // TODO: replace this dashmaps with custom wrappers that stores time of expiration
  pub sessions: Sessions,
  pub banned_sessions: BannedSessions,
  pub download_tokens: DownloadTokens,
  pub secret: String,
  pub config: Config,
  pub cryptobot: Option<sv::cryptobot::CryptoBot>,
  // Backup deduplication
  backup_hash: AtomicU64,
  // proxy
  pub active_proxy_sessions: DashMap<String, usize>,
}

// TODO: we need to transactions too
fn hash_licenses(licenses: &[license::Model]) -> u64 {
  let mut hasher = DefaultHasher::new();
  for lic in licenses {
    lic.key.hash(&mut hasher);
    lic.tg_user_id.hash(&mut hasher);
    lic.is_blocked.hash(&mut hasher);
    lic.expires_at.and_utc().timestamp().hash(&mut hasher);
    lic.max_sessions.hash(&mut hasher);
  }
  hasher.finish()
}

impl AppState {
  #[allow(dead_code)]
  pub async fn new(
    db_url: &str,
    bot_token: &str,
    admins: HashSet<i64>,
    secret: String,
  ) -> Self {
    Self::with_config(
      db_url,
      bot_token,
      admins,
      secret,
      Config::default(),
      None,
    )
    .await
  }

  pub async fn with_config(
    db_url: &str,
    bot_token: &str,
    admins: HashSet<i64>,
    secret: String,
    config: Config,
    cryptobot: Option<sv::cryptobot::CryptoBot>,
  ) -> Self {
    info!("Connecting to database...");
    let db =
      Database::connect(db_url).await.expect("Failed to connect to database");

    info!("Running migrations...");
    Migrator::up(&db, None).await.expect("Failed to run migrations");

    Self {
      db,
      sessions: DashMap::new(),
      banned_sessions: DashMap::new(),
      download_tokens: DashMap::new(),
      bot: Bot::new(bot_token),
      admins,
      secret,
      config,
      cryptobot,
      backup_hash: AtomicU64::new(0),
      active_proxy_sessions: DashMap::new(),
    }
  }

  pub fn sv(&self) -> Services<'_> {
    Services {
      user: sv::User::new(&self.db),
      stats: sv::Stats::new(&self.db),
      build: sv::Build::new(&self.db),
      license: sv::License::new(&self.db),
      steam: sv::Steam::new(&self.db),
      referral: sv::Referral::new(&self.db),
      balance: sv::Balance::new(&self.db),
      payment: sv::Payment::new(&self.db),
      cryptobot: self.cryptobot.as_ref(),
    }
  }

  /// Perform backup only when license data changes.
  /// Changes in metrics/stats tables are not a reason to backup.
  pub async fn perform_smart_backup(&self) -> anyhow::Result<()> {
    // Hash only license data - stats/metrics changes don't trigger backups
    let licenses = license::Entity::find()
      .order_by_asc(license::Column::Key)
      .all(&self.db)
      .await?;

    let new_hash = hash_licenses(&licenses);
    let old_hash = self.backup_hash.load(Ordering::Relaxed);

    self.backup_hash.store(new_hash, Ordering::Relaxed);

    // Skip backup if no license changes (or first run)
    if new_hash == old_hash || old_hash == 0 {
      debug!("No license changes, skipping backup");
      return Ok(());
    }

    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("backup_{}.db", timestamp);
    let path = Path::new(&filename);

    if path.exists() {
      let _ = fs::remove_file(path).await;
    }

    let query = format!("VACUUM INTO '{}'", filename);
    self
      .db
      .execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        query,
      ))
      .await?;

    for &admin in self.admins.iter() {
      let doc = InputFile::file(path);
      let caption = format!(
        "📦 <b>Database Backup</b>\nLicense changes detected.\nTime: {}",
        timestamp
      );

      let _ = self
        .bot
        .send_document(ChatId(admin), doc)
        .caption(caption)
        .parse_mode(ParseMode::Html)
        .await;
    }

    let _ = fs::remove_file(path).await;
    Ok(())
  }

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

  pub fn gc_sessions(&self) {
    let now = Utc::now().naive_utc();
    let timeout = self.config.session_lifetime;

    self.sessions.retain(|_key, sessions| {
      sessions.retain(|s| (now - s.last_seen).num_seconds() < timeout);
      !sessions.is_empty()
    });
  }

  pub fn drop_sessions(&self, key: &str) {
    self.sessions.remove(key);
  }

  pub fn logout_session(&self, key: &str, session_id: &str) -> bool {
    let now = Utc::now().naive_utc();

    let mut removed = false;
    if let Some(mut sessions) = self.sessions.get_mut(key) {
      let initial_len = sessions.len();
      sessions.retain(|s| s.session_id != session_id);
      removed = sessions.len() < initial_len;

      if sessions.is_empty() {
        drop(sessions);
        self.sessions.remove(key);
      }
    }

    if removed {
      self.banned_sessions.insert(
        session_id.to_string(),
        BannedSession { key: key.to_string(), banned_at: now },
      );
    }

    removed
  }

  pub fn is_session_banned(&self, session_id: &str) -> bool {
    let now = Utc::now().naive_utc();
    let timeout = self.config.banned_session_lifetime;

    if let Some(banned) = self.banned_sessions.get(session_id) {
      return (now - banned.banned_at).num_seconds() < timeout;
    }
    false
  }

  pub fn gc_banned_sessions(&self) {
    let now = Utc::now().naive_utc();
    let timeout = self.config.banned_session_lifetime;

    self
      .banned_sessions
      .retain(|_, bs| (now - bs.banned_at).num_seconds() < timeout);
  }

  pub fn create_download_token(&self, version: &str) -> String {
    let token = Uuid::new_v4().to_string();
    let now = Utc::now().naive_utc();
    self.download_tokens.insert(
      token.clone(),
      DownloadToken { version: version.to_string(), created_at: now },
    );
    token
  }

  pub fn validate_download_token(&self, token: &str) -> Option<String> {
    let now = Utc::now().naive_utc();
    let timeout = self.config.download_token_lifetime;

    if let Some(dt) = self.download_tokens.get(token)
      && (now - dt.created_at).num_seconds() < timeout
    {
      return Some(dt.version.clone());
    }
    None
  }

  pub fn gc_download_tokens(&self) {
    let now = Utc::now().naive_utc();
    let timeout = self.config.download_token_lifetime;

    self
      .download_tokens
      .retain(|_, dt| (now - dt.created_at).num_seconds() < timeout);
  }
}
