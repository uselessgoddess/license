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
          .table(ClaimedPromos::Table)
          .if_not_exists()
          .col(ColumnDef::new(ClaimedPromos::TgUserId).big_integer().not_null())
          .col(ColumnDef::new(ClaimedPromos::PromoName).string().not_null())
          .col(ColumnDef::new(ClaimedPromos::ClaimedAt).date_time().not_null())
          .primary_key(
            Index::create()
              .col(ClaimedPromos::TgUserId)
              .col(ClaimedPromos::PromoName),
          )
          .foreign_key(
            ForeignKey::create()
              .name("fk_claimed_promos_user")
              .from(ClaimedPromos::Table, ClaimedPromos::TgUserId)
              .to(Users::Table, Users::TgUserId)
              .on_delete(ForeignKeyAction::Cascade),
          )
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .drop_table(Table::drop().table(ClaimedPromos::Table).to_owned())
      .await
  }
}

#[derive(DeriveIden)]
pub enum ClaimedPromos {
  Table,
  TgUserId,
  PromoName,
  ClaimedAt,
}
