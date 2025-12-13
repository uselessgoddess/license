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
          .table(Licenses::Table)
          .if_not_exists()
          .col(ColumnDef::new(Licenses::Key).string().not_null().primary_key())
          .col(ColumnDef::new(Licenses::TgUserId).big_integer().not_null())
          .col(
            ColumnDef::new(Licenses::LicenseType)
              .string()
              .not_null()
              .default("trial"),
          )
          .col(ColumnDef::new(Licenses::ExpiresAt).date_time().not_null())
          .col(ColumnDef::new(Licenses::IsBlocked).boolean().not_null().default(false))
          .col(ColumnDef::new(Licenses::CreatedAt).date_time().not_null())
          .col(ColumnDef::new(Licenses::HwidHash).string().null())
          .col(ColumnDef::new(Licenses::MaxSessions).integer().not_null().default(5))
          .foreign_key(
            ForeignKey::create()
              .name("fk_licenses_user")
              .from(Licenses::Table, Licenses::TgUserId)
              .to(Users::Table, Users::TgUserId)
              .on_delete(ForeignKeyAction::Cascade),
          )
          .to_owned(),
      )
      .await?;

    manager
      .create_index(
        Index::create()
          .name("idx_licenses_user")
          .table(Licenses::Table)
          .col(Licenses::TgUserId)
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.drop_table(Table::drop().table(Licenses::Table).to_owned()).await
  }
}

#[derive(DeriveIden)]
pub enum Licenses {
  Table,
  Key,
  TgUserId,
  LicenseType,
  ExpiresAt,
  IsBlocked,
  CreatedAt,
  HwidHash,
  MaxSessions,
}
