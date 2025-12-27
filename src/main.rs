#![allow(irrefutable_let_patterns)]

mod entity;
mod error;
mod plugins;
mod prelude;
mod state;
mod sv;
mod utils;

use std::{collections::HashSet, env, sync::Arc};

use tracing_subscriber::{
  EnvFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::{plugins::*, prelude::*, state::AppState};

/// Validate required environment variables and return detailed error messages
fn validate_env() -> Result<(), String> {
  let mut missing: Vec<&str> = Vec::new();
  let mut invalid: Vec<String> = Vec::new();

  // Required variables
  if env::var("ADMIN_IDS").is_err() {
    missing.push("ADMIN_IDS");
  } else {
    let admin_ids = env::var("ADMIN_IDS").unwrap();
    if admin_ids.trim().is_empty() {
      invalid.push("ADMIN_IDS: cannot be empty".to_string());
    } else {
      for (i, id) in admin_ids.split(',').enumerate() {
        if !id.trim().is_empty() && id.trim().parse::<i64>().is_err() {
          invalid.push(format!(
            "ADMIN_IDS: invalid integer at position {} ('{}')",
            i + 1,
            id.trim()
          ));
          break;
        }
      }
    }
  }

  if env::var("TELOXIDE_TOKEN").is_err() {
    missing.push("TELOXIDE_TOKEN");
  }

  if env::var("SERVER_SECRET").is_err() {
    missing.push("SERVER_SECRET");
  }

  if !missing.is_empty() || !invalid.is_empty() {
    let mut msg = String::new();
    if !missing.is_empty() {
      msg.push_str(&format!(
        "Missing environment variables: {}\n",
        missing.join(", ")
      ));
    }
    if !invalid.is_empty() {
      msg.push_str(&format!(
        "Invalid environment variables:\n  {}\n",
        invalid.join("\n  ")
      ));
    }
    msg.push_str("\nRequired environment variables:\n");
    msg.push_str(
      "  ADMIN_IDS      - Comma-separated list of Telegram admin user IDs\n",
    );
    msg.push_str("  TELOXIDE_TOKEN - Telegram Bot API token\n");
    msg.push_str("  SERVER_SECRET  - Secret key for server authentication\n");
    msg.push_str("\nOptional environment variables:\n");
    msg.push_str("  DATABASE_URL   - SQLite database URL (default: sqlite:licenses.db?mode=rwc)\n");
    msg.push_str(
      "  BASE_URL       - Server base URL (default: http://localhost:3000)\n",
    );
    return Err(msg);
  }

  Ok(())
}

#[tokio::main]
async fn main() {
  dotenvy::dotenv().ok();

  tracing_subscriber::registry()
    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
      "license=debug,tower_http=debug,axum=trace,sea_orm=warn".into()
    }))
    .with(tracing_subscriber::fmt::layer())
    .init();

  // Validate environment variables before proceeding
  if let Err(msg) = validate_env() {
    eprintln!("❌ Configuration error:\n\n{}", msg);
    std::process::exit(1);
  }

  let admins: HashSet<i64> = env::var("ADMIN_IDS")
    .expect("ADMIN_IDS not set")
    .split(',')
    .filter(|s| !s.trim().is_empty())
    .map(|id| id.trim().parse().expect("Invalid Admin ID format"))
    .collect();

  let db_url = env::var("DATABASE_URL")
    .unwrap_or_else(|_| "sqlite:licenses.db?mode=rwc".into());
  let token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
  let secret = env::var("SERVER_SECRET").expect("SERVER_SECRET not set");
  let base_url =
    env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());

  info!("Starting License Server v{}", env!("CARGO_PKG_VERSION"));

  let config = state::Config { base_url, ..Default::default() };

  let app_state = Arc::new(
    AppState::with_config(&db_url, &token, admins, secret, config).await,
  );

  App::new()
    // TODO: maybe its better to use single plugin
    .register(cron::GC)
    .register(cron::Sync)
    .register(cron::Backup)
    .register(cron::StatsClean)
    //
    .register(steam::FreeGames)
    .register(steam::FreeRewards)
    //
    .register(telegram::Plugin)
    .register(server::Plugin)
    .run(app_state)
    .await;

  wait_for_shutdown().await;
}

async fn wait_for_shutdown() {
  let ctrl_c = async {
    tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
      .expect("failed to install signal handler")
      .recv()
      .await;
  };

  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  tokio::select! {
      _ = ctrl_c => {},
      _ = terminate => {},
  }
}
