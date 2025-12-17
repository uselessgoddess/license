use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(FreeGames::Table)
          .if_not_exists()
          .col(
            ColumnDef::new(FreeGames::PkgId)
              .integer()
              .not_null()
              .primary_key(),
          )
          .col(ColumnDef::new(FreeGames::AppId).integer().not_null())
          .col(ColumnDef::new(FreeGames::Name).string().not_null()) 
          .col(ColumnDef::new(FreeGames::UpdatedAt).date_time().not_null())
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.drop_table(Table::drop().table(FreeGames::Table).to_owned()).await
  }
}

#[derive(DeriveIden)]
pub enum FreeGames {
  Table,
  PkgId,
  AppId,
  Name,
  UpdatedAt,
}
