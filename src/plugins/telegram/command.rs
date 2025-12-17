use std::{path::Path, sync::Arc};

use futures::future;
use teloxide::{
  prelude::*,
  types::InputFile,
  utils::command::{BotCommands, ParseError},
};

use super::ReplyBot;
use crate::{entity::license::LicenseType, prelude::*, state::AppState};

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

async fn handle_admin_command(
  app: Arc<AppState>,
  bot: ReplyBot,
  cmd: Command,
) -> ResponseResult<()> {
  let sv = app.sv();

  if let Command::Users = cmd {
    let users = match sv.user.all().await {
      Ok(u) => u,
      Err(e) => {
        bot.reply_html(format!("❌ DB Error: {}", e)).await?;
        return Ok(());
      }
    };

    if users.is_empty() {
      bot.reply_html("There is no users.").await?;
      return Ok(());
    }

    bot
      .reply_html(format!("⏳ Found {} users. Getting names...", users.len()))
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

    bot.reply_html(response_text).await?;
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

    Command::Info(key) => {
      async {
        let active = app.sessions.get(&key).map(|s| s.len()).unwrap_or(0);
        let license =
          sv.license.by_key(&key).await?.ok_or(Error::LicenseNotFound)?;
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
          utils::format_date(license.expires_at),
        ))
      }
      .await
    }
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
