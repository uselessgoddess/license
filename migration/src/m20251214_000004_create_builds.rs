use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(Builds::Table)
          .if_not_exists()
          .col(
            ColumnDef::new(Builds::Id)
              .integer()
              .not_null()
              .auto_increment()
              .primary_key(),
          )
          .col(ColumnDef::new(Builds::Version).string().not_null().unique_key())
          .col(ColumnDef::new(Builds::FilePath).string().not_null())
          .col(ColumnDef::new(Builds::Changelog).text().null())
          .col(
            ColumnDef::new(Builds::IsActive).boolean().not_null().default(true),
          )
          .col(ColumnDef::new(Builds::CreatedAt).date_time().not_null())
          .col(
            ColumnDef::new(Builds::Downloads).integer().not_null().default(0),
          )
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.drop_table(Table::drop().table(Builds::Table).to_owned()).await
  }
}

#[derive(DeriveIden)]
pub enum Builds {
  Table,
  Id,
  Version,
  FilePath,
  Changelog,
  IsActive,
  CreatedAt,
  Downloads,
}
