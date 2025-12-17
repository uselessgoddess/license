pub mod cron;
pub mod server;
pub mod steam;
pub mod telegram;

use std::{sync::Arc, time::Duration};

use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::state::AppState;

#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
  fn name(&self) -> &'static str {
    std::any::type_name::<Self>()
  }

  async fn start(&self, app: Arc<AppState>) -> anyhow::Result<()>;
}

pub struct App {
  plugins: Vec<Arc<dyn Plugin>>,
}

impl App {
  pub fn new() -> Self {
    Self { plugins: Vec::new() }
  }

  pub fn register<P: Plugin + 'static>(mut self, plugin: P) -> Self {
    self.plugins.push(Arc::new(plugin));
    self
  }

  pub async fn run(self, app: Arc<AppState>) {
    for plugin in self.plugins {
      let app = app.clone();

      tokio::spawn(async move {
        let name = plugin.name();
        info!("SYSTEM: Service `{}` initialized", name);

        loop {
          let app = app.clone();
          let plugin = plugin.clone();

          let handle = tokio::spawn(async move { plugin.start(app).await });

          match handle.await {
            Ok(Ok(())) => {
              warn!("Service `{name}` stopped unexpectedly (Ok).",);
            }
            Ok(Err(err)) => {
              error!("Service `{name}` crashed with error: {err:#}.",);
            }
            Err(join_err) => {
              if join_err.is_cancelled() {
                info!("Service `{}` shutdown.", name);
                break;
              } else {
                error!("Service `{}` PANICKED!", name);
              }
            }
          }

          sleep(Duration::from_secs(5)).await;
          info!("SYSTEM: Restarting service `{}`...", name);
        }
      });
    }
  }
}
