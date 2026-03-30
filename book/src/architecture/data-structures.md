# Data Structures

## Order Book

The book uses `BTreeMap<u64, VecDeque<Order>>` — one map for bids, one for asks.

```rust
pub struct OrderBook {
    bids: BTreeMap<u64, VecDeque<Order>>,  // Best bid = last key
    asks: BTreeMap<u64, VecDeque<Order>>,  // Best ask = first key
    order_index: HashMap<u64, Side>,       // O(1) lookup by ID
    pub sequence: u64,                      // Snapshot version
}
```

### Why BTreeMap?

Prices must be sorted. BTreeMap gives sorted iteration for free:

```rust
// Lowest ask (best for buyers)
let best_ask = self.asks.keys().next().copied();

// Highest bid (best for sellers)
let best_bid = self.bids.keys().next_back().copied();
```

### Why VecDeque?

Time priority requires FIFO ordering at each price level:

```rust
// New order goes to the back
level.push_back(order);

// Oldest order dequeued first during matching
let oldest = level.pop_front();

// Peek + modify for partial fills
let maker = level.front_mut().unwrap();
maker.qty -= fill_qty;
```

Both `push_back` and `pop_front` are O(1) amortized.

## Complexity

| Operation | Cost | Notes |
|-----------|------|-------|
| Find best price | O(log P) | P = number of price levels (~100 max) |
| Insert order | O(log P) | BTreeMap insert + VecDeque push |
| Match one order | O(1) | VecDeque pop_front |
| Full match cycle | O(K log P + M) | K levels crossed, M fills generated |

## Core Types

```rust
pub struct Order {
    pub id: u64,         // Unique via Redis INCR
    pub side: Side,      // Buy or Sell
    pub price: u64,      // Integer ticks (no floats)
    pub qty: u64,        // Contracts remaining
    pub timestamp: u64,  // Nanoseconds for time priority
}

pub struct Fill {
    pub maker_order_id: u64,
    pub taker_order_id: u64,
    pub price: u64,       // Always maker's price
    pub qty: u64,
    pub timestamp: u64,
}

pub enum Side { Buy, Sell }
```

All types derive `Serialize` and `Deserialize` for JSON transport over HTTP, Redis, and WebSocket.
