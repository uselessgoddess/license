//! Telegram bot with interactive inline keyboards

use std::path::Path;
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use teloxide::prelude::*;
use teloxide::types::{
  InlineKeyboardButton, InlineKeyboardMarkup, InputFile, MessageId, ParseMode,
};
use teloxide::utils::command::BotCommands;
use tracing::{error, info};

use crate::entities::license::LicenseType;
use crate::services::{BuildService, LicenseService, StatsService, UserService};
use crate::state::AppState;

fn format_date(date: NaiveDateTime) -> String {
  date.format("%d.%m.%Y %H:%M").to_string()
}

fn format_duration(expires_at: NaiveDateTime, now: NaiveDateTime) -> String {
  if expires_at <= now {
    return "Expired!".to_string();
  }
  let duration = expires_at - now;
  let days = duration.num_days();
  let hours = duration.num_hours() % 24;
  let minutes = duration.num_minutes() % 60;
  format!("{}d {}h {}m", days, hours, minutes)
}

// Callback data prefixes
const CB_PROFILE: &str = "profile";
const CB_LICENSE: &str = "license";
const CB_TRIAL: &str = "trial";
const CB_DOWNLOAD: &str = "download";
const CB_SUPPORT: &str = "support";
const CB_BACK: &str = "back";
const CB_ADMIN: &str = "admin";
const CB_STATS: &str = "stats";
const CB_BACKUP: &str = "backup";

/// Build the main menu keyboard
fn main_menu_keyboard(is_admin: bool) -> InlineKeyboardMarkup {
  let mut rows = vec![
    vec![InlineKeyboardButton::callback("👤 My Profile", CB_PROFILE)],
    vec![InlineKeyboardButton::callback("🔑 My License", CB_LICENSE)],
    vec![InlineKeyboardButton::callback("🆓 Get Free Trial", CB_TRIAL)],
    vec![InlineKeyboardButton::callback("📥 Download Panel", CB_DOWNLOAD)],
    vec![InlineKeyboardButton::callback("🆘 Support", CB_SUPPORT)],
  ];

  if is_admin {
    rows.push(vec![InlineKeyboardButton::callback("🔧 Admin Panel", CB_ADMIN)]);
  }

  InlineKeyboardMarkup::new(rows)
}

/// Build the admin panel keyboard
fn admin_keyboard() -> InlineKeyboardMarkup {
  InlineKeyboardMarkup::new(vec![
    vec![InlineKeyboardButton::callback("📊 Server Stats", CB_STATS)],
    vec![InlineKeyboardButton::callback("📦 Backup DB", CB_BACKUP)],
    vec![InlineKeyboardButton::callback("« Back to Menu", CB_BACK)],
  ])
}

/// Build back button keyboard
fn back_keyboard() -> InlineKeyboardMarkup {
  InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
    "« Back to Menu",
    CB_BACK,
  )]])
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
  Start,
  Help,
  MyKey,
  FreeWeek,
  // Admin commands
  Gen(String),
  #[command(parse_with = "split")]
  Buy { key: String, days: i64 },
  Ban(String),
  Unban(String),
  Info(String),
  Stats,
  Backup,
}

trait BotExt {
  async fn reply_html(&self, chat_id: ChatId, text: impl Into<String>) -> ResponseResult<Message>;

  async fn reply_with_keyboard(
    &self,
    chat_id: ChatId,
    text: impl Into<String>,
    keyboard: InlineKeyboardMarkup,
  ) -> ResponseResult<Message>;

  async fn edit_with_keyboard(
    &self,
    chat_id: ChatId,
    message_id: MessageId,
    text: impl Into<String>,
    keyboard: InlineKeyboardMarkup,
  ) -> ResponseResult<()>;

  async fn infer_username(&self, chat_id: ChatId) -> String;
}

impl BotExt for Bot {
  async fn reply_html(&self, chat_id: ChatId, text: impl Into<String>) -> ResponseResult<Message> {
    self
      .send_message(chat_id, text.into())
      .parse_mode(ParseMode::Html)
      .await
  }

  async fn reply_with_keyboard(
    &self,
    chat_id: ChatId,
    text: impl Into<String>,
    keyboard: InlineKeyboardMarkup,
  ) -> ResponseResult<Message> {
    self
      .send_message(chat_id, text.into())
      .parse_mode(ParseMode::Html)
      .reply_markup(keyboard)
      .await
  }

