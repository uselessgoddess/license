//! Stats service - handles user statistics and system telemetry

use chrono::Utc;
use flate2::read::GzDecoder;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use std::io::Read;

use crate::entities::prelude::*;
use crate::error::{AppError, AppResult};
use crate::services::UserService;

/// Compressed stats payload from CS2 panel
#[derive(Debug, Deserialize)]
pub struct SystemStats {
  pub session_id: String,
  pub hwid_hash: String,
  pub app_version: String,
  pub uptime: u64,
  pub performance: PerformanceStats,
  pub farming: FarmingStats,
  pub network: NetworkStats,
  #[serde(default)]
  pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PerformanceStats {
  pub avg_fps: f32,
  pub avg_ram_mb: u32,
  pub avg_ai_ms: u32,
}

#[derive(Debug, Deserialize)]
pub struct FarmingStats {
  pub cycle_time: u32,
  #[serde(default)]
  pub state_stucks: std::collections::HashMap<String, u32>,
  #[serde(default)]
  pub xp_gained: i64,
  #[serde(default)]
  pub drops: i32,
}

#[derive(Debug, Deserialize)]
pub struct NetworkStats {
  #[serde(default)]
  pub srt: std::collections::HashMap<String, ServerRegionStats>,
  pub avg_ping: u32,
  #[serde(default)]
  pub gc_timeouts: u32,
}

#[derive(Debug, Deserialize)]
pub struct ServerRegionStats {
  pub ping: u32,
}

/// Aggregated stats for display
#[derive(Debug, Serialize)]
pub struct UserStatsDisplay {
  pub weekly_xp: i64,
  pub total_xp: i64,
  pub drops_count: i32,
  pub active_instances: i32,
  pub total_runtime_hours: f64,
}

pub struct StatsService;

impl StatsService {
  /// Get or create user stats
  pub async fn get_or_create(db: &DatabaseConnection, tg_user_id: i64) -> AppResult<UserStatsModel> {
    if let Some(stats) = UserStats::find_by_id(tg_user_id).one(db).await? {
      return Ok(stats);
    }

    // Ensure user exists first
    UserService::get_or_create(db, tg_user_id, None).await?;

    let now = Utc::now().naive_utc();
    let stats = UserStatsActiveModel {
      tg_user_id: Set(tg_user_id),
      weekly_xp: Set(0),
      total_xp: Set(0),
      drops_count: Set(0),
      active_instances: Set(0),
      total_runtime_hours: Set(0.0),
      last_updated: Set(now),
    };

    let stats = stats.insert(db).await?;
    Ok(stats)
  }

  /// Update stats from heartbeat/telemetry
  pub async fn update_from_telemetry(
    db: &DatabaseConnection,
    tg_user_id: i64,
    stats: &SystemStats,
    active_instances: i32,
  ) -> AppResult<()> {
    let existing = Self::get_or_create(db, tg_user_id).await?;
    let now = Utc::now().naive_utc();

    let new_xp = stats.farming.xp_gained;
    let new_drops = stats.farming.drops;
    let runtime_hours = stats.uptime as f64 / 3600.0;

    let mut model: UserStatsActiveModel = existing.into();
    model.weekly_xp = Set(model.weekly_xp.unwrap() + new_xp);
    model.total_xp = Set(model.total_xp.unwrap() + new_xp);
    model.drops_count = Set(model.drops_count.unwrap() + new_drops);
    model.active_instances = Set(active_instances);
    model.total_runtime_hours = Set(model.total_runtime_hours.unwrap() + runtime_hours);
    model.last_updated = Set(now);

    model.update(db).await?;
    Ok(())
  }

  /// Get user stats for display
  pub async fn get_display_stats(
    db: &DatabaseConnection,
    tg_user_id: i64,
  ) -> AppResult<UserStatsDisplay> {
    let stats = Self::get_or_create(db, tg_user_id).await?;

    Ok(UserStatsDisplay {
      weekly_xp: stats.weekly_xp,
      total_xp: stats.total_xp,
      drops_count: stats.drops_count,
      active_instances: stats.active_instances,
      total_runtime_hours: stats.total_runtime_hours,
    })
  }

  /// Decompress gzip stats payload
  pub fn decompress_stats(compressed: &[u8]) -> AppResult<SystemStats> {
    let mut decoder = GzDecoder::new(compressed);
    let mut decompressed = String::new();
    decoder.read_to_string(&mut decompressed).map_err(|e| {
      AppError::Internal(format!("Failed to decompress stats: {}", e))
    })?;

    json::from_str(&decompressed).map_err(|e| {
      AppError::Internal(format!("Failed to parse stats JSON: {}", e))
    })
  }

  /// Reset weekly XP (should be called by a scheduled job)
  pub async fn reset_weekly_xp(db: &DatabaseConnection) -> AppResult<()> {
    use sea_orm::sea_query::Expr;

    crate::entities::user_stats::Entity::update_many()
      .col_expr(crate::entities::user_stats::Column::WeeklyXp, Expr::value(0i64))
      .exec(db)
      .await?;

    Ok(())
  }
}
