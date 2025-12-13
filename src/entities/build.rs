//! Build entity - stores software versions and distribution info

use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "builds")]
pub struct Model {
  #[sea_orm(primary_key)]
  pub id: i32,
  pub version: String,
  pub file_path: String,
  pub changelog: Option<String>,
  pub is_active: bool,
  pub created_at: NaiveDateTime,
  pub download_count: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