  async fn edit_with_keyboard(
    &self,
    chat_id: ChatId,
    message_id: MessageId,
    text: impl Into<String>,
    keyboard: InlineKeyboardMarkup,
  ) -> ResponseResult<()> {
    self
      .edit_message_text(chat_id, message_id, text.into())
      .parse_mode(ParseMode::Html)
      .reply_markup(keyboard)
      .await?;
    Ok(())
  }

  async fn infer_username(&self, chat_id: ChatId) -> String {
    match self.get_chat(chat_id).await {
      Ok(chat) => {
        if let Some(username) = chat.username() {
          format!("@{}", username)
        } else {
          format!("<a href=\"tg://user?id={}\">User</a>", chat_id)
        }
      }
      Err(_) => format!("<code>{}</code> (API Error)", chat_id),
    }
  }
}

fn help_text(is_admin: bool) -> String {
  let mut text = String::from("<b>🎮 YACS Panel</b>\n\n");
  text.push_str("<b>Commands:</b>\n");
  text.push_str("/start - Open main menu\n");
  text.push_str("/freeweek - Get free trial\n");
  text.push_str("/mykey - View your licenses\n");
  text.push_str("/help - Show this help\n");

  if is_admin {
    text.push_str("\n<b>Admin Commands:</b>\n");
    text.push_str("/gen <code>user_id</code> [days] - Generate key\n");
    text.push_str("/buy <code>key</code> <code>days</code> - Extend key\n");
    text.push_str("/ban <code>key</code> - Block key\n");
    text.push_str("/unban <code>key</code> - Unblock key\n");
    text.push_str("/info <code>key</code> - Key info\n");
    text.push_str("/stats - Server statistics\n");
    text.push_str("/backup - Force backup\n");
  }

  text
}

