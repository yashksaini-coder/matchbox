use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use fred::prelude::*;

use engine::models::{CreateOrderRequest, CreateOrderResponse, Order, OrderBookSnapshot};

use crate::errors::AppError;
use crate::state::AppState;

pub async fn post_order(
    State(state): State<AppState>,
    Json(req): Json<CreateOrderRequest>,
) -> Result<(StatusCode, Json<CreateOrderResponse>), AppError> {
    // Validate input
    if req.qty == 0 {
        return Err(AppError::Validation("qty must be > 0".into()));
    }
    if req.price == 0 {
        return Err(AppError::Validation("price must be > 0".into()));
    }

    // Get a globally unique order ID from Redis INCR (atomic)
    let order_id: u64 = state.redis.incr("engine:order_id_counter").await?;

    // Build the order
    let order = Order {
        id: order_id,
        side: req.side,
        price: req.price,
        qty: req.qty,
        timestamp: now_nanos(),
    };

    // Serialize and push to the engine queue
    let json = serde_json::to_string(&order).map_err(|e| AppError::Internal(e.to_string()))?;
    state
        .redis
        .rpush::<(), _, _>("engine:order_queue", json)
        .await?;

    Ok((StatusCode::CREATED, Json(CreateOrderResponse { order_id })))
}

pub async fn get_orderbook(
    State(state): State<AppState>,
) -> Result<Json<OrderBookSnapshot>, AppError> {
    let raw: Option<String> = state.redis.get("orderbook:snapshot").await?;

    match raw {
        Some(json) => {
            let snapshot: OrderBookSnapshot =
                serde_json::from_str(&json).map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(Json(snapshot))
        }
        // Book empty / not yet initialized
        None => Ok(Json(OrderBookSnapshot {
            bids: vec![],
            asks: vec![],
            sequence: 0,
        })),
    }
}

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}
