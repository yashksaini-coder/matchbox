# API Reference

## POST /orders

Submit a new limit order.

```bash
curl -X POST localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":50,"qty":10}'
```

**Request body:**

```json
{
  "side": "buy",
  "price": 50,
  "qty": 10
}
```

**Response** (201 Created):

```json
{
  "order_id": 1
}
```

**Validation:**
- `qty` must be > 0 (400 if zero)
- `price` must be > 0 (400 if zero)
- `side` must be `"buy"` or `"sell"` (422 if invalid)

## GET /orderbook

Return the current order book state.

```bash
curl localhost:8080/orderbook
```

**Response** (200 OK):

```json
{
  "bids": [
    { "price": 50, "qty": 30 },
    { "price": 48, "qty": 10 }
  ],
  "asks": [
    { "price": 52, "qty": 15 }
  ],
  "sequence": 42
}
```

- `bids` — sorted highest price first
- `asks` — sorted lowest price first
- `sequence` — increments on every order processed

## GET /ws

WebSocket endpoint for real-time fill events.

```bash
websocat ws://localhost:8080/ws
```

Each fill arrives as a JSON text message:

```json
{
  "maker_order_id": 1,
  "taker_order_id": 2,
  "price": 50,
  "qty": 10,
  "timestamp": 1711814400000000000
}
```

## GET /health

Liveness probe.

```bash
curl localhost:8080/health
# {"status":"ok"}
```
