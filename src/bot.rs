use std::{path::Path, sync::Arc};

use futures::future;
use teloxide::{
  net::Download,
  prelude::*,
  types::{
    InlineKeyboardButton, InlineKeyboardMarkup, InputFile, MessageId, ParseMode,
  },
  utils::command::{BotCommands, ParseError},
};
use tokio::io::AsyncWriteExt;

use crate::{
  entity::license::LicenseType,
  prelude::*,
  state::{AppState, Services},
};

fn format_date(date: DateTime) -> String {
  date.format("%d.%m.%Y %H:%M").to_string()
}

fn format_duration(duration: TimeDelta) -> String {
  format!(
    "{}d {}h {}m",
    duration.num_days(),
    duration.num_hours() % 24,
    duration.num_minutes() % 60
  )
}

const CB_PROFILE: &str = "profile";
const CB_LICENSE: &str = "license";
const CB_TRIAL: &str = "trial";
const CB_DOWNLOAD: &str = "download";
const CB_BACK: &str = "back";
const CB_ADMIN: &str = "admin";
const CB_STATS: &str = "stats";
const CB_BACKUP: &str = "backup";

fn main_menu(is_promo: bool, is_admin: bool) -> InlineKeyboardMarkup {
  let mut rows = vec![
    vec![InlineKeyboardButton::callback("👤 My Profile", CB_PROFILE)],
    vec![InlineKeyboardButton::callback("🔑 My License", CB_LICENSE)],
    vec![InlineKeyboardButton::callback("📥 Download Panel", CB_DOWNLOAD)],
  ];

  if is_promo {
    rows.push(vec![InlineKeyboardButton::callback(
      "🆓 Get Free Trial",
      CB_TRIAL,
    )]);
  }

  if is_admin {
    rows.push(vec![InlineKeyboardButton::callback("🔧 Admin Panel", CB_ADMIN)]);
  }

  InlineKeyboardMarkup::new(rows)
}

fn admin_keyboard() -> InlineKeyboardMarkup {
  InlineKeyboardMarkup::new(vec![
    vec![InlineKeyboardButton::callback("📊 Server Stats", CB_STATS)],
    vec![InlineKeyboardButton::callback("📦 Backup DB", CB_BACKUP)],
    vec![InlineKeyboardButton::callback("« Back to Menu", CB_BACK)],
  ])
}

fn back_keyboard() -> InlineKeyboardMarkup {
  InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
    "« Back to Menu",
    CB_BACK,
  )]])
}

fn parse_publish(
  input: String,
) -> std::result::Result<(String, String), ParseError> {
  let mut parts = input.splitn(2, ' ');
  let version = parts.next().unwrap_or_default().to_string();
  let changelog = parts.next().unwrap_or_default().to_string();

  if version.is_empty() {
    return Err(ParseError::IncorrectFormat("Version is required".into()));
  }

  Ok((version, changelog))
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
  Start,
  Help,
  MyKey,
  FreeWeek,
  // Admin commands
  Users,
  Gen(String),
  #[command(parse_with = "split")]
  Buy {
    key: String,
    days: i64,
  },
  Ban(String),
  Unban(String),
  Info(String),
  Stats,
  Backup,
  Builds,
  #[command(parse_with = parse_publish)]
  Publish {
    version: String,
    changelog: String,
  },
  Deactivate(String),
}

