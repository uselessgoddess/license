use std::sync::Arc;

use axum::{Json, extract::State};

use crate::{
  entity::{free_game, free_item},
  prelude::*,
  state::AppState,
};

pub async fn free_games(
  State(app): State<Arc<AppState>>,
) -> Result<Json<Vec<free_game::Model>>> {
  Ok(Json(app.sv().steam.free_games().await?))
}

pub async fn free_items(
  State(app): State<Arc<AppState>>,
) -> Result<Json<Vec<free_item::Model>>> {
  Ok(Json(app.sv().steam.free_items().await?))
}
