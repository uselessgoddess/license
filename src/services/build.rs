//! Build service - handles software version distribution

use chrono::Utc;
use sea_orm::{
  ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::entities::prelude::*;
use crate::error::{AppError, AppResult};

pub struct BuildService;

impl BuildService {
  /// Get the latest active build
  pub async fn get_latest(db: &DatabaseConnection) -> AppResult<Option<BuildModel>> {
    let build = Build::find()
      .filter(crate::entities::build::Column::IsActive.eq(true))
      .order_by_desc(crate::entities::build::Column::CreatedAt)
      .one(db)
      .await?;
    Ok(build)
  }

  /// Get build by version
  pub async fn get_by_version(
    db: &DatabaseConnection,
    version: &str,
  ) -> AppResult<Option<BuildModel>> {
    let build = Build::find()
      .filter(crate::entities::build::Column::Version.eq(version))
      .one(db)
      .await?;
    Ok(build)
  }

  /// Create a new build
  pub async fn create(
    db: &DatabaseConnection,
    version: String,
    file_path: String,
    changelog: Option<String>,
  ) -> AppResult<BuildModel> {
    let now = Utc::now().naive_utc();

    let build = BuildActiveModel {
      id: Set(Default::default()),
      version: Set(version),
      file_path: Set(file_path),
      changelog: Set(changelog),
      is_active: Set(true),
      created_at: Set(now),
      download_count: Set(0),
    };

    let build = build.insert(db).await?;
    Ok(build)
  }

  /// Increment download count
  pub async fn increment_downloads(db: &DatabaseConnection, version: &str) -> AppResult<()> {
    let build = Build::find()
      .filter(crate::entities::build::Column::Version.eq(version))
      .one(db)
      .await?
      .ok_or(AppError::BuildNotFound)?;

    let mut build: BuildActiveModel = build.into();
    build.download_count = Set(build.download_count.unwrap() + 1);
    build.update(db).await?;
    Ok(())
  }

  /// Deactivate a build
  pub async fn deactivate(db: &DatabaseConnection, version: &str) -> AppResult<()> {
    let build = Build::find()
      .filter(crate::entities::build::Column::Version.eq(version))
      .one(db)
      .await?
      .ok_or(AppError::BuildNotFound)?;

    let mut build: BuildActiveModel = build.into();
    build.is_active = Set(false);
    build.update(db).await?;
    Ok(())
  }

  /// List all builds
  pub async fn list_all(db: &DatabaseConnection) -> AppResult<Vec<BuildModel>> {
    let builds = Build::find()
      .order_by_desc(crate::entities::build::Column::CreatedAt)
      .all(db)
      .await?;
    Ok(builds)
  }
}
