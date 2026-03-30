use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::models::{Order, OrderBookSnapshot, PriceLevel, Side};

pub struct OrderBook {
    /// Key: price (ascending). Best bid = last entry.
    bids: BTreeMap<u64, VecDeque<Order>>,
    /// Key: price (ascending). Best ask = first entry.
    asks: BTreeMap<u64, VecDeque<Order>>,
    /// O(1) lookup: order_id -> side. Used for cancel, auditing.
    order_index: HashMap<u64, Side>,
    /// Monotonically increasing sequence number for snapshots.
    pub sequence: u64,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_index: HashMap::new(),
            sequence: 0,
        }
    }

    /// Add a resting order to the book (not yet matched).
    pub fn add_resting_order(&mut self, order: Order) {
        let side = order.side;
        let price = order.price;
        let id = order.id;

        match side {
            Side::Buy => self.bids.entry(price).or_default().push_back(order),
            Side::Sell => self.asks.entry(price).or_default().push_back(order),
        }

        self.order_index.insert(id, side);
    }

    /// Best (highest) bid price, if any.
    pub fn best_bid(&self) -> Option<u64> {
        self.bids.keys().next_back().copied()
    }

    /// Best (lowest) ask price, if any.
    pub fn best_ask(&self) -> Option<u64> {
        self.asks.keys().next().copied()
    }

    pub fn has_bids(&self) -> bool {
        !self.bids.is_empty()
    }

    pub fn has_asks(&self) -> bool {
        !self.asks.is_empty()
    }

    /// Produce a snapshot for GET /orderbook.
    pub fn snapshot(&self, depth: usize) -> OrderBookSnapshot {
        // Bids: descending price order (best first)
        let bids = self
            .bids
            .iter()
            .rev()
            .take(depth)
            .map(|(&price, queue)| PriceLevel {
                price,
                qty: queue.iter().map(|o| o.qty).sum(),
            })
            .collect();

        // Asks: ascending price order (best first)
        let asks = self
            .asks
            .iter()
            .take(depth)
            .map(|(&price, queue)| PriceLevel {
                price,
                qty: queue.iter().map(|o| o.qty).sum(),
            })
            .collect();

        OrderBookSnapshot {
            bids,
            asks,
            sequence: self.sequence,
        }
    }

    /// Mutable access to bids for the matcher.
    pub fn bids_mut(&mut self) -> &mut BTreeMap<u64, VecDeque<Order>> {
        &mut self.bids
    }

    /// Mutable access to asks for the matcher.
    pub fn asks_mut(&mut self) -> &mut BTreeMap<u64, VecDeque<Order>> {
        &mut self.asks
    }

    /// Remove an order from the index (called after full fill).
    pub fn remove_from_index(&mut self, order_id: u64) {
        self.order_index.remove(&order_id);
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}
