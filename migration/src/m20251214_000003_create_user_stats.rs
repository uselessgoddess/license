use sea_orm_migration::prelude::*;

use super::m20251214_000001_create_users::Users;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(UserStats::Table)
          .if_not_exists()
          .col(
            ColumnDef::new(UserStats::TgUserId)
              .big_integer()
              .not_null()
              .primary_key(),
          )
          .col(
            ColumnDef::new(UserStats::WeeklyXp)
              .big_integer()
              .not_null()
              .default(0),
          )
          .col(
            ColumnDef::new(UserStats::TotalXp)
              .big_integer()
              .not_null()
              .default(0),
          )
          .col(
            ColumnDef::new(UserStats::DropsCount)
              .integer()
              .not_null()
              .default(0),
          )
          .col(
            ColumnDef::new(UserStats::Instances)
              .integer()
              .not_null()
              .default(0),
          )
          .col(
            ColumnDef::new(UserStats::RuntimeHours)
              .double()
              .not_null()
              .default(0.0),
          )
          .col(ColumnDef::new(UserStats::LastUpdated).date_time().not_null())
          .foreign_key(
            ForeignKey::create()
              .name("fk_user_stats_user")
              .from(UserStats::Table, UserStats::TgUserId)
              .to(Users::Table, Users::TgUserId)
              .on_delete(ForeignKeyAction::Cascade),
          )
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.drop_table(Table::drop().table(UserStats::Table).to_owned()).await
  }
}

#[derive(DeriveIden)]
pub enum UserStats {
  Table,
  TgUserId,
  WeeklyXp,
  TotalXp,
  DropsCount,
  Instances,
  RuntimeHours,
  LastUpdated,
}
