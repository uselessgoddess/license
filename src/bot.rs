use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::types::{InputFile, ParseMode};
use teloxide::utils::command::BotCommands;

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

trait BotExt {
  async fn reply_to(
    &self,
    chat_id: ChatId,
    text: impl ToString,
  ) -> ResponseResult<()>;
}

impl BotExt for Bot {
  async fn reply_to(
    &self,
    chat_id: ChatId,
    text: impl ToString,
  ) -> ResponseResult<()> {
    self
      .send_message(chat_id, text.to_string())
      .parse_mode(ParseMode::Html)
      .await?;
    Ok(())
  }
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
      bot.reply_to(msg.chat.id, Command::descriptions()).await?;
    }
    Command::Gen(user_id) => {
      let license = app.create_license(user_id).await;
      let message = license
        .map(|key| format!("Key: <code>{}</code>", key))
        .unwrap_or_else(|err| format!("DB Error: {err}"));
      bot.reply_to(msg.chat.id, message).await?;
    }
    Command::Buy(key, days) => match app.extend_license(&key, days).await {
      Ok(new_exp) => {
        let message = format!(
          "Key extended by {} days.\nNew expiry: <code>{}</code>",
          days, new_exp
        );
        bot.reply_to(msg.chat.id, message).await?
      }
      Err(err) => bot.reply_to(msg.chat.id, format!("Error: {err}")).await?,
    },
    Command::Ban(key) => match app.set_ban(&key, true).await {
      Ok(_) => {
        bot.reply_to(msg.chat.id, "🚫 Key blocked, sessions dropped").await?
      }
      Err(err) => bot.reply_to(msg.chat.id, format!("Error: {err}")).await?,
    },
    Command::Unban(key) => match app.set_ban(&key, false).await {
      Ok(_) => bot.reply_to(msg.chat.id, "✅ Key unblocked").await?,
      Err(e) => bot.reply_to(msg.chat.id, format!("Error: {e}")).await?,
    },
    Command::Info(key) => {
      let active = app.sessions.get(&key).map(|s| s.len()).unwrap_or(0);

      match app.license_info(&key).await {
        Ok(Some(l)) => {
          let status = if l.is_blocked { "⛔ BLOCKED" } else { "Active" };
          let resp = format!(
            "🔑 <b>Key Info</b>\nOwner: <code>{}</code>\nExpires: {}\nStatus: {}\nActive Sessions: {}",
            l.tg_user_id, l.expires_at, status, active
          );
          bot.reply_to(msg.chat.id, resp).await?;
        }
        Ok(None) => bot.reply_to(msg.chat.id, "Key not found").await?,
        Err(e) => bot.reply_to(msg.chat.id, format!("DB Error: {e}")).await?,
      }
    }
    Command::Stats => {
      let active_keys = app.sessions.len();
      let total_sessions: usize =
        app.sessions.iter().map(|e| e.value().len()).sum();

      let message = format!(
        "📊 <b>System Stats</b>\nActive Keys: {}\nTotal Windows: {}",
        active_keys, total_sessions
      );
      bot.reply_to(msg.chat.id, message).await?;
    }
    Command::Backup => {
      if let Err(_) = app.perform_backup(msg.chat.id).await {
        bot.send_document(msg.chat.id, InputFile::file("licenses.db")).await?;
      }
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
    .build()
    .dispatch()
    .await;
}
