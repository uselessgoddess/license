use sea_orm_migration::prelude::*;

use super::m20251214_000003_create_user_stats::UserStats;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .alter_table(
        Table::alter()
          .table(UserStats::Table)
          .add_column(ColumnDef::new(Alias::new("meta")).json().null())
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .alter_table(
        Table::alter()
          .table(UserStats::Table)
          .drop_column(Alias::new("meta"))
          .to_owned(),
      )
      .await
  }
}
