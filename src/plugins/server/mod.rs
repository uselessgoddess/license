mod handlers;
mod steam;
mod telegram_proxy;

use std::{fs, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use axum::{
  Router,
  extract::DefaultBodyLimit,
  routing::{get, post},
};
use tower::ServiceBuilder;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::{
  cors::{Any, CorsLayer},
  services::ServeDir,
  trace::TraceLayer,
};

use crate::{prelude::*, state::AppState};

pub struct Plugin;

#[async_trait]
impl super::Plugin for Plugin {
  async fn start(&self, app: Arc<AppState>) -> anyhow::Result<()> {
    let governor_conf = Arc::new(
      GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(100)
        .finish()
        .context("Failed to build rate limiter config")?,
    );

    let limiter = governor_conf.limiter().clone();

    let telegram_governor_conf = Arc::new(
      GovernorConfigBuilder::default()
        .per_second(20)
        .burst_size(50)
        .finish()
        .context("Failed to build telegram rate limiter config")?,
    );

    let telegram_limiter = telegram_governor_conf.limiter().clone();

    let telegram_routes = Router::new()
      .route("/send-message", post(telegram_proxy::send_message))
      .route("/send-photo", post(telegram_proxy::send_photo))
      .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
      .layer(GovernorLayer::new(telegram_governor_conf));

    if !fs::exists("models")? {
      info!("Creating missing ./models directory");
      let _ = fs::create_dir("models");
    }

    let router = Router::new()
      .nest("/api/telegram", telegram_routes)
      .route("/health", get(handlers::health))
      .route("/api/download", get(handlers::download))
      .route("/api/heartbeat", post(handlers::heartbeat))
      .route("/api/logout", post(handlers::logout))
      .route("/api/metrics", post(handlers::submit_metrics))
      .nest_service("/api/models", ServeDir::new("models"))
      .nest_service("/api/data", ServeDir::new("data"))
      .layer(TraceLayer::new_for_http())
      // TODO: split configuration
      .route("/api/cache/steam/free-games", get(steam::free_games))
      .route("/api/cache/steam/free-items", get(steam::free_items))
      .layer(
        ServiceBuilder::new()
          .layer(TraceLayer::new_for_http())
          .layer(GovernorLayer::new(governor_conf))
          .layer(
            CorsLayer::new()
              .allow_origin(Any)
              .allow_methods(Any)
              .allow_headers(Any),
          ),
      )
      .with_state(app)
      .into_make_service_with_connect_info::<SocketAddr>();

    let port: u16 =
      std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("HTTP Server listening on {addr}");

    let limiter = async {
      loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        limiter.retain_recent();
        telegram_limiter.retain_recent();
      }
    };

    let server = async {
      axum::serve(listener, router).await.context("Axum server error")
    };

    tokio::select! {
      result = server => {
        match &result {
            Ok(_) => info!("Server stopped gracefully"),
            Err(err) => error!("Server stopped with error: {err}"),
        }
        result
      }
      _ = limiter => {
        error!("Rate limiter cleaner stopped unexpectedly!");
        Ok(())
      }
    }
  }
}
