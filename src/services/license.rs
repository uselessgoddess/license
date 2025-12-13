//! License service - handles license creation, validation, and management

use std::time::Duration;

use chrono::{NaiveDateTime, TimeZone, Utc};
use sea_orm::{
  ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
  TransactionTrait,
};
use uuid::Uuid;

use crate::entities::license::LicenseType;
use crate::entities::prelude::*;
use crate::error::{AppError, AppResult};
use crate::services::UserService;

pub struct LicenseService;

impl LicenseService {
  /// Create a new license for a user
  pub async fn create(
    db: &DatabaseConnection,
    tg_user_id: i64,
    days: u64,
    license_type: LicenseType,
  ) -> AppResult<LicenseModel> {
    // Ensure user exists
    UserService::get_or_create(db, tg_user_id, None).await?;

    let now = Utc::now().naive_utc();
    let expires_at = now + Duration::from_secs(24 * 60 * 60 * days);
    let key = Uuid::new_v4().to_string();

    let license = LicenseActiveModel {
      key: Set(key),
      tg_user_id: Set(tg_user_id),
      license_type: Set(license_type),
      expires_at: Set(expires_at),
      is_blocked: Set(false),
      created_at: Set(now),
      hwid_hash: Set(None),
      max_sessions: Set(5),
    };

    let license = license.insert(db).await?;
    Ok(license)
  }

  /// Get license by key
  pub async fn get_by_key(db: &DatabaseConnection, key: &str) -> AppResult<Option<LicenseModel>> {
    let license = License::find_by_id(key).one(db).await?;
    Ok(license)
  }

  /// Get all licenses for a user
  pub async fn get_by_user(
    db: &DatabaseConnection,
    tg_user_id: i64,
    include_blocked: bool,
  ) -> AppResult<Vec<LicenseModel>> {
    let mut query = License::find().filter(crate::entities::license::Column::TgUserId.eq(tg_user_id));

    if !include_blocked {
      query = query.filter(crate::entities::license::Column::IsBlocked.eq(false));
    }

    let licenses = query.all(db).await?;
    Ok(licenses)
  }

  /// Validate license (checks expiry and block status)
  pub async fn validate(db: &DatabaseConnection, key: &str) -> AppResult<LicenseModel> {
    let license = License::find_by_id(key)
      .one(db)
      .await?
      .ok_or(AppError::LicenseNotFound)?;

    let now = Utc::now().naive_utc();
    if license.is_blocked || license.expires_at < now {
      return Err(AppError::LicenseInvalid);
    }

    Ok(license)
  }

  /// Extend license by days
  pub async fn extend(
    db: &DatabaseConnection,
    key: &str,
    days: i64,
  ) -> AppResult<NaiveDateTime> {
    let txn = db.begin().await?;

    let license = License::find_by_id(key)
      .one(&txn)
      .await?
      .ok_or(AppError::LicenseNotFound)?;

    let now = Utc::now().naive_utc();
    let base_time = if license.expires_at < now { now } else { license.expires_at };
    let new_exp = base_time + Duration::from_secs(24 * 60 * 60 * days as u64);

    let mut license: LicenseActiveModel = license.into();
    license.expires_at = Set(new_exp);
    license.is_blocked = Set(false);
    license.update(&txn).await?;

    txn.commit().await?;
    Ok(new_exp)
  }

  /// Set license blocked status
  pub async fn set_blocked(db: &DatabaseConnection, key: &str, blocked: bool) -> AppResult<()> {
    let license = License::find_by_id(key)
      .one(db)
      .await?
      .ok_or(AppError::LicenseNotFound)?;

    let mut license: LicenseActiveModel = license.into();
    license.is_blocked = Set(blocked);
    license.update(db).await?;
    Ok(())
  }

