use std::{path::Path, sync::Arc};

use reqwest::Url;
use teloxide::{
  prelude::*,
  types::{InlineKeyboardButton, InlineKeyboardMarkup},
};

use super::ReplyBot;
use crate::{
  prelude::*,
  state::{AppState, Services},
};

/// Callback data enum - provides type-safe callback handling
#[derive(Debug, Clone, PartialEq)]
pub enum Callback {
  Profile,
  License,
  Trial,
  Download,
  DownloadVersion(String),
  Buy,
  PayManual,
  Back,
}

impl Callback {
  /// Serialize callback to string for Telegram API
  pub fn to_data(&self) -> String {
    match self {
      Callback::Profile => "profile".to_string(),
      Callback::License => "license".to_string(),
      Callback::Trial => "trial".to_string(),
      Callback::Download => "download".to_string(),
      Callback::DownloadVersion(v) => format!("dl_ver:{}", v),
      Callback::Buy => "buy".to_string(),
      Callback::PayManual => "pay_man".to_string(),
      Callback::Back => "back".to_string(),
    }
  }

  /// Parse callback from string received from Telegram API
  pub fn from_data(data: &str) -> Option<Self> {
    match data {
      "profile" => Some(Callback::Profile),
      "license" => Some(Callback::License),
      "trial" => Some(Callback::Trial),
      "download" => Some(Callback::Download),
      "buy" => Some(Callback::Buy),
      "pay_man" => Some(Callback::PayManual),
      "back" => Some(Callback::Back),
      _ if data.starts_with("dl_ver:") => {
        Some(Callback::DownloadVersion(data[7..].to_string()))
      }
      _ => None,
    }
  }
}

pub fn main_menu(is_promo: bool) -> InlineKeyboardMarkup {
  let mut rows = vec![
    vec![InlineKeyboardButton::callback(
      "👤 My Profile",
      Callback::Profile.to_data(),
    )],
    vec![InlineKeyboardButton::callback(
      "🔑 My License",
      Callback::License.to_data(),
    )],
    vec![InlineKeyboardButton::callback(
      "💳 Buy License",
      Callback::Buy.to_data(),
    )],
    vec![InlineKeyboardButton::callback(
      "📥 Download Panel",
      Callback::Download.to_data(),
    )],
  ];

  if is_promo {
    rows.push(vec![InlineKeyboardButton::callback(
      "🆓 Get Free Trial",
      Callback::Trial.to_data(),
    )]);
  }

  InlineKeyboardMarkup::new(rows)
}

fn payment_method_menu() -> InlineKeyboardMarkup {
  InlineKeyboardMarkup::new(vec![
    vec![InlineKeyboardButton::callback(
      "👤 Manual Purchase",
      Callback::PayManual.to_data(),
    )],
    // vec![InlineKeyboardButton::callback("CryptoBot (Auto)", CB_PAY_CRYPTO)],
    vec![InlineKeyboardButton::callback(
      "« Back to Menu",
      Callback::Back.to_data(),
    )],
  ])
}

fn back_keyboard() -> InlineKeyboardMarkup {
  InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
    "« Back to Menu",
    Callback::Back.to_data(),
  )]])
}

