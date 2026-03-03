use crate::{
  entity::{
    TransactionType, transaction,
    user::{self, UserRole},
  },
  prelude::*,
};

pub struct Referral<'a> {
  db: &'a DatabaseConnection,
}

/// 1 USDT = 1,000,000 nanoUSDT (USDT uses 6 decimal places)
pub const NANO_USDT: i64 = 1_000_000;
#[allow(dead_code)]
pub const MONTH_PRICE: i64 = 10 * NANO_USDT;
#[allow(dead_code)]
pub const QUARTER_PRICE: i64 = 25 * NANO_USDT;

#[allow(dead_code)]
impl<'a> Referral<'a> {
  pub fn new(db: &'a DatabaseConnection) -> Self {
    Self { db }
  }

  /// Validate a referrer by their user ID
  /// Returns the referrer if they exist (any user can be a referrer)
  /// All users earn commission, but only creators/admins can withdraw
  pub async fn validate_referrer(
    &self,
    referrer_id: i64,
  ) -> Result<user::Model> {
    let referrer = user::Entity::find_by_id(referrer_id)
      .one(self.db)
      .await?
      .ok_or(Error::ReferralNotFound)?;

    Ok(referrer)
  }

  /// Find a referrer by custom referral code (for creators only)
  /// Returns the creator if the code is valid
  pub async fn find_by_code(&self, code: &str) -> Result<user::Model> {
    let referrer = user::Entity::find()
      .filter(user::Column::ReferralCode.eq(code))
      .one(self.db)
      .await?
      .ok_or(Error::ReferralNotFound)?;

    // Only creators/admins can use custom referral codes
    if referrer.role != UserRole::Creator && referrer.role != UserRole::Admin {
      return Err(Error::ReferralNotFound);
    }

    Ok(referrer)
  }

  /// Resolve a referral code to a user ID
  /// Supports both custom codes (for creators) and user IDs (for regular users)
  pub async fn resolve_code(&self, code: &str) -> Result<i64> {
    // First try to parse as user ID
    if let Ok(user_id) = code.parse::<i64>() {
      // Validate that the user exists
      let _ = self.validate_referrer(user_id).await?;
      return Ok(user_id);
    }

    // Try to find by custom referral code
    let referrer = self.find_by_code(code).await?;
    Ok(referrer.tg_user_id)
  }

  pub async fn was_commission_paid(
    &self,
    referrer_id: i64,
    buyer_id: i64,
  ) -> Result<bool> {
    let count = transaction::Entity::find()
      .filter(transaction::Column::UserId.eq(referrer_id)) // Кто получил бонус
      .filter(transaction::Column::TxType.eq(TransactionType::ReferralBonus)) // Тип транзакции
      .filter(transaction::Column::ReferrerId.eq(buyer_id)) // За кого (источник)
      .count(self.db)
      .await?;

    Ok(count > 0)
  }

  pub async fn record_sale(&self, referrer_id: i64, price: i64) -> Result<i64> {
    let txn = self.db.begin().await?;

    let referrer = user::Entity::find_by_id(referrer_id)
      .one(&txn)
      .await?
      .ok_or(Error::ReferralNotFound)?;

    let commission = (price * referrer.commission_rate as i64) / 100;

    user::ActiveModel {
      referral_sales: Set(referrer.referral_sales + 1),
      referral_earnings: Set(referrer.referral_earnings + commission),
      ..referrer.into()
    }
    .update(&txn)
    .await?;

    txn.commit().await?;
    Ok(commission)
  }

  /// Get referral stats for a user
  pub async fn stats(&self, user_id: i64) -> Result<ReferralStats> {
    let user = user::Entity::find_by_id(user_id)
      .one(self.db)
      .await?
      .ok_or(Error::UserNotFound)?;

    Ok(ReferralStats {
      commission_rate: user.commission_rate,
      discount_percent: user.discount_percent,
      total_sales: user.referral_sales,
      total_earnings: user.referral_earnings,
      can_withdraw: user.role == UserRole::Creator
        || user.role == UserRole::Admin,
    })
  }