async fn handle_command(
  app: Arc<AppState>,
  bot: Bot,
  msg: Message,
  cmd: Command,
) -> ResponseResult<()> {
  let user_id = msg.chat.id.0;
  let is_admin = app.admins.contains(&user_id);

  // Auto-register user on any command
  let username = msg
    .from
    .as_ref()
    .and_then(|u| u.username.clone());
  let _ = UserService::get_or_create(&app.db, user_id, username).await;

  match cmd {
    Command::Start => {
      let text = "👋 <b>Welcome to YACS Panel!</b>\n\n\
        Use the buttons below to navigate.\n\
        Contact support: @y_a_c_s_p";
      bot
        .reply_with_keyboard(msg.chat.id, text, main_menu_keyboard(is_admin))
        .await?;
    }

    Command::Help => {
      bot
        .reply_with_keyboard(msg.chat.id, help_text(is_admin), back_keyboard())
        .await?;
    }

    Command::MyKey => {
      handle_license_view(&app, &bot, msg.chat.id).await?;
    }

    Command::FreeWeek => {
      handle_trial_claim(&app, &bot, msg.chat.id, user_id).await?;
    }

    // Admin commands
    Command::Gen(args) => {
      if !is_admin {
        return Ok(());
      }
      let parts: Vec<&str> = args.split_whitespace().collect();
      let (target_user, days) = match parts.as_slice() {
        [user_id] => (user_id.parse::<i64>().ok(), 0u64),
        [user_id, days] => (user_id.parse::<i64>().ok(), days.parse::<u64>().unwrap_or(0)),
        _ => (None, 0),
      };

      let Some(target_user) = target_user else {
        bot.reply_html(msg.chat.id, "❌ Usage: /gen <user_id> [days]").await?;
        return Ok(());
      };

      match LicenseService::create(&app.db, target_user, days, LicenseType::Pro).await {
        Ok(license) => {
          bot
            .reply_html(msg.chat.id, format!("✅ Key created:\n<code>{}</code>", license.key))
            .await?;
        }
        Err(e) => {
          bot.reply_html(msg.chat.id, format!("❌ Error: {}", e)).await?;
        }
      }
    }

    Command::Buy { key, days } => {
      if !is_admin {
        return Ok(());
      }
      match LicenseService::extend(&app.db, &key, days).await {
        Ok(new_exp) => {
          let text = format!(
            "✅ Key extended by {} days.\nNew expiry: <code>{}</code>",
            days,
            format_date(new_exp)
          );
          bot.reply_html(msg.chat.id, text).await?;
        }
        Err(e) => {
          bot.reply_html(msg.chat.id, format!("❌ Error: {}", e)).await?;
        }
      }
    }

    Command::Ban(key) => {
      if !is_admin {
        return Ok(());
      }
      match LicenseService::set_blocked(&app.db, &key, true).await {
        Ok(_) => {
          app.drop_sessions(&key);
          bot.reply_html(msg.chat.id, "🚫 Key blocked, sessions dropped").await?;
        }
        Err(e) => {
          bot.reply_html(msg.chat.id, format!("❌ Error: {}", e)).await?;
        }
      }
    }

    Command::Unban(key) => {
      if !is_admin {
        return Ok(());
      }
      match LicenseService::set_blocked(&app.db, &key, false).await {
        Ok(_) => {
          bot.reply_html(msg.chat.id, "✅ Key unblocked").await?;
        }
        Err(e) => {
          bot.reply_html(msg.chat.id, format!("❌ Error: {}", e)).await?;
        }
      }
    }

    Command::Info(key) => {
      if !is_admin {
        return Ok(());
      }
      let active = app.sessions.get(&key).map(|s| s.len()).unwrap_or(0);

      match LicenseService::get_by_key(&app.db, &key).await {
        Ok(Some(license)) => {
          let status = if license.is_blocked { "⛔ BLOCKED" } else { "✅ Active" };
          let username = bot.infer_username(ChatId(license.tg_user_id)).await;

          let text = format!(
            "🔑 <b>Key Info</b>\n\
            Owner: {}\n\
            Type: {:?}\n\
            Expires: {}\n\
            Status: {}\n\
            Active Sessions: {}",
            username,
            license.license_type,
            format_date(license.expires_at),
            status,
            active
          );
          bot.reply_html(msg.chat.id, text).await?;
        }
        Ok(None) => {
          bot.reply_html(msg.chat.id, "❌ Key not found").await?;
        }
        Err(e) => {
          bot.reply_html(msg.chat.id, format!("❌ DB Error: {}", e)).await?;
        }
      }
    }

    Command::Stats => {
      if !is_admin {
        return Ok(());
      }
      handle_stats_view(&app, &bot, msg.chat.id).await?;
    }

    Command::Backup => {
      if !is_admin {
        return Ok(());
      }
      if let Err(_) = app.perform_backup(msg.chat.id).await {
        bot.send_document(msg.chat.id, InputFile::file("licenses.db")).await?;
      }
    }
  }

  Ok(())
}

async fn handle_callback(
  app: Arc<AppState>,
  bot: Bot,
  query: CallbackQuery,
) -> ResponseResult<()> {
  let Some(data) = query.data else {
    return Ok(());
  };

  let Some(msg) = query.message else {
    return Ok(());
  };

  let chat_id = msg.chat().id;
  let message_id = msg.id();
  let user_id = query.from.id.0 as i64;
  let is_admin = app.admins.contains(&user_id);

  // Answer callback to remove loading state
  bot.answer_callback_query(query.id.clone()).await?;

  match data.as_str() {
    CB_PROFILE => {
      handle_profile_view(&app, &bot, chat_id, message_id, user_id).await?;
    }

    CB_LICENSE => {
      handle_license_edit(&app, &bot, chat_id, message_id, user_id).await?;
    }

    CB_TRIAL => {
      handle_trial_claim(&app, &bot, chat_id, user_id).await?;
    }

    CB_DOWNLOAD => {
      handle_download(&app, &bot, chat_id, message_id).await?;
    }

    CB_SUPPORT => {
      let text = "🆘 <b>Support</b>\n\n\
        Contact us: @y_a_c_s_p\n\
        We'll help you with any issues!";
      bot.edit_with_keyboard(chat_id, message_id, text, back_keyboard()).await?;
    }

    CB_BACK => {
      let text = "👋 <b>YACS Panel</b>\n\nSelect an option:";
      bot
        .edit_with_keyboard(chat_id, message_id, text, main_menu_keyboard(is_admin))
        .await?;
    }

    CB_ADMIN => {
      if !is_admin {
        return Ok(());
      }
      let text = "🔧 <b>Admin Panel</b>\n\nSelect an action:";
      bot.edit_with_keyboard(chat_id, message_id, text, admin_keyboard()).await?;
    }

    CB_STATS => {
      if !is_admin {
        return Ok(());
      }
      handle_stats_edit(&app, &bot, chat_id, message_id).await?;
    }

    CB_BACKUP => {
      if !is_admin {
        return Ok(());
      }
      let _ = app.perform_backup(chat_id).await;
      bot
        .edit_with_keyboard(chat_id, message_id, "📦 Backup sent!", admin_keyboard())
        .await?;
    }

    _ => {}
  }

  Ok(())
}

