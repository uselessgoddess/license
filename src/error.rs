use std::io;

use axum::{
  http::StatusCode,
  response::{IntoResponse, Response},
};

#[derive(Debug)]
pub enum Promo {
  Inactive,
  Claimed,
}

#[derive(thiserror::Error, Debug)]
#[allow(dead_code)]
pub enum Error {
  #[error("License not found")]
  LicenseNotFound,
  #[error("User not found")]
  UserNotFound,
  #[error("License expired or blocked")]
  LicenseInvalid,
  #[error("Session limit reached")]
  SessionLimitReached,
  #[error("Promo is {0:?}")]
  Promo(Promo),
  #[error("Build not found")]
  BuildNotFound,
  #[error("DB error: {0}")]
  Database(#[from] sea_orm::DbErr),
  #[error("IO error: {0}")]
  Io(#[from] io::Error),
  #[error("Internal error: {0}")]
  Internal(String),
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    let (status, message) = match &self {
      Error::Database(_) => {
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
      }
      Error::LicenseNotFound => (StatusCode::NOT_FOUND, "License not found"),
      Error::UserNotFound => (StatusCode::NOT_FOUND, "User not found"),
      Error::LicenseInvalid => {
        (StatusCode::FORBIDDEN, "License expired or blocked")
      }
      Error::SessionLimitReached => {
        (StatusCode::CONFLICT, "Session limit reached")
      }
      Error::Promo(Promo::Inactive) => {
        (StatusCode::BAD_REQUEST, "Promo is not active")
      }
      Error::Promo(Promo::Claimed) => {
        (StatusCode::CONFLICT, "Promo already claimed")
      }
      Error::BuildNotFound => (StatusCode::NOT_FOUND, "Build not found"),
      Error::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO error"),
      Error::Internal(_) => {
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
      }
    };

    let body = json::json!({
      "success": false,
      "error": message
    });

    (status, axum::Json(body)).into_response()
  }
}

pub type Result<T> = std::result::Result<T, Error>;