  pub async fn discount_percent(&self, ref_id: impl Into<Option<i64>>) -> i32 {
    if let Some(ref_id) = ref_id.into()
      && let Ok(stats) = self.stats(ref_id).await
      && stats.can_withdraw
    {
      stats.discount_percent
    } else {
      0
    }
  }

  /// Update commission rate for a user (admin only)
  pub async fn set_commission_rate(
    &self,
    user_id: i64,
    rate: i32,
  ) -> Result<()> {
    let user = user::Entity::find_by_id(user_id)
      .one(self.db)
      .await?
      .ok_or(Error::UserNotFound)?;

    user::ActiveModel { commission_rate: Set(rate), ..user.into() }
      .update(self.db)
      .await?;

    Ok(())
  }

  /// Update discount percent for a user (admin only)
  pub async fn set_discount_percent(
    &self,
    user_id: i64,
    discount: i32,
  ) -> Result<()> {
    let user = user::Entity::find_by_id(user_id)
      .one(self.db)
      .await?
      .ok_or(Error::UserNotFound)?;

    user::ActiveModel { discount_percent: Set(discount), ..user.into() }
      .update(self.db)
      .await?;

    Ok(())
  }

  /// Get all creators (users who can be referrers)
  pub async fn all_creators(&self) -> Result<Vec<user::Model>> {
    Ok(
      user::Entity::find()
        .filter(
          user::Column::Role
            .eq(UserRole::Creator)
            .or(user::Column::Role.eq(UserRole::Admin)),
        )
        .all(self.db)
        .await?,
    )
  }

  pub async fn display_code(&self, referrer_id: i64) -> Option<String> {
    let referrer = user::Entity::find_by_id(referrer_id)
      .one(self.db)
      .await
      .ok()
      .flatten()?;

    let is_creator =
      referrer.role == UserRole::Creator || referrer.role == UserRole::Admin;

    Some(if is_creator {
      referrer.referral_code.unwrap_or_else(|| "[creator referral]".to_string())
    } else {
      referrer_id.to_string()
    })
  }
}

#[derive(Debug)]
pub struct ReferralStats {
  pub commission_rate: i32,
  pub discount_percent: i32,
  pub total_sales: i32,
  pub total_earnings: i64,
  pub can_withdraw: bool,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{entity::*, sv::test_utils::test_db};

  #[tokio::test]
  async fn test_validate_referrer_creator() {
    let db = test_db::setup().await;

    let now = Utc::now().naive_utc();
    user::ActiveModel {
      tg_user_id: Set(12345),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::Creator),
      referred_by: Set(None),
      commission_rate: Set(25),
      discount_percent: Set(3),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let referrer = Referral::new(&db).validate_referrer(12345).await.unwrap();
    assert_eq!(referrer.tg_user_id, 12345);
    assert_eq!(referrer.commission_rate, 25);
  }

  #[tokio::test]
  async fn test_regular_user_earns_commission() {
    let db = test_db::setup().await;

    let now = Utc::now().naive_utc();
    user::ActiveModel {
      tg_user_id: Set(12345),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::User),
      referred_by: Set(None),
      commission_rate: Set(25),
      discount_percent: Set(3),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let result = Referral::new(&db).validate_referrer(12345).await;
    assert!(result.is_ok());

    let commission =
      Referral::new(&db).record_sale(12345, MONTH_PRICE).await.unwrap();
    assert_eq!(commission, 2_500_000);

    let user =
      user::Entity::find_by_id(12345i64).one(&db).await.unwrap().unwrap();
    assert_eq!(user.referral_sales, 1);
    assert_eq!(user.referral_earnings, 2_500_000);
    assert_eq!(user.balance, 0);
  }

