//! User entity - stores Telegram user information

use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
  #[sea_orm(primary_key, auto_increment = false)]
  pub tg_user_id: i64,
  pub username: Option<String>,
  pub reg_date: NaiveDateTime,
  pub is_admin: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
  #[sea_orm(has_many = "super::license::Entity")]
  Licenses,
  #[sea_orm(has_one = "super::user_stats::Entity")]
  UserStats,
  #[sea_orm(has_many = "super::claimed_promo::Entity")]
  ClaimedPromos,
}

impl Related<super::license::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::Licenses.def()
  }
}

impl Related<super::user_stats::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::UserStats.def()
  }
}

impl Related<super::claimed_promo::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::ClaimedPromos.def()
  }
}

impl ActiveModelBehavior for ActiveModel {}
