# Redis Integration

Redis serves as the coordination layer — it handles queuing, ID generation, leader election, state sharing, and event broadcasting.

## Key Namespace

| Key | Type | Purpose | Commands |
|-----|------|---------|----------|
| `engine:order_id_counter` | String | Unique order IDs | `INCR` |
| `engine:order_queue` | List | Order FIFO queue | `RPUSH` / `LPOP` |
| `engine:leader` | String | Leader lock (30s TTL) | `SET NX EX` |
| `orderbook:snapshot` | String | Book state for queries | `SET` / `GET` |
| `fills:events` | Pub/Sub | Fill broadcast channel | `PUBLISH` / `SUBSCRIBE` |

## Connection Setup

The server uses the `fred` crate with connection pooling:

```rust
let config = Config::from_url(&redis_url)?;
let redis = Builder::from_config(config).build()?;
redis.init().await?;
```

Pub/Sub requires a **separate** dedicated client (Redis protocol constraint):

```rust
let subscriber = Builder::from_config(config)
    .build_subscriber_client()?;
subscriber.init().await?;
subscriber.subscribe("fills:events").await?;
```

## Why Not Store the Book in Redis?

Matching requires iterating price levels and mutating individual orders — doing this in Redis would need complex Lua scripts or multiple round-trips per match.

In-memory matching takes **~1-10 microseconds**. Redis round-trips take **~100-200 microseconds each**. The in-memory approach is simpler and orders of magnitude faster.

The trade-off: the book is lost if the engine crashes. Acceptable for a toy system.