async fn handle_profile_view(
  app: &AppState,
  bot: &Bot,
  chat_id: ChatId,
  message_id: MessageId,
  user_id: i64,
) -> ResponseResult<()> {
  let user = UserService::get_by_id(&app.db, user_id).await.ok().flatten();

  let (reg_date, username) = match user {
    Some(u) => (format_date(u.reg_date), u.username.unwrap_or_else(|| "Not set".into())),
    None => ("Unknown".into(), "Unknown".into()),
  };

  // Get stats if available
  let stats = StatsService::get_display_stats(&app.db, user_id).await.ok();

  let mut text = format!(
    "👤 <b>My Profile</b>\n\n\
    <b>User ID:</b> <code>{}</code>\n\
    <b>Username:</b> @{}\n\
    <b>Registered:</b> {}",
    user_id, username, reg_date
  );

  if let Some(s) = stats {
    text.push_str(&format!(
      "\n\n<b>📊 Farming Stats:</b>\n\
      Weekly XP: {}\n\
      Total XP: {}\n\
      Drops: {}\n\
      Runtime: {:.1}h",
      s.weekly_xp, s.total_xp, s.drops_count, s.total_runtime_hours
    ));
  }

  bot.edit_with_keyboard(chat_id, message_id, text, back_keyboard()).await?;
  Ok(())
}

async fn handle_license_view(app: &AppState, bot: &Bot, chat_id: ChatId) -> ResponseResult<()> {
  let user_id = chat_id.0;
  let now = Utc::now().naive_utc();

  match LicenseService::get_by_user(&app.db, user_id, false).await {
    Ok(licenses) if !licenses.is_empty() => {
      let mut text = String::from("🔑 <b>Your Licenses:</b>\n");

      for license in licenses {
        let status = if license.expires_at > now {
          format!("⏳ {}", format_duration(license.expires_at, now))
        } else {
          "❌ Expired".into()
        };

        text.push_str(&format!(
          "\n<code>{}</code>\n{} | {:?}\n",
          license.key, status, license.license_type
        ));
      }

      bot.reply_with_keyboard(chat_id, text, back_keyboard()).await?;
    }
    _ => {
      bot
        .reply_with_keyboard(
          chat_id,
          "❌ You have no active licenses.\n\nTry /freeweek to get a free trial!",
          back_keyboard(),
        )
        .await?;
    }
  }

  Ok(())
}

async fn handle_license_edit(
  app: &AppState,
  bot: &Bot,
  chat_id: ChatId,
  message_id: MessageId,
  user_id: i64,
) -> ResponseResult<()> {
  let now = Utc::now().naive_utc();

  match LicenseService::get_by_user(&app.db, user_id, false).await {
    Ok(licenses) if !licenses.is_empty() => {
      let mut text = String::from("🔑 <b>Your Licenses:</b>\n");

      for license in licenses {
        let status = if license.expires_at > now {
          format!("⏳ {}", format_duration(license.expires_at, now))
        } else {
          "❌ Expired".into()
        };

        text.push_str(&format!(
          "\n<code>{}</code>\n{} | {:?}\n",
          license.key, status, license.license_type
        ));
      }

      bot.edit_with_keyboard(chat_id, message_id, text, back_keyboard()).await?;
    }
    _ => {
      bot
        .edit_with_keyboard(
          chat_id,
          message_id,
          "❌ You have no active licenses.\n\nTry /freeweek to get a free trial!",
          back_keyboard(),
        )
        .await?;
    }
  }

  Ok(())
}

