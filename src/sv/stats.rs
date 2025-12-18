use std::io::Read;

use base64::Engine;
use flate2::read::GzDecoder;
use json::json;
use serde::{Deserialize, Serialize};

use crate::{entity::*, prelude::*, sv};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct MetaStats {
  #[serde(default)]
  pub performance: PerformanceMeta,
  #[serde(default)]
  pub network: NetworkMeta,
  #[serde(default)]
  pub states: HashMap<String, f64>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct PerformanceMeta {
  pub avg_fps: f64,
  pub avg_ram_mb: u32,
  pub avg_ai_ms: f32,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct NetworkMeta {
  pub routes: Vec<String>,
  pub avg_ping: u32,
  #[serde(default)]
  pub gc_timeouts: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MetricEvent {
  #[serde(rename = "shutdown")]
  Shutdown { uptime: f64 },
  #[serde(rename = "state")]
  State { state: String, duration: f64 },
  #[serde(rename = "srt")]
  Srt { routes: Vec<String> },
  #[serde(rename = "performance")]
  Performance {
    avg_fps: Option<f64>,
    avg_ram_mb: Option<u32>,
    avg_ai_ms: Option<f32>,
  },
}

#[derive(Debug, Deserialize)]
pub struct MetricPayload {
  #[serde(rename = "type")]
  pub event_type: String,
  pub license_key: String,
  pub data: json::Value,
}

#[derive(Debug, Serialize)]
pub struct UserStatsDisplay {
  pub weekly_xp: u64,
  pub total_xp: u64,
  pub drops_count: u32,
  pub instances: u32,
  pub runtime_hours: f64,
  pub meta: Option<MetaStats>,
}

pub struct Stats<'a> {
  db: &'a DatabaseConnection,
}

impl<'a> Stats<'a> {
  pub fn new(db: &'a DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn get_or_create(&self, tg_user_id: i64) -> Result<stats::Model> {
    if let Some(stats) =
      stats::Entity::find_by_id(tg_user_id).one(self.db).await?
    {
      return Ok(stats);
    }

    sv::User::new(self.db).get_or_create(tg_user_id).await?;

    let now = Utc::now().naive_utc();
    let stats = stats::ActiveModel {
      tg_user_id: Set(tg_user_id),
      weekly_xp: Set(0),
      total_xp: Set(0),
      drops_count: Set(0),
      instances: Set(0),
      runtime_hours: Set(0.0),
      last_updated: Set(now),
      meta: Set(None),
    };

    Ok(stats.insert(self.db).await?)
  }

  pub async fn process_metric(&self, raw_base64: &str) -> Result<()> {
    let compressed = base64::prelude::BASE64_STANDARD
      .decode(raw_base64)
      .map_err(|_| Error::InvalidArgs("Invalid base64".into()))?;

    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut json_str = String::new();
    decoder.read_to_string(&mut json_str).map_err(|e| {
      Error::InvalidArgs(format!("Decompression failed: {}", e))
    })?;

    let payload: MetricPayload = json::from_str(&json_str)
      .map_err(|e| Error::InvalidArgs(format!("Invalid JSON: {}", e)))?;

    let license = sv::License::new(self.db)
      .by_key(&payload.license_key)
      .await?
      .ok_or(Error::LicenseNotFound)?;

    let stats = self.get_or_create(license.tg_user_id).await?;
    let mut meta: MetaStats = match &stats.meta {
      Some(val) => json::from_value(val.clone()).unwrap_or_default(),
      None => MetaStats::default(),
    };

    let event_json = json!({
      "type": payload.event_type,
      "data": payload.data
    });

    let event: MetricEvent = json::from_value(event_json).map_err(|e| {
      Error::InvalidArgs(format!("Unknown event format: {}", e))
    })?;

    let mut model: stats::ActiveModel = stats.clone().into();

    match event {
      MetricEvent::Shutdown { uptime } => {
        model.runtime_hours = Set(stats.runtime_hours + (uptime / 3600.0));
      }
      MetricEvent::State { state, duration } => {
        *meta.states.entry(state).or_insert(0.0) += duration;
      }
      MetricEvent::Srt { routes } => {
        meta.network.routes = routes;
      }
      MetricEvent::Performance { avg_fps, avg_ram_mb, avg_ai_ms } => {
        if let Some(fps) = avg_fps {
          meta.performance.avg_fps = fps;
        }
        if let Some(ram) = avg_ram_mb {
          meta.performance.avg_ram_mb = ram;
        }
        if let Some(ai) = avg_ai_ms {
          meta.performance.avg_ai_ms = ai;
        }
      }
    }

    let now = Utc::now().naive_utc();
    model.last_updated = Set(now);
    model.meta = Set(Some(json::to_value(meta).unwrap()));

    model.update(self.db).await?;

    Ok(())
  }

  pub async fn display_stats(
    &self,
    tg_user_id: i64,
  ) -> Result<UserStatsDisplay> {
    let stats = self.get_or_create(tg_user_id).await?;

    let meta: Option<MetaStats> =
      stats.meta.map(|v| json::from_value(v).unwrap_or_default());

    Ok(UserStatsDisplay {
      weekly_xp: stats.weekly_xp as u64,
      total_xp: stats.total_xp as u64,
      drops_count: stats.drops_count as u32,
      instances: stats.instances as u32,
      runtime_hours: stats.runtime_hours,
      meta,
    })
  }
  pub async fn reset_weekly_xp(db: &DatabaseConnection) -> Result<()> {
    use sea_orm::sea_query::Expr;

    stats::Entity::update_many()
      .col_expr(stats::Column::WeeklyXp, Expr::value(0i64))
      .exec(db)
      .await?;

    Ok(())
  }

  #[allow(dead_code)]
  pub async fn aggregate(&self) -> Result<AggregatedStats> {
    use sea_orm::sea_query::Expr;

    type StatsRow = (Option<i64>, Option<i64>, Option<i64>, Option<f64>);
    let result: Option<StatsRow> = stats::Entity::find()
      .select_only()
      .column_as(Expr::col(stats::Column::TotalXp).sum(), "total_xp")
      .column_as(Expr::col(stats::Column::WeeklyXp).sum(), "weekly_xp")
      .column_as(Expr::col(stats::Column::DropsCount).sum(), "drops")
      .column_as(Expr::col(stats::Column::RuntimeHours).sum(), "runtime")
      .into_tuple()
      .one(self.db)
      .await?;

    let active_instances: Option<i64> = stats::Entity::find()
      .select_only()
      .column_as(Expr::col(stats::Column::Instances).sum(), "instances")
      .into_tuple()
      .one(self.db)
      .await?;

    Ok(AggregatedStats {
      total_xp: result.and_then(|r| r.0).unwrap_or(0) as u64,
      weekly_xp: result.and_then(|r| r.1).unwrap_or(0) as u64,
      total_drops: result.and_then(|r| r.2).unwrap_or(0) as u64,
      total_runtime_hours: result.and_then(|r| r.3).unwrap_or(0.0),
      active_instances: active_instances.unwrap_or(0) as u32,
    })
  }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AggregatedStats {
  pub total_xp: u64,
  pub weekly_xp: u64,
  pub total_drops: u64,
  pub total_runtime_hours: f64,
  pub active_instances: u32,
}
