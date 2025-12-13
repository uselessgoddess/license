//! Error types for the license server

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use derive_more::{Display, From};

#[derive(Debug, Display, From)]
pub enum AppError {
  #[display("Database error: {_0}")]
  Database(sea_orm::DbErr),

  #[display("License not found")]
  LicenseNotFound,

  #[display("User not found")]
  UserNotFound,

  #[display("License expired or blocked")]
  LicenseInvalid,

  #[display("Session limit reached")]
  SessionLimitReached,

  #[display("Promo not active")]
  PromoNotActive,

  #[display("Promo already claimed")]
  PromoAlreadyClaimed,

  #[display("Build not found")]
  BuildNotFound,

  #[display("IO error: {_0}")]
  Io(std::io::Error),

  #[display("Internal error: {_0}")]
  Internal(String),
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
  fn into_response(self) -> Response {
    let (status, message) = match &self {
      AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
      AppError::LicenseNotFound => (StatusCode::NOT_FOUND, "License not found"),
      AppError::UserNotFound => (StatusCode::NOT_FOUND, "User not found"),
      AppError::LicenseInvalid => (StatusCode::FORBIDDEN, "License expired or blocked"),
      AppError::SessionLimitReached => (StatusCode::CONFLICT, "Session limit reached"),
      AppError::PromoNotActive => (StatusCode::BAD_REQUEST, "Promo is not active"),
      AppError::PromoAlreadyClaimed => (StatusCode::CONFLICT, "Promo already claimed"),
      AppError::BuildNotFound => (StatusCode::NOT_FOUND, "Build not found"),
      AppError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO error"),
      AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal error"),
    };

    let body = json::json!({
      "success": false,
      "error": message
    });

    (status, axum::Json(body)).into_response()
  }
}

pub type AppResult<T> = Result<T, AppError>;
