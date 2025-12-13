//! User service - handles user registration and management

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::entities::prelude::*;
use crate::error::{AppError, AppResult};

pub struct UserService;

impl UserService {
  /// Get or create a user by Telegram ID (auto-registration on /start)
  pub async fn get_or_create(
    db: &DatabaseConnection,
    tg_user_id: i64,
    username: Option<String>,
  ) -> AppResult<UserModel> {
    if let Some(user) = User::find_by_id(tg_user_id).one(db).await? {
      return Ok(user);
    }

    let now = Utc::now().naive_utc();
    let user = UserActiveModel {
      tg_user_id: Set(tg_user_id),
      username: Set(username),
      reg_date: Set(now),
      is_admin: Set(false),
    };

    let user = user.insert(db).await?;
    Ok(user)
  }

  /// Get user by Telegram ID
  pub async fn get_by_id(db: &DatabaseConnection, tg_user_id: i64) -> AppResult<Option<UserModel>> {
    let user = User::find_by_id(tg_user_id).one(db).await?;
    Ok(user)
  }

  /// Update username
  pub async fn update_username(
    db: &DatabaseConnection,
    tg_user_id: i64,
    username: Option<String>,
  ) -> AppResult<()> {
    let user = User::find_by_id(tg_user_id)
      .one(db)
      .await?
      .ok_or(AppError::UserNotFound)?;

    let mut user: UserActiveModel = user.into();
    user.username = Set(username);
    user.update(db).await?;
    Ok(())
  }

  /// Get all admins
  pub async fn get_admins(db: &DatabaseConnection) -> AppResult<Vec<UserModel>> {
    let admins = User::find()
      .filter(crate::entities::user::Column::IsAdmin.eq(true))
      .all(db)
      .await?;
    Ok(admins)
  }

  /// Set admin status
  pub async fn set_admin(
    db: &DatabaseConnection,
    tg_user_id: i64,
    is_admin: bool,
  ) -> AppResult<()> {
    let user = User::find_by_id(tg_user_id)
      .one(db)
      .await?
      .ok_or(AppError::UserNotFound)?;

    let mut user: UserActiveModel = user.into();
    user.is_admin = Set(is_admin);
    user.update(db).await?;
    Ok(())
  }
}
