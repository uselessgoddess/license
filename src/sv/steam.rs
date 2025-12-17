use crate::{entity::free_game, prelude::*};

pub struct Steam<'a> {
  db: &'a DatabaseConnection,
}

impl<'a> Steam<'a> {
  pub fn new(db: &'a DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn replace_free_games_cache(
    &self,
    items: Vec<(i32, i32, String)>,
  ) -> Result<()> {
    let txn = self.db.begin().await?;

    free_game::Entity::delete_many().exec(&txn).await?;

    if !items.is_empty() {
      let now = Utc::now().naive_utc();

      let models: Vec<_> = items
        .into_iter()
        .map(|(pkg_id, app_id, name)| free_game::ActiveModel {
          pkg_id: Set(pkg_id),
          app_id: Set(app_id),
          name: Set(name),
          updated_at: Set(now),
        })
        .collect();

      free_game::Entity::insert_many(models).exec(&txn).await?;
    }
    txn.commit().await?;

    Ok(())
  }

  pub async fn free_games(&self) -> Result<Vec<free_game::Model>> {
    Ok(free_game::Entity::find().all(self.db).await?)
  }
}
