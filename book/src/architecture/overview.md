# System Overview

Matchbox uses a **Single Writer** architecture. All API servers push orders into a shared Redis queue. One elected engine worker consumes orders sequentially and publishes fills.

## Component Diagram

```
                          CLIENTS
 Browser/CLI  ←─ HTTP/WS ─→  API Server A  ←─ HTTP/WS ─→  ...
                                   │
                                   │ RPUSH order
                                   ▼
┌──────────────────────── REDIS ────────────────────────────┐
│  engine:order_queue (List)      fills:events (Pub/Sub)    │
│  engine:order_id_counter        orderbook:snapshot        │
│  engine:leader (SETNX)                                    │
└──────────┬───────────────────────────────┬────────────────┘
           │ LPOP                          │ SUBSCRIBE
           ▼                               ▼
    ENGINE WORKER                  ALL API INSTANCES
  (single leader task)          (fan-out to WS clients)
  • Owns OrderBook               • broadcast::Sender<Fill>
  • Sequential matching           • Per-connection subscribe
  • Publishes fills
```

## Order Lifecycle

An order travels through these stages:

```
Client POST /orders
    → Axum handler validates input
    → Redis INCR for unique order ID
    → Redis RPUSH to engine:order_queue
    → 201 Created returned to client

Engine Worker (async)
    → Redis LPOP from queue
    → Deserialize Order
    → match_order(order, &mut book)
    → Publish fills to Redis Pub/Sub
    → Update orderbook:snapshot in Redis

All API Instances
    → Redis SUBSCRIBE fills:events
    → tokio::sync::broadcast to local WS clients
    → Each WebSocket connection receives the fill
```

## Why Single Writer?

The core problem: if two servers match orders against the same book simultaneously, a resting order can be consumed twice (double-fill).

The single writer eliminates this by construction — one task, one book, sequential processing. Redis queue serializes all incoming orders. No locks, no CAS retry loops, no distributed consensus.

| Approach | Correctness | Complexity | Chosen? |
|----------|-------------|------------|---------|
| Single Writer (Redis Queue) | Correct by construction | Simple | **Yes** |
| Optimistic Locking (CAS) | Correct with retries | Moderate | No |
| Raft Consensus | Strongly consistent | Very high | No |
