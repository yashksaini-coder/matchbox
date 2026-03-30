pub mod orders;
pub mod ws;

use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

pub fn app_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/orders", post(orders::post_order))
        .route("/orderbook", get(orders::get_orderbook))
        .route("/ws", get(ws::ws_handler))
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}
