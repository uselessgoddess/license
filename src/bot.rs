use std::sync::Arc;

use chrono::{Duration, Utc};
use teloxide::prelude::*;
use teloxide::types::{InputFile, ParseMode};
use teloxide::utils::command::BotCommands;
use uuid::Uuid;

use crate::state::App;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
  #[command(description = "help")]
  Help,
  #[command(description = "gen <user_id> (generate expired key)")]
  Gen(i64),
  #[command(
    description = "buy <key> <days> (extend license time)",
    parse_with = "split"
  )]
  Buy(String, i64),
  #[command(description = "ban <key>")]
  Ban(String),
  #[command(description = "unban <key>")]
  Unban(String),
  #[command(description = "info <key>")]
  Info(String),
  #[command(description = "server stats")]
  Stats,
  #[command(description = "force backup database")]
  Backup,
}

async fn update(
  app: Arc<App>,
  bot: Bot,
  msg: Message,
  cmd: Command,
) -> ResponseResult<()> {
  if !app.admins.contains(&msg.chat.id.0) {
    return Ok(());
  }

  let _ = bot.set_my_commands(Command::bot_commands()).await;

  match cmd {
    Command::Help => {
      bot
        .send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    }
    Command::Gen(user_id) => {
      let key = Uuid::new_v4().to_string();
      let exp = Utc::now().naive_utc();

      let insert = sqlx::query!(
        "INSERT INTO licenses (key, tg_user_id, expires_at) VALUES (?, ?, ?)",
        key,
        user_id,
        exp
      )
      .execute(&app.db)
      .await;

      if let Err(err) = insert {
        bot.send_message(msg.chat.id, format!("Error: {err:?}")).await?
      } else {
        bot
          .send_message(msg.chat.id, format!("Key: <code>{}</code>", key))
          .parse_mode(ParseMode::Html)
          .await?
      };
    }
    Command::Buy(key, days) => {
      let mut tx = match app.db.begin().await {
        Ok(tx) => tx,
        Err(err) => {
          return {
            bot.send_message(msg.chat.id, format!("DB Error: {err}")).await?;
            Ok(())
          };
        }
      };

      let license =
        sqlx::query!("SELECT expires_at FROM licenses WHERE key = ?", key)
          .fetch_optional(&mut *tx)
          .await;

      match license {
        Ok(Some(rec)) => {
          let now = Utc::now().naive_utc();
          let current_exp = rec.expires_at;

          let base_time = if current_exp < now { now } else { current_exp };
          let new_exp = base_time + Duration::days(days);

          let update = sqlx::query!(
            "UPDATE licenses SET expires_at = ?, is_blocked = FALSE WHERE key = ?",
            new_exp, key
          )
            .execute(&mut *tx)
            .await;

          if let Err(e) = update {
            bot
              .send_message(msg.chat.id, format!("Update failed: {e}"))
              .await?;
          } else {
            tx.commit().await.unwrap();
            bot
              .send_message(
                msg.chat.id,
                format!(
                  "Key extended by {} days.\nNew expiry: <code>{}</code>",
                  days, new_exp
                ),
              )
              .parse_mode(ParseMode::Html)
              .await?;
          }
        }
        Ok(None) => {
          bot.send_message(msg.chat.id, "❌ Key not found").await?;
        }
        Err(e) => {
          bot.send_message(msg.chat.id, format!("Error: {e}")).await?;
        }
      }
    }
    Command::Backup => {
      if let Err(err) = app.perform_backup(msg.chat.id).await {
        let _ =
          bot.send_document(msg.chat.id, InputFile::file("licenses.db")).await;
        bot
          .send_message(msg.chat.id, format!("Backup failed: {err:?}"))
          .await?;
      }
    }
    Command::Ban(key) => {
      let _res = sqlx::query!(
        "UPDATE licenses SET is_blocked = TRUE WHERE key = ?",
        key
      )
      .execute(&app.db)
      .await;
      app.sessions.remove(&key);
      bot
        .send_message(msg.chat.id, "🚫 Key blocked and sessions dropped")
        .await?;
    }
    Command::Unban(key) => {
      let _ = sqlx::query!(
        "UPDATE licenses SET is_blocked = FALSE WHERE key = ?",
        key
      )
      .execute(&app.db)
      .await;
      bot.send_message(msg.chat.id, "✅ Key unblocked").await?;
    }
    Command::Info(key) => {
      let active = if let Some(sessions) = app.sessions.get(&key) {
        sessions.len()
      } else {
        0
      };

      let lic = sqlx::query!(
        "SELECT tg_user_id, expires_at, is_blocked FROM licenses WHERE key = ?",
        key
      )
      .fetch_optional(&app.db)
      .await;

      match lic {
        Ok(Some(l)) => {
          let status = if l.is_blocked { "⛔ BLOCKED" } else { "Active" };
          let response = format!(
            "🔑 <b>Key Info</b>\nOwner: <code>{}</code>\nExpires: {}\nStatus: {}\nActive Sessions: {}",
            l.tg_user_id, l.expires_at, status, active
          );
          bot
            .send_message(msg.chat.id, response)
            .parse_mode(ParseMode::Html)
            .await?;
        }
        _ => {
          bot.send_message(msg.chat.id, "Key not found").await?;
        }
      }
    }
    Command::Stats => {
      let active_keys = app.sessions.len();
      let total_sessions: usize =
        app.sessions.iter().map(|entry| entry.value().len()).sum();

      bot.send_message(
                msg.chat.id,
                format!(
                    "📊 <b>System Stats</b>\nActive Keys Online: {}\nTotal Sessions (Windows): {}",
                    active_keys, total_sessions
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
    }
  };
  respond(())
}

pub async fn run_bot(app: Arc<App>) {
  let bot = app.bot.clone();
  let handler = Update::filter_message().filter_command::<Command>().endpoint(
    move |bot: Bot, msg: Message, cmd: Command| {
      let app = app.clone();
      update(app, bot, msg, cmd)
    },
  );

  Dispatcher::builder(bot, handler)
    .enable_ctrlc_handler()
    .build()
    .dispatch()
    .await;
}
