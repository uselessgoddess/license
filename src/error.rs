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
  #[error("Build already yanked")]
  BuildInactive,
  #[error("Build already active")]
  BuildAlreadyActive,
  #[error("Invalid arguments: {0}")]
  InvalidArgs(String),
  #[error("DB error: {0}")]
  Database(#[from] sea_orm::DbErr),
  #[error("IO error: {0}")]
  Io(#[from] io::Error),
  #[error("Internal error: {0}")]
  Internal(String),
}

impl Error {
  /// User-friendly error message for telegram bot responses
  pub fn user_message(&self) -> String {
    match self {
      Error::LicenseNotFound => "Key not found".into(),
      Error::UserNotFound => "User not found".into(),
      Error::LicenseInvalid => "License expired or blocked".into(),
      Error::SessionLimitReached => "Session limit reached".into(),
      Error::Promo(Promo::Inactive) => "Promo is not active right now".into(),
      Error::Promo(Promo::Claimed) => {
        "You have already claimed this promo".into()
      }
      Error::BuildNotFound => "Build not found".into(),
      Error::BuildInactive => "Build is already yanked".into(),
      Error::BuildAlreadyActive => "Build is already active".into(),
      Error::InvalidArgs(msg) => msg.clone(),
      Error::Database(e) => format!("Database error: {}", e),
      Error::Io(e) => format!("IO error: {}", e),
      Error::Internal(msg) => format!("Internal error: {}", msg),
    }
  }
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
      Error::BuildInactive => (StatusCode::BAD_REQUEST, "Build already yanked"),
      Error::BuildAlreadyActive => {
        (StatusCode::BAD_REQUEST, "Build already active")
      }
      Error::InvalidArgs(msg) => (StatusCode::BAD_REQUEST, msg.as_str()),
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

pub type Result<T, E = Error> = std::result::Result<T, E>;
