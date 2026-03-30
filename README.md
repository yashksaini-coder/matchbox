# Matchbox

A toy (but architecturally honest) order matching engine for a prediction market, built in Rust. Designed to demonstrate correctness, clean architecture, and understanding of distributed systems — not feature completeness.

**Stack**: Rust · Tokio · Axum · Redis · WebSockets · serde

---

## Table of Contents

- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Getting Started](#getting-started)
  - [Option A: Manual Setup](#option-a-manual-setup)
  - [Option B: Docker Compose](#option-b-docker-compose-full-stack)
- [API Reference](#api-reference)
- [Usage Examples](#usage-examples)
  - [Submitting Orders](#submitting-orders)
  - [Querying the Order Book](#querying-the-order-book)
  - [Real-Time WebSocket Feed](#real-time-websocket-feed)
  - [Partial Fill Example](#partial-fill-example)
  - [Price Priority Example](#price-priority-example)
- [Multi-Instance Mode](#multi-instance-mode)
  - [Manual Multi-Instance](#manual-multi-instance)
  - [Docker Multi-Instance](#docker-multi-instance)
  - [Leader Failover Test](#leader-failover-test)
- [Running Tests](#running-tests)
- [Project Structure](#project-structure)
- [The 4 Critical Questions](#the-4-critical-questions)
- [What's NOT Built (by design)](#whats-not-built-by-design)

---

## Architecture

```
                              CLIENTS
   Browser/CLI  <── HTTP/WS ──>  API Server A  <── HTTP/WS ──>  ...
                                     │
                                     │ RPUSH order (JSON)
                                     v
  ┌─────────────────────────────── REDIS ───────────────────────────────┐
  │  engine:order_queue (List)          fills:events (Pub/Sub Channel)  │
  │  engine:order_id_counter (INCR)     orderbook:snapshot (String)     │
  │  engine:leader (SETNX, TTL=30s)                                    │
  └────────────┬──────────────────────────────────────┬────────────────┘
               │ LPOP                                 │ SUBSCRIBE
               v                                      v
        ENGINE WORKER                        ALL API SERVER INSTANCES
     (single leader task)                (subscribe, broadcast to local WS)
     - Owns OrderBook in memory          - tokio::sync::broadcast<Fill>
     - Sequential match_order()          - Each WS connection subscribes
     - Publishes fills to Pub/Sub        - Fan-out to connected clients
     - Updates snapshot in Redis
```

**Key insight**: All API servers push orders to a shared Redis list. A single engine worker — elected via Redis distributed lock (`SETNX`) — consumes orders sequentially from the queue. This eliminates double-matching without complex distributed consensus. Redis acts as both the coordination layer and the message bus.

### How the Flow Works

1. Client sends `POST /orders` to any API server instance
2. API server assigns a globally unique ID via `INCR engine:order_id_counter`
3. API server pushes the serialized order to `engine:order_queue` via `RPUSH`
4. API server immediately returns `201 {order_id}` to the client
5. The engine worker (leader) polls the queue via `LPOP`, deserializes the order
6. Engine runs `match_order()` against the in-memory `OrderBook`
7. Generated fills are published to `fills:events` via Redis Pub/Sub
8. Engine updates the `orderbook:snapshot` in Redis
9. All API server instances receive fills via their Redis subscription
10. Each instance fans out fills to its locally connected WebSocket clients

---

## Prerequisites

- **Rust** (stable, 2021 edition) — [install via rustup](https://rustup.rs/)
- **Redis 7+** — either locally installed or via Docker
- **Docker & Docker Compose** (optional) — for containerized Redis or full-stack setup
- **websocat** (optional) — for testing WebSocket feeds (`cargo install websocat`)
- **curl** — for testing HTTP endpoints

---

## Getting Started

### Option A: Manual Setup

Use this when you want to run the Rust server directly on your machine with a separate Redis instance.

**Step 1: Install and start Redis**

```bash
# macOS (Homebrew)
brew install redis
redis-server

# Arch Linux
sudo pacman -S redis
sudo systemctl start redis

# Ubuntu/Debian
sudo apt install redis-server
sudo systemctl start redis

# Or just use Docker for Redis only:
docker run -d --name redis -p 6379:6379 redis:7-alpine
```

**Step 2: Build the project**

```bash
git clone <repo-url> && cd prediction-market-engine
cargo build --workspace
```

**Step 3: Run tests**

```bash
cargo test --workspace
```

**Step 4: Start the server**

```bash
# Default: connects to redis://127.0.0.1:6379, listens on port 8080
RUST_LOG=info cargo run -p server
```

**Step 5: Verify it's running**

```bash
curl http://localhost:8080/health
# {"status":"ok"}
```

### Option B: Docker Compose (Full Stack)

Use this for zero-install setup. Docker Compose runs Redis for you.

**Step 1: Start Redis**

```bash
docker compose up -d
```

**Step 2: Build and run the server**

```bash
REDIS_URL=redis://localhost:6379 RUST_LOG=info cargo run -p server
```

**Step 3: Verify**

```bash
curl http://localhost:8080/health
# {"status":"ok"}
```

**Teardown:**

```bash
docker compose down
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis connection URL |
| `PORT` | `8080` | HTTP server listen port |
| `RUST_LOG` | _(none)_ | Log level filter (e.g., `info`, `debug`, `server=debug`) |

---

## API Reference

### POST /orders

Submit a new limit order to the matching engine.

**Request:**
```json
{
  "side": "buy" | "sell",
  "price": 50,
  "qty": 10
}
```

- `side` — `"buy"` or `"sell"`
- `price` — integer ticks (u64). e.g., 50 = $0.50 if tick = $0.01
- `qty` — number of contracts (u64, must be > 0)

**Response** (`201 Created`):
```json
{
  "order_id": 1
}
```

**Errors:**
- `400` — validation error (qty=0, price=0)
- `500` — Redis connection error

---

### GET /orderbook

Returns the current order book state (bids and asks aggregated by price level).

**Response** (`200 OK`):
```json
{
  "bids": [
    { "price": 50, "qty": 30 },
    { "price": 48, "qty": 10 }
  ],
  "asks": [
    { "price": 52, "qty": 15 },
    { "price": 55, "qty": 20 }
  ],
  "sequence": 42
}
```

- `bids` — sorted best (highest price) first
- `asks` — sorted best (lowest price) first
- `sequence` — monotonically increasing counter; increments on every order processed

---

### GET /ws

WebSocket endpoint. After connecting, the client receives fill events as JSON messages in real time.

**Fill message format:**
```json
{
  "maker_order_id": 1,
  "taker_order_id": 2,
  "price": 50,
  "qty": 10,
  "timestamp": 1711814400000000000
}
```

- `maker_order_id` — the resting order that was already in the book
- `taker_order_id` — the incoming order that triggered the match
- `price` — fill price (always the maker's price, per price-time priority)
- `qty` — number of contracts filled
- `timestamp` — Unix nanoseconds when the fill occurred

---

### GET /health

Liveness probe.

**Response** (`200 OK`):
```json
{
  "status": "ok"
}
```

---

## Usage Examples

### Submitting Orders

```bash
# Place a sell order: 10 contracts at price 50
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"sell","price":50,"qty":10}'
# {"order_id":1}

# Place a matching buy order: 10 contracts at price 50
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":50,"qty":10}'
# {"order_id":2}
```

### Querying the Order Book

```bash
# After the sell rests (before the buy arrives):
curl -s http://localhost:8080/orderbook
# {"bids":[],"asks":[{"price":50,"qty":10}],"sequence":1}

# After the buy matches the sell (book is empty):
curl -s http://localhost:8080/orderbook
# {"bids":[],"asks":[],"sequence":2}
```

### Real-Time WebSocket Feed

```bash
# Terminal 1: Connect to WebSocket feed
websocat ws://localhost:8080/ws

# Terminal 2: Submit crossing orders
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"sell","price":60,"qty":5}'

curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":60,"qty":5}'

# Terminal 1 receives:
# {"maker_order_id":1,"taker_order_id":2,"price":60,"qty":5,"timestamp":...}
```

### Partial Fill Example

```bash
# Sell 30 at price 100
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"sell","price":100,"qty":30}'

# Buy 50 at price 100 — only 30 available, remaining 20 rests as a bid
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":100,"qty":50}'

curl -s http://localhost:8080/orderbook
# {"bids":[{"price":100,"qty":20}],"asks":[],"sequence":2}
# Fill generated: qty=30 (partial fill of the buy order)
# Remaining 20 from the buy rests on the bids side
```

### Price Priority Example

```bash
# Three sells at different prices
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" -d '{"side":"sell","price":490,"qty":10}'
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" -d '{"side":"sell","price":500,"qty":10}'
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" -d '{"side":"sell","price":510,"qty":10}'

# Buy 25 at price 510
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" -d '{"side":"buy","price":510,"qty":25}'

# Three fills generated:
#   fill 1: price=490, qty=10 (lowest ask consumed first)
#   fill 2: price=500, qty=10 (next lowest)
#   fill 3: price=510, qty=5  (partially consumes the 510 ask)
#
# Note: buyer submitted at 510 but got filled at 490 and 500 too.
# The fill price is always the MAKER's price — not the taker's.

curl -s http://localhost:8080/orderbook
# {"bids":[],"asks":[{"price":510,"qty":5}],"sequence":4}
```

---

## Multi-Instance Mode

The system is designed to support N API server instances simultaneously, all sharing the same Redis backend. Only one instance becomes the engine leader; the others serve HTTP/WS and relay fills.

### Manual Multi-Instance

```bash
# Terminal 1: Start instance A (will likely become leader)
PORT=8080 RUST_LOG=info cargo run -p server

# Terminal 2: Start instance B (becomes a follower)
PORT=8081 RUST_LOG=info cargo run -p server

# Terminal 3: Connect WebSocket to instance A
websocat ws://localhost:8080/ws

# Terminal 4: Connect WebSocket to instance B
websocat ws://localhost:8081/ws

# Terminal 5: Submit orders to different instances
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"sell","price":50,"qty":10}'

curl -s -X POST http://localhost:8081/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":50,"qty":10}'

# Both WebSocket clients (Terminal 3 & 4) receive the fill event.
# Both instances return empty orderbook:
curl -s http://localhost:8080/orderbook  # {"bids":[],"asks":[],"sequence":2}
curl -s http://localhost:8081/orderbook  # {"bids":[],"asks":[],"sequence":2}
```

### Docker Multi-Instance

```bash
# Start Redis
docker compose up -d

# Run two server instances in separate terminals (or background them)
REDIS_URL=redis://localhost:6379 PORT=8080 RUST_LOG=info cargo run -p server &
REDIS_URL=redis://localhost:6379 PORT=8081 RUST_LOG=info cargo run -p server &

# Verify both are up
curl -s http://localhost:8080/health  # {"status":"ok"}
curl -s http://localhost:8081/health  # {"status":"ok"}

# Check which instance is the leader
docker exec $(docker compose ps -q redis) redis-cli GET engine:leader
# Returns the UUID of the leader instance

# Submit crossing orders across instances
curl -s -X POST http://localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"sell","price":75,"qty":10}'

curl -s -X POST http://localhost:8081/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":75,"qty":10}'

# Verify matching occurred — both show empty book
curl -s http://localhost:8080/orderbook
curl -s http://localhost:8081/orderbook
```

### Leader Failover Test

```bash
# 1. Start two instances as above
# 2. Note which is the leader (check logs for "Became engine leader")
# 3. Kill the leader instance (Ctrl+C or kill the process)
# 4. Wait ~30 seconds (leader lock TTL expires)
# 5. The surviving instance logs "Became engine leader"
# 6. Submit new orders — they are processed by the new leader

# During the failover window:
# - Orders are accepted by any API server (pushed to Redis queue)
# - Orders queue up but are NOT matched until a new leader is elected
# - No orders are lost — they wait in the Redis list
# - After failover, queued orders are processed in FIFO order
#
# Known limitation: the in-memory order book is lost when the leader dies.
# Any resting orders from before the crash are gone. In production,
# you'd reconstruct the book from a persistent fill log.
```

---

## Running Tests

```bash
# Run all workspace tests (engine + server)
cargo test --workspace

# Run only the engine (matching logic) tests
cargo test -p engine

# Run tests with output
cargo test --workspace -- --nocapture

# Run a specific test
cargo test -p engine test_price_priority
```

### Test Coverage

The engine crate includes 9 unit tests covering:

| Test | What It Verifies |
|------|-----------------|
| `test_no_match_rests_on_book` | Sell into empty book rests on asks |
| `test_full_match` | Exact crossing match, both orders consumed |
| `test_partial_fill_incoming_larger` | Incoming qty > resting qty, remainder rests |
| `test_partial_fill_resting_larger` | Incoming qty < resting qty, resting reduced |
| `test_price_priority` | Best price matched first across levels |
| `test_time_priority` | FIFO ordering at the same price level |
| `test_fills_sum_to_matched_qty` | Sum of fill quantities is correct |
| `test_no_match_price_too_low` | Buy below ask, both rest on book |
| `test_sell_matches_highest_bid_first` | Sell matches highest bid first |

### Code Quality

```bash
# Lint
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --all -- --check

# Format (auto-fix)
cargo fmt --all
```

---

## Project Structure

```
matchbox/
├── Cargo.toml                 # Workspace definition + shared dependencies
├── Cargo.lock
├── README.md
├── docker-compose.yml         # Redis service
├── .gitignore
│
├── crates/
│   ├── engine/                # Core matching engine library (pure Rust, no I/O)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs         # Public module re-exports
│   │       ├── models.rs      # Order, Fill, Side, request/response types
│   │       ├── book.rs        # OrderBook struct (BTreeMap + VecDeque)
│   │       └── matcher.rs     # match_order() + price-time priority + tests
│   │
│   └── server/                # API server binary (Axum + Redis + WebSocket)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs            # Entry point, startup sequence
│           ├── state.rs           # AppState (Redis client, broadcast, instance ID)
│           ├── errors.rs          # AppError enum with IntoResponse
│           ├── engine_worker.rs   # LPOP loop + matching + leader election
│           ├── redis_sub.rs       # Redis Pub/Sub subscriber -> broadcast
│           └── routes/
│               ├── mod.rs         # Router definition
│               ├── orders.rs      # POST /orders, GET /orderbook
│               └── ws.rs          # GET /ws WebSocket handler
```

### Module Responsibilities

| Module | Responsibility | Dependencies |
|--------|---------------|--------------|
| `engine::models` | Domain types: Order, Fill, Side, snapshots | serde only |
| `engine::book` | OrderBook: BTreeMap storage, add/query/snapshot | std::collections |
| `engine::matcher` | Price-time priority matching algorithm | engine::book, engine::models |
| `server::main` | Tokio runtime, Redis init, Axum serve, task spawning | everything |
| `server::state` | AppState shared across all handlers | fred, tokio::sync |
| `server::engine_worker` | Leader election + order queue consumer | engine, fred |
| `server::redis_sub` | Redis Pub/Sub -> tokio broadcast bridge | fred, tokio::sync |
| `server::routes::orders` | HTTP handlers for order submission and book query | state, engine::models, fred |
| `server::routes::ws` | WebSocket upgrade and fill streaming | state, tokio::sync |

### Redis Key Namespace

| Key | Type | Purpose | TTL |
|-----|------|---------|-----|
| `engine:order_id_counter` | String (int) | Monotonically increasing order ID via INCR | None |
| `engine:order_queue` | List | FIFO queue of serialized orders (RPUSH/LPOP) | None |
| `engine:leader` | String | Current engine leader instance ID (SET NX EX) | 30s |
| `orderbook:snapshot` | String (JSON) | Latest order book snapshot for GET /orderbook | None |
| `fills:events` | Pub/Sub Channel | Fill event broadcast from engine to all instances | N/A |

---

## The 4 Critical Questions

### 1. How does your system handle multiple API server instances without double-matching an order?

Every incoming order is serialized and pushed (`RPUSH`) into a Redis list named `engine:order_queue`. On startup, each API server instance attempts to acquire a Redis distributed lock (`SET engine:leader NX EX 30`) — only the instance that wins this atomic SET becomes the engine leader.

The leader runs a background task that polls `engine:order_queue` via `LPOP`, consuming orders one at a time. The leader maintains the complete order book in process memory and processes orders sequentially. No concurrent modification is possible. After processing, fills are published to a Redis Pub/Sub channel (`fills:events`), which all API server instances subscribe to and fan out to their locally connected WebSocket clients.

If the leader crashes, its lock expires after 30 seconds (TTL). The next API server instance that runs its election cycle acquires the lock and becomes the new leader. Until then, orders queue up in Redis and are processed once leadership is re-established.

**Why this is correct**: Redis `LPOP` is atomic. Only one consumer ever receives each queue element. The engine worker processes orders serially against a single in-memory `OrderBook` — no concurrent modifications are possible within the process (single task, no locks needed), and no concurrent modifications are possible across processes (single leader guarantee via SETNX).

### 2. What data structure did you use for the order book and why?

The order book uses two `BTreeMap<u64, VecDeque<Order>>` — one for bids, one for asks.

**BTreeMap** was chosen because it keeps price levels sorted by key automatically. For bids, I need the highest price first — achieved with `.keys().next_back()`. For asks, I need the lowest price first — achieved with `.keys().next()`. Insertions and deletions are O(log P) where P is the number of distinct price levels. In a prediction market, P is typically small (a binary market may have fewer than 100 tick levels).

**VecDeque** at each price level enforces time priority. New orders are `push_back` (newest at the back). When matching, `pop_front` retrieves the earliest order at that price. Both operations are O(1) amortized, making the time-priority discipline essentially free.

**Alternatives considered**:

| Data Structure | Pros | Cons | Verdict |
|---------------|------|------|---------|
| `BinaryHeap` | O(1) best-price peek | Requires full rebuild to remove non-peak elements; no time ordering within a price | Rejected |
| `HashMap` + sort | O(1) lookup by price | Loses sorted order; requires O(P log P) sort on every match | Rejected |
| `Vec<Order>` sorted | Simple | O(n) insertion and O(n) price-level lookup | Too slow |
| Skip list | Comparable to BTreeMap | Not in std; more complex for no practical gain | Rejected |
| `BTreeMap + VecDeque` | O(log P) sorted access, O(1) FIFO dequeue, standard library | Slightly more memory than a flat array | **Chosen** |

### 3. What breaks first if this were under real production load?

Three things, in order of severity:

1. **Single engine worker bottleneck**: Every order in the system must pass through one sequential polling loop. Under sustained load, the Redis queue will grow. Throughput is limited by how fast one Rust task can call `match_order()` — the pure in-memory matching is nanoseconds, but the Redis round-trips for LPOP + PUBLISH + SET add ~200-400us per order. Theoretical maximum: ~2,500 orders/second through the matching engine. For a prediction market with hundreds of traders, this is more than adequate. For a high-frequency trading venue, it's a bottleneck.

2. **Redis latency on order submission**: `POST /orders` blocks on two Redis round-trips (INCR + RPUSH). Under very high load, Redis connection pool exhaustion or network latency will cause request timeouts. Mitigation: pipeline INCR+RPUSH into a single round-trip, use UNIX domain socket for Redis (saves ~50us per call), or batch multiple orders.

3. **Order book loss on leader restart**: The in-memory book is not persisted. If the leader crashes, any resting orders that were on the book are gone. The snapshot in Redis reflects the last state, but it's read-only (used for GET /orderbook). In production, every fill would be written to PostgreSQL inside the engine loop, and the book would be rebuilt from the fill history on startup.

### 4. What would you build next if you had another 4 hours?

- **PostgreSQL persistence**: Write every fill to a `fills` table. On startup, the engine worker replays the fill log to reconstruct the book state. This makes the system crash-safe and enables fill history queries.
- **Order cancellation**: `DELETE /orders/:id` that publishes a cancel command to the queue. The engine worker would use the `order_index: HashMap<u64, Side>` (already in the OrderBook struct) to look up and remove the order by ID.
- **Integration test suite**: Spawn an in-process Axum server + embedded Redis (via testcontainers), submit a series of orders, and assert the exact fills generated and the final book state. This would catch regressions across the full stack.
- **Graceful shutdown**: On SIGTERM, finish processing any orders already popped from the queue before exiting, preventing lost fills.
- **Metrics**: A `/metrics` endpoint (Prometheus format) reporting queue depth, fills per second, and average match latency. Essential for production observability.

---

## What's NOT Built (by design)

The spec explicitly warns against over-engineering. These are intentionally omitted:

- Authentication / authorization
- Order persistence to PostgreSQL (order book can be rebuilt from Redis queue)
- Complex order types (market orders, stop-loss, IOC, FOK)
- Historical fill data or analytics
- Rate limiting, circuit breakers, monitoring
- Docker / Kubernetes deployment configs
- Extensive error message polish

> *3-4 hours means ~400-600 lines of focused, clean Rust. Not a 3000-line codebase. Every line should earn its place.*
