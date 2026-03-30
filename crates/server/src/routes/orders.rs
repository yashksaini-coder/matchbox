use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
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
    if req.qty == 0 {
        return Err(AppError::Validation("qty must be > 0".into()));
    }
    if req.price == 0 {
        return Err(AppError::Validation("price must be > 0".into()));
    }

    let order_id: u64 = state.redis.incr("engine:order_id_counter").await?;

    let order = Order {
        id: order_id,
        side: req.side,
        price: req.price,
        qty: req.qty,
        timestamp: now_nanos(),
    };

    let json = serde_json::to_string(&order).map_err(|e| AppError::Internal(e.to_string()))?;

    // Store order for lookup via GET /orders and GET /orders/:id
    state
        .redis
        .set::<(), _, _>(&format!("order:{order_id}"), &json, None, None, false)
        .await?;

    // Track order ID in a sorted set (score = order_id for ordering)
    state
        .redis
        .zadd::<(), _, _>(
            "orders:index",
            None,
            None,
            false,
            false,
            (order_id as f64, order_id.to_string()),
        )
        .await?;

    // Push to engine queue for matching
    state
        .redis
        .rpush::<(), _, _>("engine:order_queue", json)
        .await?;

    Ok((StatusCode::CREATED, Json(CreateOrderResponse { order_id })))
}

pub async fn list_orders(State(state): State<AppState>) -> Result<Json<Vec<Order>>, AppError> {
    // Get all order IDs from the sorted set
    let ids: Vec<String> = state
        .redis
        .zrange("orders:index", 0, -1, None, false, None, false)
        .await?;

    let mut orders = Vec::with_capacity(ids.len());
    for id in &ids {
        let raw: Option<String> = state.redis.get(&format!("order:{id}")).await?;
        if let Some(json) = raw {
            if let Ok(order) = serde_json::from_str::<Order>(&json) {
                orders.push(order);
            }
        }
    }

    Ok(Json(orders))
}

pub async fn get_order(
    State(state): State<AppState>,
    Path(order_id): Path<u64>,
) -> Result<Json<Order>, AppError> {
    let raw: Option<String> = state.redis.get(&format!("order:{order_id}")).await?;

    match raw {
        Some(json) => {
            let order: Order =
                serde_json::from_str(&json).map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(Json(order))
        }
        None => Err(AppError::Validation(format!("order {order_id} not found"))),
    }
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
