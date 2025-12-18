use crate::{
  entity::{license, user},
  prelude::*,
};

pub struct User<'a> {
  db: &'a DatabaseConnection,
}

impl<'a> User<'a> {
  pub fn new(db: &'a DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn get_or_create(&self, tg_user_id: i64) -> Result<user::Model> {
    if let Some(user) =
      user::Entity::find_by_id(tg_user_id).one(self.db).await?
    {
      return Ok(user);
    }

    let now = Utc::now().naive_utc();
    let user =
      user::ActiveModel { tg_user_id: Set(tg_user_id), reg_date: Set(now) };

    Ok(user.insert(self.db).await?)
  }

  pub async fn by_id(&self, tg_user_id: i64) -> Result<Option<user::Model>> {
    let user = user::Entity::find_by_id(tg_user_id).one(self.db).await?;
    Ok(user)
  }

  #[allow(dead_code)]
  pub async fn all(&self) -> Result<Vec<user::Model>> {
    let users = user::Entity::find()
      .order_by_asc(user::Column::RegDate)
      .all(self.db)
      .await?;
    Ok(users)
  }

  pub async fn all_with_licenses(
    &self,
  ) -> Result<Vec<(user::Model, Vec<license::Model>)>> {
    let users = user::Entity::find()
      .order_by_asc(user::Column::RegDate)
      .find_with_related(license::Entity)
      .all(self.db)
      .await?;
    Ok(users)
  }

  #[allow(dead_code)]
  pub async fn count(&self) -> Result<u64> {
    Ok(user::Entity::find().count(self.db).await?)
  }
}
