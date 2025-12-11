use teloxide::prelude::*;
use teloxide::types::{InputFile, ParseMode};
use teloxide::utils::command::BotCommands;

use crate::prelude::*;
use crate::state::{App, Promo};

fn date(date: DateTime) -> impl std::fmt::Display {
  date.format("%d.%m.%Y %H:%M")
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
  // --- PUBLIC COMMANDS ---
  Start,
  FreeWeek,
  MyKey,
  Help,

  // --- ADMIN COMMANDS ---
  Gen(i64),
  #[command(parse_with = "split")]
  Buy(String, i64),
  Ban(String),
  Unban(String),
  Info(String),
  Stats,
  Backup,
}

trait BotExt {
  async fn reply_to(
    &self,
    chat_id: ChatId,
    text: impl ToString,
  ) -> ResponseResult<()>;

  async fn infer_username(&self, chat_id: ChatId) -> String;
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

  async fn infer_username(&self, chat_id: ChatId) -> String {
    match self.get_chat(chat_id).await {
      Ok(chat) => {
        if let Some(username) = chat.username() {
          format!("@{}", username)
        } else {
          format!("tg://user?id={}\">", chat_id)
        }
      }
      Err(_) => {
        format!("<code>{}</code> (API Error)", chat_id)
      }
    }
  }
}

fn help_text(admin: bool) -> String {
  let mut text = String::from("<b>YACSP Panel</b>\n\n");

  text.push_str("/start - Start bot\n");
  text.push_str("/freeweek - 🎁 Try week for free!\n");
  text.push_str("/mykey - Your licenses\n");
  text.push_str("/help - Show this menu\n");

  if admin {
    text.push_str("\n<b>Admin Commands:</b>\n");
    text.push_str("/gen <code>id</code> <code>days?</code> - generate key\n");
    text.push_str("/buy <code>key</code> <code>days</code> - extend key\n");
    text.push_str("/ban <code>key</code> - block key\n");
    text.push_str("/unban <code>key</code> - unblock key\n");
    text.push_str("/info <code>key</code> - key info\n");
    text.push_str("/stats - server stats\n");
    text.push_str("/backup - force backup db\n");
  }

  text
}

async fn update(
  app: Arc<App>,
  bot: Bot,
  msg: Message,
  cmd: Command,
) -> ResponseResult<()> {
  let user_id = msg.chat.id.0;
  let is_admin = app.admins.contains(&user_id);

  match cmd {
    Command::Start => {
      bot
        .reply_to(
          msg.chat.id,
          "Welcome to YACSP! Type /help to see commands.\nContact us here: @y_a_c_s_p",
        )
        .await?;
    }
    Command::Help => {
      let text = help_text(is_admin);
      error!(text);
      bot.reply_to(msg.chat.id, text).await?;
    }
    Command::MyKey => {
      let now = Utc::now().naive_utc();

      if let Some(mut keys) = app.keys_of(user_id).await
        && !keys.is_empty()
      {
        let mut msg_text = String::from("🔑 <b>Your Keys:</b>\n");
        keys.sort_by_key(|license| license.expires_at < now);

        for key in keys {
          let expire = if key.expires_at > now {
            // TODO: use function
            let duration = key.expires_at - now;
            let days = duration.num_days();
            let hours = duration.num_hours() % 24;
            let minutes = duration.num_minutes() % 60;

            format!("Time left: {days}d {hours}h {minutes}m")
          } else {
            "Expired!".to_string()
          };
          msg_text.push_str(&format!("\n<code>{}</code>\n{expire}", key.key,));
        }
        bot.reply_to(msg.chat.id, msg_text).await?;
      } else {
        bot.reply_to(msg.chat.id, "You have no active keys.").await?
      }
    }
    Command::FreeWeek => {
      let promo_name = "first_promo"; // i swear

      if let Ok(promo) = app.claim_promo(user_id, promo_name).await {
        match promo {
          Promo::Key(key) => {
            let message = format!(
              "🎉 <b>Success!</b>\nHere is your FREE week license:\n \
              <code>{key}</code>\n\n \
              Download software here: ..."
            );
            bot.reply_to(msg.chat.id, message).await?;
          }
          Promo::Err(err) => bot.reply_to(msg.chat.id, err).await?,
        }
      }
    }
    _ => {}
  }

  if is_admin {
    let _ = bot.set_my_commands(Command::bot_commands()).await;
    admin_space(app, bot, msg, cmd).await?;
  }

  Ok(())
}

async fn admin_space(
  app: Arc<App>,
  bot: Bot,
  msg: Message,
  cmd: Command,
) -> ResponseResult<()> {
  match cmd {
    Command::Gen(user_id) => {
      let license = app.create_license(user_id, 0).await;
      let message = license
        .map(|key| format!("<code>{}</code>", key))
        .unwrap_or_else(|err| format!("DB Error: {err}"));
      bot.reply_to(msg.chat.id, message).await?;
    }
    Command::Buy(key, days) => match app.extend_license(&key, days).await {
      Ok(new_exp) => {
        let message = format!(
          "Key extended by {} days.\nNew expiry: <code>{}</code>",
          days,
          date(new_exp)
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
        Ok(Some(license)) => {
          let status =
            if license.is_blocked { "⛔ BLOCKED" } else { "Active" };
          let username = bot.infer_username(ChatId(license.tg_user_id)).await;

          let resp = format!(
            "🔑 <b>Key Info</b>\nOwner: {}\nExpires: {}\nStatus: {}\nActive Sessions: {}",
            username,
            date(license.expires_at),
            status,
            active
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
    _ => {}
  };

  Ok(())
}

pub async fn run_bot(app: Arc<App>) {
  let bot = app.bot.clone();
  let handler = Update::filter_message().filter_command::<Command>().endpoint(
    move |bot: Bot, msg: Message, cmd: Command| {
      let app = app.clone();
      update(app, bot, msg, cmd)
    },
  );

  Dispatcher::builder(bot, handler).build().dispatch().await;
}