async fn handle_trial_claim(
  app: &AppState,
  bot: &Bot,
  chat_id: ChatId,
  user_id: i64,
) -> ResponseResult<()> {
  let promo_name = "first_promo";

  match LicenseService::claim_promo(&app.db, user_id, promo_name).await {
    Ok(license) => {
      let text = format!(
        "🎉 <b>Success!</b>\n\n\
        Here is your FREE week license:\n\
        <code>{}</code>\n\n\
        Download the software using the Download button!",
        license.key
      );
      bot.reply_with_keyboard(chat_id, text, back_keyboard()).await?;
    }
    Err(e) => {
      let msg = match e {
        crate::error::AppError::PromoNotActive => "⏳ Promo is not active right now.",
        crate::error::AppError::PromoAlreadyClaimed => "❌ You have already claimed this promo!",
        _ => "❌ An error occurred.",
      };
      bot.reply_with_keyboard(chat_id, msg, back_keyboard()).await?;
    }
  }

  Ok(())
}

async fn handle_download(
  app: &AppState,
  bot: &Bot,
  chat_id: ChatId,
  message_id: MessageId,
) -> ResponseResult<()> {
  match BuildService::get_latest(&app.db).await {
    Ok(Some(build)) => {
      let path = Path::new(&build.file_path);
      if path.exists() {
        let doc = InputFile::file(path);
        let caption = format!(
          "📥 <b>YACS Panel v{}</b>\n\n{}\n\nDownloads: {}",
          build.version,
          build.changelog.unwrap_or_default(),
          build.download_count
        );

        bot.send_document(chat_id, doc).caption(caption).parse_mode(ParseMode::Html).await?;

        // Increment download count
        let _ = BuildService::increment_downloads(&app.db, &build.version).await;
      } else {
        bot
          .edit_with_keyboard(
            chat_id,
            message_id,
            "❌ Build file not found. Contact support.",
            back_keyboard(),
          )
          .await?;
      }
    }
    _ => {
      bot
        .edit_with_keyboard(
          chat_id,
          message_id,
          "❌ No builds available yet. Contact support.",
          back_keyboard(),
        )
        .await?;
    }
  }

  Ok(())
}

async fn handle_stats_view(app: &AppState, bot: &Bot, chat_id: ChatId) -> ResponseResult<()> {
  let active_keys = app.sessions.len();
  let total_sessions: usize = app.sessions.iter().map(|e| e.value().len()).sum();

  let text = format!(
    "📊 <b>System Stats</b>\n\n\
    <b>Active Keys:</b> {}\n\
    <b>Total Sessions:</b> {}",
    active_keys, total_sessions
  );

  bot.reply_with_keyboard(chat_id, text, back_keyboard()).await?;
  Ok(())
}

async fn handle_stats_edit(
  app: &AppState,
  bot: &Bot,
  chat_id: ChatId,
  message_id: MessageId,
) -> ResponseResult<()> {
  let active_keys = app.sessions.len();
  let total_sessions: usize = app.sessions.iter().map(|e| e.value().len()).sum();

  let text = format!(
    "📊 <b>System Stats</b>\n\n\
    <b>Active Keys:</b> {}\n\
    <b>Total Sessions:</b> {}",
    active_keys, total_sessions
  );

  bot.edit_with_keyboard(chat_id, message_id, text, admin_keyboard()).await?;
  Ok(())
}

pub async fn run_bot(app: Arc<AppState>) {
  info!("Starting Telegram bot...");

  let bot = app.bot.clone();

  let handler = dptree::entry()
    .branch(
      Update::filter_message()
        .filter_command::<Command>()
        .endpoint({
          let app = app.clone();
          move |bot: Bot, msg: Message, cmd: Command| {
            let app = app.clone();
            async move { handle_command(app, bot, msg, cmd).await }
          }
        }),
    )
    .branch(Update::filter_callback_query().endpoint({
      let app = app.clone();
      move |bot: Bot, query: CallbackQuery| {
        let app = app.clone();
        async move { handle_callback(app, bot, query).await }
      }
    }));

  Dispatcher::builder(bot, handler)
    .enable_ctrlc_handler()
    .build()
    .dispatch()
    .await;
}
