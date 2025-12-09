use std::collections::HashSet;
use std::path::Path;

use dashmap::DashMap;
use sqlx::SqlitePool;
use teloxide::Bot;
use teloxide::prelude::*;
use teloxide::types::InputFile;
use tokio::fs;

use crate::model::Session;

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
}
