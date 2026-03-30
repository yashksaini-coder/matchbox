use serde::{Deserialize, Serialize};

/// Direction of an order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

/// A resting or incoming limit order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: u64,
    pub side: Side,
    /// Integer ticks — no floats. 100 = $1.00 if tick size is $0.01
    pub price: u64,
    /// Number of contracts remaining
    pub qty: u64,
    /// Unix nanoseconds — used for time priority tiebreaking
    pub timestamp: u64,
}

/// A matched fill — created when two orders cross
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// The resting order that was already in the book
    pub maker_order_id: u64,
    /// The incoming order that triggered the match
    pub taker_order_id: u64,
    /// Fill price = maker's price (price-time priority rule)
    pub price: u64,
    /// Number of contracts filled
    pub qty: u64,
    /// Unix nanoseconds when fill occurred
    pub timestamp: u64,
}

/// HTTP request body for POST /orders
#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub side: Side,
    pub price: u64,
    pub qty: u64,
}

/// HTTP response for POST /orders
#[derive(Debug, Serialize)]
pub struct CreateOrderResponse {
    pub order_id: u64,
}

/// Aggregated price level for order book snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: u64,
    /// Total qty across all orders at this price level
    pub qty: u64,
}

/// Snapshot of the order book for GET /orderbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    /// Bids sorted best (highest) first
    pub bids: Vec<PriceLevel>,
    /// Asks sorted best (lowest) first
    pub asks: Vec<PriceLevel>,
    /// Monotonically increasing; clients can detect missed updates
    pub sequence: u64,
}
