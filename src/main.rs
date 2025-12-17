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

#[tokio::main]
async fn main() {
  dotenvy::dotenv().ok();

  tracing_subscriber::registry()
    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
      "license=debug,tower_http=debug,axum=trace,sea_orm=warn".into()
    }))
    .with(tracing_subscriber::fmt::layer())
    .init();

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
