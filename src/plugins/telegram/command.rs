use std::{path::Path, sync::Arc};

use futures::future;
use teloxide::{
  prelude::*,
  types::InputFile,
  utils::command::{BotCommands, ParseError},
};

use super::ReplyBot;
use crate::{
  entity::license::LicenseType,
  prelude::*,
  state::{AppState, Services},
};

fn parse_publish(
  input: String,
) -> std::result::Result<(String, String, String), ParseError> {
  let mut parts = input.splitn(3, ' ');
  let filename = parts.next().unwrap_or_default().to_string();
  let version = parts.next().unwrap_or_default().to_string();
  let changelog = parts.next().unwrap_or_default().to_string();

  if filename.is_empty() || version.is_empty() {
    return Err(ParseError::IncorrectFormat(
      "Usage: /publish <filename> <version> [changelog]".into(),
    ));
  }

  Ok((filename, version, changelog))
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Command {
  Start,
  // Admin commands only below - users use button interface
  Help,
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
    filename: String,
    version: String,
    changelog: String,
  },
  Deactivate(String),
}

pub async fn handle(
  app: Arc<AppState>,
  bot: ReplyBot,
  cmd: Command,
) -> ResponseResult<()> {
  let sv = app.sv();

  let _ = sv.user.get_or_create(bot.user_id).await;

  if let Command::Start = &cmd {
    let text = "<b>Yet Another Counter Strike Pbot.anel!</b>\n\n\
      Use the buttons below to navigate.\n\
      Read docs: https://yacsp.gitbook.io/yacsp\n\
      Contact support: @y_a_c_s_p";
    bot
      .reply_with_keyboard(
        text,
        super::callback::main_menu(sv.license.is_promo_active()),
      )
      .await?;
  }

  if app.admins.contains(&bot.user_id) {
    handle_admin_command(app, bot, cmd).await?;
  }

  Ok(())
}

async fn process_info_command(
  sv: &Services<'_>,
  app: &AppState,
  bot: &ReplyBot,
  input: String,
) -> Result<String> {
  let input = input.trim();
  if input.is_empty() {
    return Err(Error::InvalidArgs(
      "Usage: /info <license_key | user_id>".into(),
    ));
  }

  if let Ok(user_id) = input.parse::<i64>() {
    let user = sv.user.by_id(user_id).await?.ok_or(Error::UserNotFound)?;
    let username = bot.infer_username(ChatId(user_id)).await;
    let stats = sv.stats.display_stats(user_id).await?;
    let licenses = sv.license.by_user(user_id, true).await?;

    let mut total_active_sessions = 0;
    let mut lic_text = String::new();

    for lic in &licenses {
      let active = app.sessions.get(&lic.key).map(|s| s.len()).unwrap_or(0);
      total_active_sessions += active;

      let status_icon = if lic.is_blocked {
        "⛔"
      } else if lic.expires_at < Utc::now().naive_utc() {
        "❌"
      } else if active > 0 {
        "🟢"
      } else {
        "⚪"
      };

      lic_text.push_str(&format!(
        "{} <code>{}</code> ({:?})\n",
        status_icon, lic.key, lic.license_type
      ));
    }

    return Ok(format!(
      "👤 <b>User Info</b>\n\
      ID: <code>{}</code>\n\
      Name: {}\n\
      Registered: {}\n\n\
      📊 <b>Global Stats</b>\n\
      XP (Week/Total): {} / {}\n\
      Runtime: {:.1}h\n\
      Total Sessions: {}\n\n\
      🔑 <b>Licenses ({})</b>\n\
      {}",
      user.tg_user_id,
      username,
      utils::format_date(user.reg_date),
      stats.weekly_xp,
      stats.total_xp,
      stats.runtime_hours,
      total_active_sessions,
      licenses.len(),
      if lic_text.is_empty() { "No licenses" } else { &lic_text }
    ));
  }

  let key = input;
  let license = sv.license.by_key(key).await?.ok_or(Error::LicenseNotFound)?;
  let username = bot.infer_username(ChatId(license.tg_user_id)).await;

  let sessions = app.sessions.get(key);
  let active_count = sessions.as_ref().map(|s| s.len()).unwrap_or(0);
  let now = Utc::now().naive_utc();

  let status = if license.is_blocked {
    "⛔ BLOCKED"
  } else if license.expires_at < now {
    "❌ EXPIRED"
  } else if active_count > 0 {
    "🟢 ONLINE"
  } else {
    "⚪ OFFLINE"
  };

  let duration_left = if license.expires_at > now {
    utils::format_duration(license.expires_at - now)
  } else {
    "0d 0h".to_string()
  };

  let mut text = format!(
    "🔑 <b>License Info</b>\n\n\
    <b>Key:</b> <code>{}</code>\n\
    <b>Type:</b> {:?}\n\
    <b>Status:</b> {}\n\
    <b>Owner:</b> {} (<code>{}</code>)\n\n\
    📅 <b>Timeline</b>\n\
    Created: {}\n\
    Expires: {} (in {})\n\n\
    🖥 <b>Sessions ({}/{})</b>\n",
    license.key,
    license.license_type,
    status,
    username,
    license.tg_user_id,
    utils::format_date(license.created_at),
    utils::format_date(license.expires_at),
    duration_left,
    active_count,
    license.max_sessions
  );

  if let Some(sess_list) = sessions {
    for (i, s) in sess_list.iter().enumerate() {
      text.push_str(&format!(
        " {}. ID: <code>{}...</code>\n    HWID: <code>{}</code>\n",
        i + 1,
        &s.session_id.chars().take(8).collect::<String>(),
        s.hwid_hash.as_deref().unwrap_or("Unknown")
      ));
    }
  } else if active_count == 0 {
    text.push_str(" <i>No active sessions</i>");
  }

  Ok(text)
}

