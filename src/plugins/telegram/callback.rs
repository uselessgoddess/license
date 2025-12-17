use std::{path::Path, sync::Arc};

use teloxide::{
  prelude::*,
  types::{InlineKeyboardButton, InlineKeyboardMarkup},
};

use super::ReplyBot;
use crate::{
  prelude::*,
  state::{AppState, Services},
};

const CB_PROFILE: &str = "profile";
const CB_LICENSE: &str = "license";
const CB_TRIAL: &str = "trial";
const CB_DOWNLOAD: &str = "download";
const CB_BACK: &str = "back";

pub fn main_menu(is_promo: bool) -> InlineKeyboardMarkup {
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

  InlineKeyboardMarkup::new(rows)
}

fn back_keyboard() -> InlineKeyboardMarkup {
  InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
    "« Back to Menu",
    CB_BACK,
  )]])
}

pub async fn handle(
  app: Arc<AppState>,
  bot: ReplyBot,
  data: &str,
) -> ResponseResult<()> {
  let sv = app.sv();

  match data {
    CB_PROFILE => {
      handle_profile_view(&sv, &bot).await?;
    }
    CB_LICENSE => {
      handle_license_edit(&sv, &bot).await?;
    }
    CB_TRIAL => {
      handle_trial_claim(&sv, &bot).await?;
    }
    CB_DOWNLOAD => {
      if let Ok(keys) = sv.license.by_user(bot.chat_id.0, false).await
        && !keys.is_empty()
      {
        handle_download(&sv, &bot, &app).await?;
      } else {
        bot
          .edit_with_keyboard("You have no active license!", back_keyboard())
          .await?;
      }
    }
    CB_BACK => {
      let text = "<b>Yet Another Counter Strike Panel!</b>\n\n\
        Use the buttons below to navigate.\n\
        Read docs: https://yacsp.gitbook.io/yacsp\n\
        Contact support: @y_a_c_s_p";
      bot
        .edit_with_keyboard(text, main_menu(sv.license.is_promo_active()))
        .await?;
    }
    _ => {}
  }

  Ok(())
}

async fn handle_profile_view(
  sv: &Services<'_>,
  bot: &ReplyBot,
) -> ResponseResult<()> {
  let user = sv.user.by_id(bot.user_id).await.ok().flatten();

  let reg_date = match user {
    Some(u) => utils::format_date(u.reg_date),
    None => "Unknown".into(),
  };

  let stats = sv.stats.display_stats(bot.user_id).await.ok();

  let mut text = format!(
    "👤 <b>My Profile</b>\n\n\
    <b>User ID:</b> <code>{}</code>\n\
    <b>Registered:</b> {}",
    bot.user_id, reg_date
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

  bot.edit_with_keyboard(text, back_keyboard()).await?;

  Ok(())
}

async fn handle_license_edit(
  sv: &Services<'_>,
  bot: &ReplyBot,
) -> ResponseResult<()> {
  let now = Utc::now().naive_utc();

  match sv.license.by_user(bot.user_id, false).await {
    Ok(licenses) if !licenses.is_empty() => {
      let mut text = String::from("🔑 <b>Your Licenses:</b>\n");

      for license in licenses {
        let status = if license.expires_at > now {
          format!("⏳ {}", utils::format_duration(license.expires_at - now))
        } else {
          "❌ Expired".into()
        };

        text.push_str(&format!(
          "\n<code>{}</code>\n{} | {:?}\n",
          license.key, status, license.license_type
        ));
      }

      bot.edit_with_keyboard(text, back_keyboard()).await?;
    }
    _ => {
      bot
        .edit_with_keyboard("You have no active license!", back_keyboard())
        .await?;
    }
  }

  Ok(())
}

async fn handle_trial_claim(
  sv: &Services<'_>,
  bot: &ReplyBot,
) -> ResponseResult<()> {
  let promo_name = "first_promo";

  match sv.license.claim_promo(bot.user_id, promo_name).await {
    Ok(license) => {
      let text = format!(
        "🎉 <b>Success!</b>\n\n\
        Here is your FREE week license:\n\
        <code>{}</code>\n\n\
        Download the software using the Download button!",
        license.key
      );
      bot.reply_with_keyboard(text, back_keyboard()).await?;
    }
    Err(e) => {
      let msg = match e {
        Error::Promo(Promo::Inactive) => "Promo is not active right now.",
        Error::Promo(Promo::Claimed) => "You have already claimed this promo",
        _ => "An error occurred.",
      };
      bot.reply_with_keyboard(msg, back_keyboard()).await?;
    }
  }

  Ok(())
}

async fn handle_download(
  sv: &Services<'_>,
  bot: &ReplyBot,
  app: &AppState,
) -> ResponseResult<()> {
  match sv.build.latest().await {
    Ok(Some(build)) => {
      let path = Path::new(&build.file_path);
      if path.exists() {
        let token = app.create_download_token(&build.version);
        let download_url =
          format!("{}/api/download?token={}", app.config.base_url, token);

        let text = format!(
          "<b>YACS Panel v{}</b>\n\n\
          {}\n\n\
          📥 <a href=\"{}\">Click here to download</a>\n\n\
          <i>⚠️ Link expires in 10 minutes</i>",
          build.version,
          build.changelog.as_deref().unwrap_or(""),
          download_url
        );

        bot.edit_with_keyboard(text, back_keyboard()).await?;
      } else {
        bot
          .edit_with_keyboard(
            "❌ Build file not found. Contact support.",
            back_keyboard(),
          )
          .await?;
      }
    }
    _ => {
      bot
        .edit_with_keyboard(
          "❌ No builds available yet. Contact support.",
          back_keyboard(),
        )
        .await?;
    }
  }

  Ok(())
}
