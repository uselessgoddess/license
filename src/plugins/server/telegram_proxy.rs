use std::sync::{Arc, LazyLock};

use axum::{
  Json,
  body::Body,
  extract::{Multipart, State},
  http::{StatusCode, header},
  response::{IntoResponse, Response},
};
use json::{Map, Value, json};
use serde::Deserialize;

use crate::{prelude::*, state::AppState};

static TELEGRAM_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
  reqwest::Client::builder()
    .timeout(Duration::from_secs(90))
    .build()
    .expect("telegram proxy reqwest client")
});

#[derive(Debug, Deserialize)]
pub struct SendMessageReq {
  pub key: String,
  pub bot_token: String,
  pub chat_id: Value,
  pub text: String,
  #[serde(default)]
  pub parse_mode: Option<String>,
  #[serde(default)]
  pub reply_markup: Option<Value>,
  #[serde(default)]
  pub reply_to_message_id: Option<i32>,
  #[serde(default)]
  pub disable_web_page_preview: Option<bool>,
  #[serde(default)]
  pub disable_notification: Option<bool>,
  #[serde(default)]
  pub message_thread_id: Option<i32>,
}

async fn validate_license(app: &AppState, key: &str) -> Result<(), Response> {
  match app.sv().license.validate(key).await {
    Ok(_) => Ok(()),
    Err(Error::LicenseNotFound) => {
      Err(proxy_fail(StatusCode::UNAUTHORIZED, "Invalid license"))
    }
    Err(Error::LicenseInvalid) => {
      Err(proxy_fail(StatusCode::FORBIDDEN, "License expired or blocked"))
    }
    Err(_) => {
      Err(proxy_fail(StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
    }
  }
}

fn proxy_fail(status: StatusCode, description: impl Into<String>) -> Response {
  let body = json!({ "ok": false, "description": description.into() });
  (status, Json(body)).into_response()
}

fn empty_token_response() -> Response {
  proxy_fail(StatusCode::BAD_REQUEST, "bot_token is required")
}

pub async fn send_message(
  State(app): State<Arc<AppState>>,
  Json(req): Json<SendMessageReq>,
) -> impl IntoResponse {
  if req.bot_token.trim().is_empty() {
    return empty_token_response();
  }

  if let Err(resp) = validate_license(&app, &req.key).await {
    return resp;
  }

  let mut map = Map::new();
  map.insert("chat_id".into(), req.chat_id);
  map.insert("text".into(), Value::String(req.text));
  if let Some(v) = req.parse_mode {
    map.insert("parse_mode".into(), Value::String(v));
  }
  if let Some(v) = req.reply_markup {
    map.insert("reply_markup".into(), v);
  }
  if let Some(v) = req.reply_to_message_id {
    map.insert("reply_to_message_id".into(), Value::from(v));
  }
  if let Some(v) = req.disable_web_page_preview {
    map.insert("disable_web_page_preview".into(), Value::from(v));
  }
  if let Some(v) = req.disable_notification {
    map.insert("disable_notification".into(), Value::from(v));
  }
  if let Some(v) = req.message_thread_id {
    map.insert("message_thread_id".into(), Value::from(v));
  }

  let url =
    format!("https://api.telegram.org/bot{}/sendMessage", req.bot_token);
  let payload = Value::Object(map);

  match TELEGRAM_CLIENT.post(&url).json(&payload).send().await {
    Ok(res) => forward_telegram_response(res).await,
    Err(e) => {
      error!("telegram sendMessage request failed: {e}");
      proxy_fail(StatusCode::BAD_GATEWAY, "Telegram request failed")
    }
  }
}

async fn forward_telegram_response(res: reqwest::Response) -> Response {
  let status = StatusCode::from_u16(res.status().as_u16())
    .unwrap_or(StatusCode::BAD_GATEWAY);
  match res.text().await {
    Ok(text) => Response::builder()
      .status(status)
      .header(header::CONTENT_TYPE, "application/json")
      .body(Body::from(text))
      .unwrap_or_else(|_| {
        proxy_fail(StatusCode::INTERNAL_SERVER_ERROR, "Bad response")
      }),
    Err(e) => {
      error!("telegram response body read failed: {e}");
      proxy_fail(StatusCode::BAD_GATEWAY, "Telegram response failed")
    }
  }
}

pub async fn send_photo(
  State(app): State<Arc<AppState>>,
  mut multipart: Multipart,
) -> impl IntoResponse {
  let mut key = String::new();
  let mut bot_token = String::new();
  let mut chat_id = String::new();
  let mut caption: Option<String> = None;
  let mut parse_mode: Option<String> = None;
  let mut reply_to_message_id: Option<i32> = None;
  let mut reply_markup: Option<String> = None;
  let mut photo: Option<Vec<u8>> = None;
  let mut photo_filename = "photo.png".to_string();
  let mut photo_mime: Option<String> = None;

  loop {
    let field = match multipart.next_field().await {
      Ok(Some(f)) => f,
      Ok(None) => break,
      Err(e) => {
        error!("telegram send-photo multipart read failed: {e}");
        return proxy_fail(StatusCode::BAD_REQUEST, "Invalid multipart body");
      }
    };
    let name = field.name().unwrap_or("").to_string();
    match name.as_str() {
      "key" => {
        if let Ok(s) = field.text().await {
          key = s;
        }
      }
      "bot_token" => {
        if let Ok(s) = field.text().await {
          bot_token = s;
        }
      }
      "chat_id" => {
        if let Ok(s) = field.text().await {
          chat_id = s;
        }
      }
      "caption" => {
        if let Ok(s) = field.text().await {
          caption = Some(s);
        }
      }
      "parse_mode" => {
        if let Ok(s) = field.text().await {
          parse_mode = Some(s);
        }
      }
      "reply_to_message_id" => {
        if let Ok(s) = field.text().await {
          reply_to_message_id = s.parse().ok();
        }
      }
      "reply_markup" => {
        if let Ok(s) = field.text().await {
          reply_markup = Some(s);
        }
      }
      "photo" => {
        if let Some(name) = field.file_name()
          && !name.is_empty()
        {
          photo_filename = name.to_string();
        }
        photo_mime = field.content_type().map(|m| m.to_string());
        if let Ok(bytes) = field.bytes().await
          && !bytes.is_empty()
        {
          photo = Some(bytes.to_vec());
        }
      }
      _ => {}
    }
  }

  if bot_token.trim().is_empty() {
    return empty_token_response();
  }

  if chat_id.trim().is_empty() {
    return proxy_fail(StatusCode::BAD_REQUEST, "chat_id is required");
  }

  let Some(photo_bytes) = photo else {
    return proxy_fail(StatusCode::BAD_REQUEST, "photo file is required");
  };

  if let Err(resp) = validate_license(&app, &key).await {
    return resp;
  }

  let mime_str = photo_mime.as_deref().unwrap_or("application/octet-stream");
  let file_part = match reqwest::multipart::Part::bytes(photo_bytes)
    .file_name(photo_filename)
    .mime_str(mime_str)
  {
    Ok(p) => p,
    Err(e) => {
      error!("telegram sendPhoto multipart part build failed: {e}");
      return proxy_fail(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
    }
  };

  let mut form = reqwest::multipart::Form::new()
    .text("chat_id", chat_id)
    .part("photo", file_part);

  if let Some(c) = caption {
    form = form.text("caption", c);
  }
  if let Some(pm) = parse_mode {
    form = form.text("parse_mode", pm);
  }
  if let Some(id) = reply_to_message_id {
    form = form.text("reply_to_message_id", id.to_string());
  }
  if let Some(rm) = reply_markup {
    if let Err(e) = json::from_str::<Value>(&rm) {
      return proxy_fail(
        StatusCode::BAD_REQUEST,
        format!("reply_markup is not valid JSON: {e}"),
      );
    }
    form = form.text("reply_markup", rm);
  }

  let url = format!("https://api.telegram.org/bot{}/sendPhoto", bot_token);

  match TELEGRAM_CLIENT.post(&url).multipart(form).send().await {
    Ok(res) => forward_telegram_response(res).await,
    Err(e) => {
      error!("telegram sendPhoto request failed: {e}");
      proxy_fail(StatusCode::BAD_GATEWAY, "Telegram request failed")
    }
  }
}
