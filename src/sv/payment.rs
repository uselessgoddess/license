use crate::{
  entity::pending_invoice,
  prelude::*,
  sv::{
    balance::Balance,
    cryptobot::{CryptoBot, InvoiceStatus},
    referral::NANO_USDT,
  },
};

pub struct Payment<'a> {
  db: &'a DatabaseConnection,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct PaymentResult {
  pub invoice_id: i64,
  pub amount_nano: i64,
  pub user_id: i64,
  pub referrer_id: Option<i64>,
}

#[allow(dead_code)]
impl<'a> Payment<'a> {
  pub fn new(db: &'a DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn save_pending(
    &self,
    invoice_id: i64,
    user_id: i64,
    amount_usdt: f64,
    referrer_id: Option<i64>,
  ) -> Result<()> {
    let now = Utc::now().naive_utc();
    let expires_at = now + chrono::Duration::hours(1);

    let amount_nano = (amount_usdt * NANO_USDT as f64) as i64;

    pending_invoice::ActiveModel {
      invoice_id: Set(invoice_id),
      user_id: Set(user_id),
      amount_nano: Set(amount_nano),
      referrer_id: Set(referrer_id),
      created_at: Set(now),
      expires_at: Set(expires_at),
    }
    .insert(self.db)
    .await?;

    Ok(())
  }

  pub async fn pending_by_user(
    &self,
    user_id: i64,
  ) -> Result<Vec<pending_invoice::Model>> {
    let now = Utc::now().naive_utc();

    Ok(
      pending_invoice::Entity::find()
        .filter(pending_invoice::Column::UserId.eq(user_id))
        .filter(pending_invoice::Column::ExpiresAt.gt(now))
        .order_by_desc(pending_invoice::Column::CreatedAt)
        .all(self.db)
        .await?,
    )
  }

  pub async fn delete_pending(&self, invoice_id: i64) -> Result<()> {
    pending_invoice::Entity::delete_by_id(invoice_id).exec(self.db).await?;
    Ok(())
  }

  pub async fn cleanup_expired(&self) -> Result<u64> {
    let now = Utc::now().naive_utc();

    let result = pending_invoice::Entity::delete_many()
      .filter(pending_invoice::Column::ExpiresAt.lt(now))
      .exec(self.db)
      .await?;

    Ok(result.rows_affected)
  }

  pub async fn check_and_process(
    &self,
    cryptobot: &CryptoBot,
    user_id: i64,
  ) -> Result<Vec<PaymentResult>> {
    let pending = self.pending_by_user(user_id).await?;

    if pending.is_empty() {
      return Ok(vec![]);
    }

    let invoice_ids: Vec<i64> = pending.iter().map(|p| p.invoice_id).collect();

    let invoices =
      cryptobot.get_invoices(Some(invoice_ids), None).await.unwrap_or_default();

    let mut results = Vec::new();

    for pending_inv in pending {
      let invoice =
        invoices.iter().find(|i| i.invoice_id == pending_inv.invoice_id);

      if let Some(inv) = invoice {
        if inv.status == InvoiceStatus::Paid {
          let balance = Balance::new(self.db);
          balance
            .deposit(
              pending_inv.user_id,
              pending_inv.amount_nano,
              Some(format!("CryptoBot deposit #{}", pending_inv.invoice_id)),
            )
            .await?;

          self.delete_pending(pending_inv.invoice_id).await?;

          results.push(PaymentResult {
            invoice_id: pending_inv.invoice_id,
            amount_nano: pending_inv.amount_nano,
            user_id: pending_inv.user_id,
            referrer_id: pending_inv.referrer_id,
          });
        } else if inv.status == InvoiceStatus::Expired {
          self.delete_pending(pending_inv.invoice_id).await?;
        }
      }
    }

    Ok(results)
  }
}
