use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(FreeItems::Table)
          .if_not_exists()
          .col(
            ColumnDef::new(FreeItems::DefId).integer().not_null().primary_key(), // ID из стима теперь PK
          )
          .col(ColumnDef::new(FreeItems::AppId).integer().not_null())
          .col(ColumnDef::new(FreeItems::Name).string().not_null())
          .col(ColumnDef::new(FreeItems::UpdatedAt).date_time().not_null())
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.drop_table(Table::drop().table(FreeItems::Table).to_owned()).await
  }
}

#[derive(DeriveIden)]
pub enum FreeItems {
  Table,
  DefId,
  AppId,
  Name,
  UpdatedAt,
}
