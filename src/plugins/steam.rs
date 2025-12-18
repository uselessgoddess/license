use std::sync::Arc;

use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;

use crate::{plugins::Plugin, prelude::*, state::AppState};

#[derive(Debug, Deserialize)]
struct AppDetailsResponse {
  #[serde(default)]
  success: bool,
  data: Option<AppData>,
}

#[derive(Debug, Deserialize)]
struct AppData {
  #[serde(default)]
  name: String,
  #[serde(default)]
  package_groups: Vec<PackageGroup>,
}

#[derive(Debug, Deserialize)]
struct PackageGroup {
  #[serde(default)]
  subs: Vec<PackageSub>,
}

#[derive(Debug, Deserialize)]
struct PackageSub {
  packageid: u32,
  price_in_cents_with_discount: u32,
}

pub struct FreeGames;

#[async_trait]
impl Plugin for FreeGames {
  async fn start(&self, app: Arc<AppState>) -> anyhow::Result<()> {
    time::sleep(Duration::from_secs(10)).await;

    // TODO: configure user agent
    const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                              AppleWebKit/537.36 (KHTML, like Gecko) \
                              Chrome/91.0.4472.124 Safari/537.36";

    let client = Client::builder().user_agent(USER_AGENT).build()?;

    loop {
      info!("Scanning Steam for free games...");

      match scrape_games(&client).await {
        Ok(games) => {
          let count = games.len();
          info!("Found {} free packages. Updating DB...", count);

          if let Err(e) = app.sv().steam.replace_free_games_cache(games).await {
            error!("Failed to update DB cache: {}", e);
          } else {
            info!("DB cache updated successfully.");
          }
        }
        Err(e) => {
          error!("Steam scrape failed: {}", e);
        }
      }

      time::sleep(Duration::from_secs(12 * 3600)).await;
    }
  }
}

async fn scrape_games(
  client: &Client,
) -> anyhow::Result<Vec<(i32, i32, String)>> {
  let app_ids = fetch_free_app_ids(client).await?;
  let mut results = Vec::new();

  for app_id in app_ids {
    time::sleep(Duration::from_millis(250)).await;

    match get_free_game_details(client, app_id).await {
      Ok(Some((pkg_id, name))) => {
        results.push((pkg_id as i32, app_id as i32, name));
      }
      Ok(None) => {}
      Err(e) => warn!("Skipping app {}: {}", app_id, e),
    }
  }

  Ok(results)
}
async fn fetch_free_app_ids(client: &Client) -> anyhow::Result<Vec<u32>> {
  let url = "https://store.steampowered.com/search/results?sort_by=Price_ASC&force_infinite=1&specials=1&maxprice=free&ndl=1&snr=1_7_7_2300_7";
  let html = client.get(url).send().await?.text().await?;
  let document = Html::parse_document(&html);
  let selector = Selector::parse("a.search_result_row").unwrap();

  let mut ids = Vec::new();
  for element in document.select(&selector) {
    if let Some(attr) = element.value().attr("data-ds-appid")
      && let Some(first_part) = attr.split(',').next()
      && let Ok(id) = first_part.trim().parse::<u32>()
    {
      ids.push(id);
    }
  }
  Ok(ids)
}

async fn get_free_game_details(
  client: &Client,
  app_id: u32,
) -> anyhow::Result<Option<(u32, String)>> {
  let url =
    format!("https://store.steampowered.com/api/appdetails?appids={}", app_id);
  let resp: HashMap<String, AppDetailsResponse> =
    client.get(&url).send().await?.json().await?;

  if let Some(details) = resp.get(&app_id.to_string())
    && details.success
    && let Some(data) = &details.data
  {
    for group in &data.package_groups {
      for sub in &group.subs {
        if sub.price_in_cents_with_discount == 0 {
          return Ok(Some((sub.packageid, data.name.clone())));
        }
      }
    }
  }
  Ok(None)
}
