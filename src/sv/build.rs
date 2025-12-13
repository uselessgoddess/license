use crate::{entity::*, prelude::*};

#[allow(dead_code)]
pub struct Build<'a> {
  db: &'a DatabaseConnection,
}

#[allow(dead_code)]
impl<'a> Build<'a> {
  pub fn new(db: &'a DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn latest(&self) -> Result<Option<build::Model>> {
    let build = build::Entity::find()
      .filter(build::Column::IsActive.eq(true))
      .order_by_desc(build::Column::CreatedAt)
      .one(self.db)
      .await?;
    Ok(build)
  }

  pub async fn by_version(
    db: &DatabaseConnection,
    version: &str,
  ) -> Result<Option<build::Model>> {
    let build = build::Entity::find()
      .filter(build::Column::Version.eq(version))
      .one(db)
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
      id: Set(Default::default()),
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

  pub async fn all(&self) -> Result<Vec<build::Model>> {
    let builds = build::Entity::find()
      .order_by_desc(build::Column::CreatedAt)
      .all(self.db)
      .await?;

    Ok(builds)
  }
}
