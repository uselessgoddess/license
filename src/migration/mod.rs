//! Database migrations using SeaORM

use sea_orm_migration::prelude::*;

mod m20251214_000001_create_users;
mod m20251214_000002_create_licenses;
mod m20251214_000003_create_user_stats;
mod m20251214_000004_create_builds;
mod m20251214_000005_create_claimed_promos;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
  fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![
      Box::new(m20251214_000001_create_users::Migration),
      Box::new(m20251214_000002_create_licenses::Migration),
      Box::new(m20251214_000003_create_user_stats::Migration),
      Box::new(m20251214_000004_create_builds::Migration),
      Box::new(m20251214_000005_create_claimed_promos::Migration),
    ]
  }
}
