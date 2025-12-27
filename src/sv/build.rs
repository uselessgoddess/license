use crate::{entity::*, prelude::*};

pub struct Build<'a> {
  db: &'a DatabaseConnection,
}

impl<'a> Build<'a> {
  pub fn new(db: &'a DatabaseConnection) -> Self {
    Self { db }
  }

  #[allow(dead_code)]
  pub async fn latest(&self) -> Result<Option<build::Model>> {
    let build = build::Entity::find()
      .filter(build::Column::IsActive.eq(true))
      .order_by_desc(build::Column::CreatedAt)
      .one(self.db)
      .await?;
    Ok(build)
  }

  pub async fn by_version(
    &self,
    version: &str,
  ) -> Result<Option<build::Model>> {
    let build = build::Entity::find()
      .filter(build::Column::Version.eq(version))
      .one(self.db)
      .await?;
    Ok(build)
  }

  pub async fn create(
    &self,
    version: String,
    file_path: String,
    changelog: Option<String>,
  ) -> Result<build::Model> {
    let now = Utc::now().naive_utc();

    let build = build::ActiveModel {
      id: NotSet,
      version: Set(version),
      file_path: Set(file_path),
      changelog: Set(changelog),
      is_active: Set(true),
      created_at: Set(now),
      downloads: Set(0),
    };

    Ok(build.insert(self.db).await?)
  }

  pub async fn increment_downloads(&self, version: &str) -> Result<()> {
    let build = build::Entity::find()
      .filter(build::Column::Version.eq(version))
      .one(self.db)
      .await?
      .ok_or(Error::BuildNotFound)?;

    build::ActiveModel { downloads: Set(build.downloads + 1), ..build.into() }
      .update(self.db)
      .await?;

    Ok(())
  }

  pub async fn deactivate(&self, version: &str) -> Result<()> {
    let build = build::Entity::find()
      .filter(build::Column::Version.eq(version))
      .one(self.db)
      .await?
      .ok_or(Error::BuildNotFound)?;

    build::ActiveModel { is_active: Set(false), ..build.into() }
      .update(self.db)
      .await?;

    Ok(())
  }

  /// Reactivate (un-yank) a previously yanked build
  pub async fn activate(&self, version: &str) -> Result<()> {
    let build = build::Entity::find()
      .filter(build::Column::Version.eq(version))
      .one(self.db)
      .await?
      .ok_or(Error::BuildNotFound)?;

    build::ActiveModel { is_active: Set(true), ..build.into() }
      .update(self.db)
      .await?;

    Ok(())
  }

  pub async fn all(&self) -> Result<Vec<build::Model>> {
    let builds = build::Entity::find()
      .order_by_desc(build::Column::CreatedAt)
      .all(self.db)
      .await?;

    Ok(builds)
  }

  /// Get all active builds (available for download)
  pub async fn active(&self) -> Result<Vec<build::Model>> {
    let builds = build::Entity::find()
      .filter(build::Column::IsActive.eq(true))
      .order_by_desc(build::Column::CreatedAt)
      .all(self.db)
      .await?;

    Ok(builds)
  }

  #[allow(dead_code)]
  pub async fn count(&self) -> Result<u64> {
    Ok(build::Entity::find().count(self.db).await?)
  }

  #[allow(dead_code)]
  pub async fn total_downloads(&self) -> Result<u64> {
    use sea_orm::sea_query::Expr;

    let result: Option<i64> = build::Entity::find()
      .select_only()
      .column_as(Expr::col(build::Column::Downloads).sum(), "total")
      .into_tuple()
      .one(self.db)
      .await?;

    Ok(result.unwrap_or(0) as u64)
  }
}