trait BotExt {
  async fn reply_html(
    &self,
    chat_id: ChatId,
    text: impl Into<String>,
  ) -> ResponseResult<Message>;

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
  async fn reply_html(
    &self,
    chat_id: ChatId,
    text: impl Into<String>,
  ) -> ResponseResult<Message> {
    self.send_message(chat_id, text.into()).parse_mode(ParseMode::Html).await
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
  let mut text = String::from("<b>YACS Panel</b>\n\n");
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
    text.push_str("\n<b>Build Management:</b>\n");
    text.push_str("/builds - List all builds\n");
    text
      .push_str("/publish <code>version</code> <code>changelog</code> - Publish build (reply to file)\n");
    text.push_str("/deactivate <code>version</code> - Deactivate build\n");
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

  let sv = app.sv();

  let _ = sv.user.get_or_create(user_id).await;

  match &cmd {
    Command::Start => {
      let text = "<b>Yet Another Counter Strike Panel!</b>\n\n\
        Use the buttons below to navigate.\n\
        Read docs: https://yacsp.gitbook.io/yacsp\n\
        Contact support: @y_a_c_s_p";
      bot
        .reply_with_keyboard(
          msg.chat.id,
          text,
          main_menu(sv.license.is_promo_active(), is_admin),
        )
        .await?;
    }

    Command::Help => {
      bot
        .reply_with_keyboard(msg.chat.id, help_text(is_admin), back_keyboard())
        .await?;
    }

    Command::MyKey => {
      handle_license_view(&sv, &bot, msg.chat.id).await?;
    }

    Command::FreeWeek => {
      handle_trial_claim(&sv, &bot, msg.chat.id, user_id).await?;
    }

    _ => {}
  }

  if is_admin {
    handle_admin_command(app, bot, msg, cmd).await?;
  }

  Ok(())
}

async fn handle_admin_command(
  app: Arc<AppState>,
  bot: Bot,
  msg: Message,
  cmd: Command,
) -> ResponseResult<()> {
  let sv = app.sv();

  if let Command::Users = cmd {
    let users = match sv.user.all().await {
      Ok(u) => u,
      Err(e) => {
        bot.reply_html(msg.chat.id, format!("❌ DB Error: {}", e)).await?;
        return Ok(());
      }
    };

    if users.is_empty() {
      bot.reply_html(msg.chat.id, "There is no users.").await?;
      return Ok(());
    }

    bot
      .reply_html(
        msg.chat.id,
        format!("⏳ Found {} users. Getting names...", users.len()),
      )
      .await?;

    let username_futures = users.into_iter().map(|u| {
      let bot = bot.clone();
      async move { bot.infer_username(ChatId(u.tg_user_id)).await }
    });

    let usernames = future::join_all(username_futures).await;

    let mut response_text =
      format!("<b>👥 Registered users (Total: {}):</b>\n\n", usernames.len());
    for (i, username) in usernames.iter().enumerate() {
      let line = format!("{}. {}\n", i + 1, username);
      // Защита от слишком длинного сообщения (лимит Telegram ~4096 символов)
      if response_text.len() + line.len() > 4096 {
        response_text.push_str("etc. (list is to long).");
        break;
      }
      response_text.push_str(&line);
    }

    bot.reply_html(msg.chat.id, response_text).await?;
    return Ok(());
  }

  let result: std::result::Result<String, String> = match cmd {
    Command::Gen(args) => {
      let parts: Vec<&str> = args.split_whitespace().collect();
      let (target_user, days) = match parts.as_slice() {
        [user_id] => (user_id.parse::<i64>().ok(), 0u64),
        [user_id, days] => {
          (user_id.parse::<i64>().ok(), days.parse::<u64>().unwrap_or(0))
        }
        _ => (None, 0),
      };

      match target_user {
        Some(target_user) => sv
          .license
          .create(target_user, LicenseType::Pro, days)
          .await
          .map(|l| format!("✅ Key created:\n<code>{}</code>", l.key))
          .map_err(|e| e.to_string()),
        None => Err("Usage: /gen <user_id> [days]".into()),
      }
    }

    Command::Buy { key, days } => sv
      .license
      .extend(&key, days)
      .await
      .map(|new_exp| {
        format!(
          "✅ Key extended by {days} days.\nNew expiry: <code>{}</code>",
          format_date(new_exp)
        )
      })
      .map_err(|e| e.to_string()),

    Command::Ban(key) => {
      let result = sv.license.set_blocked(&key, true).await;
      if result.is_ok() {
        app.drop_sessions(&key);
      }
      result
        .map(|_| "🚫 Key blocked, sessions dropped".into())
        .map_err(|e| e.to_string())
    }

    Command::Unban(key) => sv
      .license
      .set_blocked(&key, false)
      .await
      .map(|_| "✅ Key unblocked".into())
      .map_err(|e| e.to_string()),

    Command::Info(key) => {
      let active = app.sessions.get(&key).map(|s| s.len()).unwrap_or(0);
      match sv.license.by_key(&key).await {
        Ok(Some(license)) => {
          let status =
            if license.is_blocked { "⛔ BLOCKED" } else { "✅ Active" };
          let username = bot.infer_username(ChatId(license.tg_user_id)).await;
          Ok(format!(
            "🔑 <b>Key Info</b>\n\
            Owner: {username}\n\
            Type: {:?}\n\
            Expires: {}\n\
            Status: {status}\n\
            Active Sessions: {active}",
            license.license_type,
            format_date(license.expires_at),
          ))
        }
        Ok(None) => Err("Key not found".into()),
        Err(e) => Err(e.to_string()),
      }
    }
    Command::Backup => {
      if app.perform_backup(msg.chat.id).await.is_err() {
        bot.send_document(msg.chat.id, InputFile::file("licenses.db")).await?;
      }
      return Ok(());
    }
    Command::Builds => match sv.build.all().await {
      Ok(builds) if !builds.is_empty() => {
        let mut text = String::from("<b>All Builds:</b>\n");
        for build in builds {
          let status = if build.is_active { "✅" } else { "❌" };
          text.push_str(&format!(
            "\n{} <b>v{}</b>\n{} downloads\n{}\n",
            status,
            build.version,
            build.downloads,
            format_date(build.created_at)
          ));
          if let Some(changelog) = &build.changelog {
            text.push_str(&format!("<code>{}</code>\n", changelog));
          }
        }
        bot.reply_with_keyboard(msg.chat.id, text, back_keyboard()).await?;
        return Ok(());
      }
      Ok(_) => Err("No builds found".into()),
      Err(e) => Err(e.to_string()),
    },

    Command::Publish { version, changelog } => {
      let document = msg.reply_to_message().and_then(|reply| reply.document());

      let Some(doc) = document else {
        bot
          .reply_html(
            msg.chat.id,
            "❌ Please reply to a file with this command.\n\
            Example: Reply to a .exe file with /publish 1.0.0 Initial release",
          )
          .await?;
        return Ok(());
      };

      let file = match bot.get_file(doc.file.id.clone()).await {
        Ok(f) => f,
        Err(e) => return Err(e),
      };
      let file_name = doc
        .file_name
        .clone()
        .unwrap_or_else(|| format!("build_{}.bin", version));
      let file_path = format!("{}/{}", app.config.builds_directory, file_name);

      if let Err(e) =
        tokio::fs::create_dir_all(&app.config.builds_directory).await
      {
        bot
          .reply_html(
            msg.chat.id,
            format!("❌ Failed to create builds directory: {}", e),
          )
          .await?;
        return Ok(());
      }

      let download_result = async {
        let mut stream = bot.download_file_stream(&file.path);
        let mut dst = tokio::fs::File::create(&file_path).await?;

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
          let chunk = chunk?;
          dst.write_all(&chunk).await?;
        }
        dst.flush().await?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
      }
      .await;

      if let Err(e) = download_result {
        bot
          .reply_html(msg.chat.id, format!("❌ Failed to download file: {}", e))
          .await?;
        return Ok(());
      }

      let changelog_opt =
        if changelog.is_empty() { None } else { Some(changelog) };

      sv.build
        .create(version.clone(), file_path, changelog_opt)
        .await
        .map(|build| {
          format!(
            "✅ Build published!\n\n\
            <b>Version:</b> {}\n\
            <b>File:</b> {}\n\
            <b>Created:</b> {}",
            build.version,
            build.file_path,
            format_date(build.created_at)
          )
        })
        .map_err(|e| e.to_string())
    }

    Command::Deactivate(version) => match sv.build.by_version(&version).await {
      Ok(Some(build)) if build.is_active => sv
        .build
        .deactivate(&version)
        .await
        .map(|_| {
          format!(
            "✅ Build deactivated.\n\n\
            <b>Version:</b> {}\n\
            <b>Downloads:</b> {}",
            build.version, build.downloads
          )
        })
        .map_err(|e| e.to_string()),
      Ok(Some(_)) => Err(format!("Build v{} is already inactive", version)),
      Ok(None) => Err(format!("Build v{} not found", version)),
      Err(e) => Err(e.to_string()),
    },

    _ => return Ok(()),
  };