  /// Bind HWID to license
  pub async fn bind_hwid(db: &DatabaseConnection, key: &str, hwid_hash: &str) -> AppResult<()> {
    let license = License::find_by_id(key)
      .one(db)
      .await?
      .ok_or(AppError::LicenseNotFound)?;

    let mut license: LicenseActiveModel = license.into();
    license.hwid_hash = Set(Some(hwid_hash.to_string()));
    license.update(db).await?;
    Ok(())
  }

  /// Check if promo is active
  pub fn is_promo_active() -> bool {
    let now = Utc::now();
    // TODO: configurable promo periods
    let start = Utc.with_ymd_and_hms(2025, 12, 14, 18, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 12, 21, 23, 59, 59).unwrap();
    now >= start && now <= end
  }

  /// Claim a promo (free trial)
  pub async fn claim_promo(
    db: &DatabaseConnection,
    tg_user_id: i64,
    promo_name: &str,
  ) -> AppResult<LicenseModel> {
    if !Self::is_promo_active() {
      return Err(AppError::PromoNotActive);
    }

    // Ensure user exists
    UserService::get_or_create(db, tg_user_id, None).await?;

    // Check if already claimed
    let existing = ClaimedPromo::find_by_id((tg_user_id, promo_name.to_string()))
      .one(db)
      .await?;

    if existing.is_some() {
      return Err(AppError::PromoAlreadyClaimed);
    }

    // Create trial license (7 days)
    let license = Self::create(db, tg_user_id, 7, LicenseType::Trial).await?;

    // Record the claim
    let now = Utc::now().naive_utc();
    let claim = ClaimedPromoActiveModel {
      tg_user_id: Set(tg_user_id),
      promo_name: Set(promo_name.to_string()),
      claimed_at: Set(now),
    };
    claim.insert(db).await?;

    Ok(license)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sea_orm::{ConnectionTrait, Database, DbBackend, Schema};

  async fn setup_test_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();

    let schema = Schema::new(DbBackend::Sqlite);

    // Create tables
    let stmt = schema.create_table_from_entity(crate::entities::user::Entity);
    db.execute(db.get_database_backend().build(&stmt)).await.unwrap();

    let stmt = schema.create_table_from_entity(crate::entities::license::Entity);
    db.execute(db.get_database_backend().build(&stmt)).await.unwrap();

    let stmt = schema.create_table_from_entity(crate::entities::claimed_promo::Entity);
    db.execute(db.get_database_backend().build(&stmt)).await.unwrap();

    db
  }

  #[tokio::test]
  async fn test_create_license() {
    let db = setup_test_db().await;

    let license = LicenseService::create(&db, 12345, 30, LicenseType::Pro)
      .await
      .unwrap();

    assert_eq!(license.tg_user_id, 12345);
    assert_eq!(license.license_type, LicenseType::Pro);
    assert!(!license.is_blocked);
  }

  #[tokio::test]
  async fn test_validate_license() {
    let db = setup_test_db().await;

    let license = LicenseService::create(&db, 12345, 30, LicenseType::Trial)
      .await
      .unwrap();

    let validated = LicenseService::validate(&db, &license.key).await.unwrap();
    assert_eq!(validated.key, license.key);
  }

  #[tokio::test]
  async fn test_block_license() {
    let db = setup_test_db().await;

    let license = LicenseService::create(&db, 12345, 30, LicenseType::Trial)
      .await
      .unwrap();

    LicenseService::set_blocked(&db, &license.key, true)
      .await
      .unwrap();

    let result = LicenseService::validate(&db, &license.key).await;
    assert!(matches!(result, Err(AppError::LicenseInvalid)));
  }

  #[tokio::test]
  async fn test_extend_license() {
    let db = setup_test_db().await;

    let license = LicenseService::create(&db, 12345, 1, LicenseType::Trial)
      .await
      .unwrap();

    let old_exp = license.expires_at;
    let new_exp = LicenseService::extend(&db, &license.key, 30).await.unwrap();

    assert!(new_exp > old_exp);
  }
}
