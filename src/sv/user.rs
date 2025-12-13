use crate::{entity::user, prelude::*};

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

  pub async fn count(&self) -> Result<u64> {
    Ok(user::Entity::find().count(self.db).await?)
  }
}
