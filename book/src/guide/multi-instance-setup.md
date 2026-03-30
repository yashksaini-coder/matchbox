# Multi-Instance Setup

Run multiple API servers sharing one Redis, with automatic leader election.

## Start Two Instances

```bash
# Terminal 1 — Instance A (will likely become leader)
PORT=8080 RUST_LOG=info cargo run -p server

# Terminal 2 — Instance B (follower, retries every 5s)
PORT=8081 RUST_LOG=info cargo run -p server
```

Check the logs — one instance prints `Became engine leader`, the other keeps retrying.

## Cross-Instance Matching

```bash
# Sell on Instance A
curl -X POST localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"sell","price":50,"qty":10}'

# Buy on Instance B
curl -X POST localhost:8081/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":50,"qty":10}'

# Both show empty book (orders matched through shared queue)
curl localhost:8080/orderbook
curl localhost:8081/orderbook
```

## WebSocket on Both

```bash
# Connect WS to each instance
websocat ws://localhost:8080/ws &
websocat ws://localhost:8081/ws &

# Submit crossing orders — BOTH connections receive the fill
```

## Leader Failover

```bash
# 1. Kill the leader instance (Ctrl+C)
# 2. Wait ~30 seconds (lock TTL expires)
# 3. The other instance logs "Became engine leader"
# 4. Submit orders — they are processed by the new leader
```

During failover, orders queue in Redis and are **not lost**. The new leader processes them on election.

## Verify Leader

```bash
# Check which instance holds the lock
docker exec $(docker compose ps -q redis) redis-cli GET engine:leader
```