async fn handle_admin_command(
  app: Arc<AppState>,
  bot: ReplyBot,
  cmd: Command,
) -> ResponseResult<()> {
  let sv = app.sv();

  if let Command::Users = cmd {
    let users_data = match sv.user.all_with_licenses().await {
      Ok(u) => u,
      Err(e) => {
        bot.reply_html(format!("❌ DB Error: {}", e)).await?;
        return Ok(());
      }
    };

    if users_data.is_empty() {
      bot.reply_html("📭 Database is empty.").await?;
      return Ok(());
    }

    bot
      .reply_html(format!("⏳ Loading data for {} users...", users_data.len()))
      .await?;

    let user_futures = users_data.into_iter().map(|(u, licenses)| {
      let bot = bot.clone();
      async move {
        let username = bot.infer_username(ChatId(u.tg_user_id)).await;
        (u, username, licenses)
      }
    });

    let resolved_users = future::join_all(user_futures).await;

    let mut text =
      format!("👥 <b>Users List (Total: {})</b>\n\n", resolved_users.len());
    let now = Utc::now().naive_utc();

    for (i, (user, username, licenses)) in resolved_users.iter().enumerate() {
      text.push_str(&format!(
        "<b>{}. {}</b> <code>{}</code>\n",
        i + 1,
        username,
        user.tg_user_id
      ));

      if licenses.is_empty() {
        text.push_str("   └─ <i>No licenses</i>\n");
      }

      for (j, lic) in licenses.iter().enumerate() {
        let is_last = j == licenses.len() - 1;
        let branch = if is_last { "└─" } else { "├─" };

        let (icon, status_text) = if lic.is_blocked {
          ("⛔", "BLOCKED".to_string())
        } else if lic.expires_at < now {
          ("❌", "Expired".to_string())
        } else {
          match app.sessions.get(&lic.key) {
            Some(sessions) if !sessions.is_empty() => {
              ("🟢", format!("Online ({})", sessions.len()))
            }
            _ => ("⚪", "Offline".to_string()),
          }
        };

        let short_key =
          if lic.key.len() > 8 { &lic.key[..8] } else { &lic.key };

        text.push_str(&format!(
          "   {} {} <b>{:?}</b> [{}]\n   │  <code>{}...</code> | {}\n",
          branch,
          icon,
          lic.license_type,
          status_text,
          short_key,
          utils::format_date(lic.expires_at)
        ));
      }

      text.push_str("\n");

      if text.len() > 3800 {
        text.push_str("<i>...list truncated (too long)...</i>");
        break;
      }
    }

    bot.reply_html(text).await?;
    return Ok(());
  }

  let result: Result<String> = match cmd {
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
          .map(|l| format!("✅ Key created:\n<code>{}</code>", l.key)),
        None => Err(Error::InvalidArgs("Usage: /gen <user_id> [days]".into())),
      }
    }

    Command::Buy { key, days } => {
      sv.license.extend(&key, days).await.map(|new_exp| {
        format!(
          "✅ Key extended by {days} days.\nNew expiry: <code>{}</code>",
          utils::format_date(new_exp)
        )
      })
    }

    Command::Ban(key) => {
      let result = sv.license.set_blocked(&key, true).await;
      if result.is_ok() {
        app.drop_sessions(&key);
      }
      result.map(|_| "🚫 Key blocked, sessions dropped".into())
    }

    Command::Unban(key) => sv
      .license
      .set_blocked(&key, false)
      .await
      .map(|_| "✅ Key unblocked".into()),

    Command::Info(input) => process_info_command(&sv, &app, &bot, input).await,
    Command::Backup => {
      if app.perform_backup(bot.chat_id).await.is_err() {
        bot.send_document(InputFile::file("licenses.db")).await?;
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
            utils::format_date(build.created_at)
          ));
          if let Some(changelog) = &build.changelog {
            text.push_str(&format!("<code>{}</code>\n", changelog));
          }
        }
        bot.reply_html(text).await?;
        return Ok(());
      }
      Ok(_) => Err(Error::BuildNotFound),
      Err(e) => Err(e),
    },

    Command::Publish { filename, version, changelog } => {
      let file_path = format!("{}/{}", app.config.builds_directory, filename);
      let path = Path::new(&file_path);

      if !path.exists() {
        Err(Error::InvalidArgs(format!(
          "File not found: {}\n\nUpload the file to the builds folder using scp:\nscp file.exe server:{}/",
          file_path, app.config.builds_directory
        )))
      } else {
        let changelog_opt =
          if changelog.is_empty() { None } else { Some(changelog) };

        sv.build.create(version.clone(), file_path, changelog_opt).await.map(
          |build| {
            format!(
              "✅ Build published!\n\n\
              <b>Version:</b> {}\n\
              <b>File:</b> {}\n\
              <b>Created:</b> {}",
              build.version,
              build.file_path,
              utils::format_date(build.created_at)
            )
          },
        )
      }
    }

    Command::Deactivate(version) => {
      async {
        let build =
          sv.build.by_version(&version).await?.ok_or(Error::BuildNotFound)?;
        if !build.is_active {
          return Err(Error::BuildInactive);
        }
        sv.build.deactivate(&version).await?;
        Ok(format!(
          "✅ Build deactivated.\n\n\
        <b>Version:</b> {}\n\
        <b>Downloads:</b> {}",
          build.version, build.downloads
        ))
      }
      .await
    }

    Command::Stats => Ok(format!(
      "Active Keys: {}\n\
       Active Sessions: {}",
      app.sessions.iter().map(|kv| kv.value().len()).sum::<usize>(),
      app.sessions.len()
    )),

    _ => return Ok(()),
  };

  match result {
    Ok(text) => {
      bot.reply_html(text).await?;
    }
    Err(e) => {
      bot.reply_html(format!("❌ {}", e.user_message())).await?;
    }
  }

  Ok(())
}