  #[tokio::test]
  async fn test_record_sale() {
    let db = test_db::setup().await;

    let now = Utc::now().naive_utc();
    user::ActiveModel {
      tg_user_id: Set(12345),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::Creator),
      referred_by: Set(None),
      commission_rate: Set(25),
      discount_percent: Set(3),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let commission =
      Referral::new(&db).record_sale(12345, MONTH_PRICE).await.unwrap();

    // 25% of 10 USDT = 2.5 USDT
    assert_eq!(commission, 2_500_000);

    let user =
      user::Entity::find_by_id(12345i64).one(&db).await.unwrap().unwrap();
    assert_eq!(user.referral_sales, 1);
    assert_eq!(user.referral_earnings, 2_500_000);
    assert_eq!(user.balance, 0); // does not update balance 
  }

  #[tokio::test]
  async fn test_custom_referral_code() {
    let db = test_db::setup().await;

    let now = Utc::now().naive_utc();
    user::ActiveModel {
      tg_user_id: Set(12345),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::Creator),
      referred_by: Set(None),
      commission_rate: Set(25),
      discount_percent: Set(3),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(Some("CREATOR123".to_string())),
    }
    .insert(&db)
    .await
    .unwrap();

    // Test find by custom code
    let referrer = Referral::new(&db).find_by_code("CREATOR123").await.unwrap();
    assert_eq!(referrer.tg_user_id, 12345);

    // Test resolve_code with custom code
    let user_id = Referral::new(&db).resolve_code("CREATOR123").await.unwrap();
    assert_eq!(user_id, 12345);

    // Test resolve_code with user ID
    let user_id = Referral::new(&db).resolve_code("12345").await.unwrap();
    assert_eq!(user_id, 12345);
  }

  #[tokio::test]
  async fn test_custom_code_only_for_creators() {
    let db = test_db::setup().await;

    let now = Utc::now().naive_utc();
    // Create a regular user with a referral code (should be ignored)
    user::ActiveModel {
      tg_user_id: Set(12345),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::User),
      referred_by: Set(None),
      commission_rate: Set(25),
      discount_percent: Set(3),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(Some("USER123".to_string())),
    }
    .insert(&db)
    .await
    .unwrap();

    // Regular user's custom code should not be found
    let result = Referral::new(&db).find_by_code("USER123").await;
    assert!(result.is_err());

    // But resolve_code should still work with user ID
    let user_id = Referral::new(&db).resolve_code("12345").await.unwrap();
    assert_eq!(user_id, 12345);
  }

  #[tokio::test]
  async fn test_display_code() {
    let db = test_db::setup().await;
    let now = Utc::now().naive_utc();

    // Create a creator with custom code
    user::ActiveModel {
      tg_user_id: Set(11111),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::Creator),
      referred_by: Set(None),
      commission_rate: Set(25),
      discount_percent: Set(3),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(Some("CREATOR_CODE".to_string())),
    }
    .insert(&db)
    .await
    .unwrap();

    // Create a creator without custom code
    user::ActiveModel {
      tg_user_id: Set(22222),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::Creator),
      referred_by: Set(None),
      commission_rate: Set(25),
      discount_percent: Set(3),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // Create a regular user (friend)
    user::ActiveModel {
      tg_user_id: Set(33333),
      reg_date: Set(now),
      balance: Set(0),
      role: Set(UserRole::User),
      referred_by: Set(None),
      commission_rate: Set(10),
      discount_percent: Set(0),
      referral_sales: Set(0),
      referral_earnings: Set(0),
      referral_code: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let referral = Referral::new(&db);

    // Creator with custom code should show the custom code
    let display = referral.display_code(11111).await.unwrap();
    assert_eq!(display, "CREATOR_CODE");

    // Creator without custom code should show "creator referral" to hide their ID
    let display = referral.display_code(22222).await.unwrap();
    assert_eq!(display, "[creator referral]");

    // Regular user (friend) should show their user ID
    let display = referral.display_code(33333).await.unwrap();
    assert_eq!(display, "33333");

    // Non-existent user should return None
    let display = referral.display_code(99999).await;
    assert!(display.is_none());
  }
}
