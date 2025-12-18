use json::Value;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::user;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "user_stats")]
pub struct Model {
  #[sea_orm(primary_key, auto_increment = false)]
  pub tg_user_id: i64,
  pub weekly_xp: i64,
  pub total_xp: i64,
  pub drops_count: i32,
  pub runtime_hours: f64,
  pub instances: i32,
  pub last_updated: DateTime,
  /// json stats metadata
  pub meta: Option<Value>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
  #[sea_orm(
    belongs_to = "user::Entity",
    from = "Column::TgUserId",
    to = "user::Column::TgUserId"
  )]
  User,
}

impl Related<user::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::User.def()
  }
}

impl ActiveModelBehavior for ActiveModel {}