  match result {
    Ok(text) => {
      bot.reply_html(msg.chat.id, text).await?;
    }
    Err(e) => {
      bot.reply_html(msg.chat.id, format!("❌ {}", e)).await?;
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

  let sv = app.sv();

  let chat_id = msg.chat().id;
  let message_id = msg.id();
  let user_id = query.from.id.0 as i64;
  let is_admin = app.admins.contains(&user_id);

  // answer callback to remove loading state
  bot.answer_callback_query(query.id.clone()).await?;

  match data.as_str() {
    CB_PROFILE => {
      handle_profile_view(&sv, &bot, chat_id, message_id, user_id).await?;
    }
    CB_LICENSE => {
      handle_license_edit(&sv, &bot, chat_id, message_id, user_id).await?;
    }
    CB_TRIAL => {
      handle_trial_claim(&sv, &bot, chat_id, user_id).await?;
    }
    CB_DOWNLOAD => {
      if let Ok(keys) = sv.license.by_user(chat_id.0, false).await
        && !keys.is_empty()
      {
        handle_download(&sv, &bot, chat_id, message_id).await?;
      } else {
        bot
          .edit_with_keyboard(
            chat_id,
            message_id,
            "You have no active license!",
            back_keyboard(),
          )
          .await?;
      }
    }
    CB_BACK => {
      let text = "<b>Yet Another Counter Strike Panel!</b>\n\n\
        Use the buttons below to navigate.\n\
        Read docs: https://yacsp.gitbook.io/yacsp\n\
        Contact support: @y_a_c_s_p";
      bot
        .edit_with_keyboard(
          chat_id,
          message_id,
          text,
          main_menu(sv.license.is_promo_active(), is_admin),
        )
        .await?;
    }
    CB_ADMIN => {
      if !is_admin {
        return Ok(());
      }
      let text = "🔧 <b>Admin Panel</b>\n\nSelect an action:";
      bot
        .edit_with_keyboard(chat_id, message_id, text, admin_keyboard())
        .await?;
    }
    CB_STATS => {
      let active_keys = app.sessions.len();
      let total_sessions: usize =
        app.sessions.iter().map(|e| e.value().len()).sum();

      let message = format!(
        "📊 <b>System Stats</b>\nActive Keys: {}\nTotal Windows: {}",
        active_keys, total_sessions
      );
      bot.reply_html(chat_id, message).await?;
    }
    CB_BACKUP => {
      if !is_admin {
        return Ok(());
      }
      let _ = app.perform_backup(chat_id).await;
      bot
        .edit_with_keyboard(
          chat_id,
          message_id,
          "📦 Backup sent!",
          admin_keyboard(),
        )
        .await?;
    }
    _ => {}
  }

  Ok(())
}

async fn handle_profile_view(
  sv: &Services<'_>,
  bot: &Bot,
  chat_id: ChatId,
  message_id: MessageId,
  user_id: i64,
) -> ResponseResult<()> {
  let user = sv.user.by_id(user_id).await.ok().flatten();

  let reg_date = match user {
    Some(u) => format_date(u.reg_date),
    None => "Unknown".into(),
  };

  let stats = sv.stats.display_stats(user_id).await.ok();

  let mut text = format!(
    "👤 <b>My Profile</b>\n\n\
    <b>User ID:</b> <code>{}</code>\n\
    <b>Registered:</b> {}",
    user_id, reg_date
  );

  if let Some(s) = stats {
    text.push_str(&format!(
      "\n\n<b>📊 Farming Stats:</b>\n\
      Weekly XP: {}\n\
      Total XP: {}\n\
      Drops: {}\n\
      Runtime: {:.1}h",
      s.weekly_xp, s.total_xp, s.drops_count, s.runtime_hours
    ));
  }

  bot.edit_with_keyboard(chat_id, message_id, text, back_keyboard()).await?;

  Ok(())
}

async fn handle_license_view(
  sv: &Services<'_>,
  bot: &Bot,
  chat_id: ChatId,
) -> ResponseResult<()> {
  let user_id = chat_id.0;
  let now = Utc::now().naive_utc();

  match sv.license.by_user(user_id, false).await {
    Ok(licenses) if !licenses.is_empty() => {
      let mut text = String::from("🔑 <b>Your Licenses:</b>\n");

      for license in licenses {
        let status = if license.expires_at > now {
          format!("⏳ {}", format_duration(license.expires_at - now))
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
          "You have no active license!",
          back_keyboard(),
        )
        .await?;
    }
  }

  Ok(())
}

async fn handle_license_edit(
  sv: &Services<'_>,
  bot: &Bot,
  chat_id: ChatId,
  message_id: MessageId,
  user_id: i64,
) -> ResponseResult<()> {
  let now = Utc::now().naive_utc();

  match sv.license.by_user(user_id, false).await {
    Ok(licenses) if !licenses.is_empty() => {
      let mut text = String::from("🔑 <b>Your Licenses:</b>\n");

      for license in licenses {
        let status = if license.expires_at > now {
          format!("⏳ {}", format_duration(license.expires_at - now))
        } else {
          "❌ Expired".into()
        };

        text.push_str(&format!(
          "\n<code>{}</code>\n{} | {:?}\n",
          license.key, status, license.license_type
        ));
      }

      bot
        .edit_with_keyboard(chat_id, message_id, text, back_keyboard())
        .await?;
    }
    _ => {
      bot
        .edit_with_keyboard(
          chat_id,
          message_id,
          "You have no active license!",
          back_keyboard(),
        )
        .await?;
    }
  }

  Ok(())
}

async fn handle_trial_claim(
  sv: &Services<'_>,
  bot: &Bot,
  chat_id: ChatId,
  user_id: i64,
) -> ResponseResult<()> {
  let promo_name = "first_promo";

  match sv.license.claim_promo(user_id, promo_name).await {
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
        Error::Promo(Promo::Inactive) => "Promo is not active right now.",
        Error::Promo(Promo::Claimed) => "You have already claimed this promo",
        _ => "An error occurred.",
      };
      bot.reply_with_keyboard(chat_id, msg, back_keyboard()).await?;
    }
  }

  Ok(())
}

async fn handle_download(
  sv: &Services<'_>,
  bot: &Bot,
  chat_id: ChatId,
  message_id: MessageId,
) -> ResponseResult<()> {
  match sv.build.latest().await {
    Ok(Some(build)) => {
      let path = Path::new(&build.file_path);
      if path.exists() {
        let doc = InputFile::file(path);
        let caption = format!(
          "<b>YACS Panel v{}</b>\n\n{}",
          build.version,
          build.changelog.unwrap_or_default(),
        );

        bot
          .send_document(chat_id, doc)
          .caption(caption)
          .parse_mode(ParseMode::Html)
          .await?;

        let _ = sv.build.increment_downloads(&build.version).await;
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

pub async fn run_bot(app: Arc<AppState>) {
  info!("Starting Telegram bot...");

  let bot = app.bot.clone();

  let handler = dptree::entry()
    .branch(Update::filter_message().filter_command::<Command>().endpoint({
      let app = app.clone();
      move |bot: Bot, msg: Message, cmd: Command| {
        let app = app.clone();
        async move { handle_command(app, bot, msg, cmd).await }
      }
    }))
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