pub async fn handle(
  app: Arc<AppState>,
  bot: ReplyBot,
  data: &str,
) -> ResponseResult<()> {
  let sv = app.sv();

  let Some(callback) = Callback::from_data(data) else {
    return Ok(());
  };

  match callback {
    Callback::Profile => {
      handle_profile_view(&sv, &bot).await?;
    }
    Callback::License => {
      handle_license_edit(&sv, &bot).await?;
    }
    Callback::Trial => {
      handle_trial_claim(&sv, &bot).await?;
    }
    Callback::Download => {
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
    Callback::Buy => {
      let text = "💳 <b>Purchase License</b>\n\n\
        Select a payment method below.";
      bot.edit_with_keyboard(text, payment_method_menu()).await?;
    }
    Callback::PayManual => {
      let text = "👤 <b>Manual Purchase</b>\n\n\
        To purchase a license via USDT or other methods, please contact our support:\n\n\
        👉 @y_a_c_s_p\n\n\
        <i>Send a message with \"I want to buy license\"</i>";

      let kb = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::url(
          "Open Chat with Support",
          Url::parse("https://t.me/y_a_c_s_p").expect("invalid link, what???"),
        )],
        vec![InlineKeyboardButton::callback("« Back", Callback::Buy.to_data())],
      ]);

      bot.edit_with_keyboard(text, kb).await?;
    }
    Callback::Back => {
      let text = "<b>Yet Another Counter Strike Panel!</b>\n\n\
        Use the buttons below to navigate.\n\
        Read docs: https://yacsp.gitbook.io/yacsp\n\
        Contact support: @y_a_c_s_p";
      bot
        .edit_with_keyboard(text, main_menu(sv.license.is_promo_active()))
        .await?;
    }
    Callback::DownloadVersion(version) => {
      handle_download_version(&sv, &bot, &app, &version).await?;
    }
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
    // Базовая стата
    text.push_str(&format!(
      "\n\n<b>📊 Farming Stats:</b>\n\
        Weekly XP: {}\n\
        Total XP: {}\n\
        Drops: {}\n\
        Runtime: {:.1}h",
      s.weekly_xp, s.total_xp, s.drops_count, s.runtime_hours
    ));

    if let Some(meta) = s.meta {
      if !meta.network.routes.is_empty() {
        text.push_str(&format!(
          "\n🌐 <b>Routes:</b> {}",
          meta.network.routes.join(", ")
        ));
      }

      if meta.performance.avg_fps > 0.0 {
        text.push_str(&format!(
          "\n🚀 <b>Perf:</b> {:.0} FPS | {} MB",
          meta.performance.avg_fps, meta.performance.avg_ram_mb
        ));
      }

      let mut states: Vec<_> = meta.states.clone().into_iter().collect();
      states.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

      if let Some((top_state, duration)) = states.first() {
        text.push_str(&format!(
          "\n⏳ <b>Top State:</b> {top_state} ({:.1}h)",
          *duration / 3600.0
        ));
      }
    }
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
  let builds = sv.build.active().await.unwrap_or_default();

  if builds.is_empty() {
    bot
      .edit_with_keyboard(
        "❌ No builds available yet. Contact support.",
        back_keyboard(),
      )
      .await?;
    return Ok(());
  }

  // If only one version available, download directly
  if builds.len() == 1 {
    return handle_download_version(sv, bot, app, &builds[0].version).await;
  }

  // Multiple versions - show selection menu
  let mut rows = Vec::new();
  for build in &builds {
    let label = if Some(build.id) == builds.first().map(|b| b.id) {
      format!("📥 v{} (latest)", build.version)
    } else {
      format!("📥 v{}", build.version)
    };
    rows.push(vec![InlineKeyboardButton::callback(
      label,
      Callback::DownloadVersion(build.version.clone()).to_data(),
    )]);
  }
  rows.push(vec![InlineKeyboardButton::callback(
    "« Back to Menu",
    Callback::Back.to_data(),
  )]);

  let text = "📥 <b>Select Version</b>\n\n\
    Choose which version to download:";

  bot.edit_with_keyboard(text, InlineKeyboardMarkup::new(rows)).await?;

  Ok(())
}

async fn handle_download_version(
  sv: &Services<'_>,
  bot: &ReplyBot,
  app: &AppState,
  version: &str,
) -> ResponseResult<()> {
  match sv.build.by_version(version).await {
    Ok(Some(build)) if build.is_active => {
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
          "❌ Build not available. Contact support.",
          back_keyboard(),
        )
        .await?;
    }
  }

  Ok(())
}
