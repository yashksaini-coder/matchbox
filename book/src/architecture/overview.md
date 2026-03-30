# System Overview

Matchbox uses a **Single Writer** architecture. All API servers push orders into a shared Redis queue. One elected engine worker consumes orders sequentially and publishes fills.

## Component Diagram

```mermaid
graph TB
    subgraph Clients["Clients"]
        C1["Browser / CLI"]
        C2["WebSocket Client"]
    end

    subgraph API["API Server Instances"]
        A1["Server A · :8080"]
        A2["Server B · :8081"]
        A3["Server N · :808N"]
    end

    subgraph Redis["Redis — Coordination Layer"]
        Q["engine:order_queue<br/>List — FIFO Order Queue"]
        ID["engine:order_id_counter<br/>String — Atomic INCR"]
        LOCK["engine:leader<br/>String — SETNX · TTL 30s"]
        SNAP["orderbook:snapshot<br/>String — JSON Snapshot"]
        PS["fills:events<br/>Pub/Sub — Fill Broadcast"]
    end

    subgraph Engine["Engine Worker — Single Leader"]
        EW["engine_worker_loop<br/>· Owns OrderBook in memory<br/>· Sequential match_order"]
    end

    C1 -- "POST /orders" --> A1
    C1 -- "POST /orders" --> A2
    C1 -- "GET /orderbook" --> A3

    A1 -- "INCR" --> ID
    A1 -- "RPUSH order" --> Q
    A2 -- "RPUSH order" --> Q

    Q -- "LPOP" --> EW
    LOCK -. "SET NX EX 30" .-> EW
    EW -- "PUBLISH fill" --> PS
    EW -- "SET snapshot" --> SNAP

    PS -- "SUBSCRIBE" --> A1
    PS -- "SUBSCRIBE" --> A2
    PS -- "SUBSCRIBE" --> A3

    A1 -- "WS fill" --> C2
    A2 -- "WS fill" --> C2
    SNAP -- "GET" --> A3

    style Redis fill:#dc382c,color:#fff,stroke:#b71c1c
    style Engine fill:#1565c0,color:#fff,stroke:#0d47a1
    style API fill:#2e7d32,color:#fff,stroke:#1b5e20
    style Clients fill:#f57f17,color:#fff,stroke:#e65100
```

## Order Lifecycle

```mermaid
sequenceDiagram
    participant C as Client
    participant A as API Server
    participant R as Redis
    participant E as Engine Worker
    participant W as WebSocket Clients

    C->>A: POST /orders {side, price, qty}
    A->>R: INCR engine:order_id_counter
    R-->>A: order_id = 42
    A->>R: RPUSH engine:order_queue
    A-->>C: 201 Created {order_id: 42}

    Note over E: Polling queue via LPOP

    R->>E: LPOP returns Order JSON
    E->>E: match_order(order, book)
    E->>R: PUBLISH fills:events
    E->>R: SET orderbook:snapshot

    R->>A: SUBSCRIBE message
    A->>A: broadcast::send(fill)
    A->>W: WebSocket Text(fill JSON)
```

## Why Single Writer?

The core problem: if two servers match orders against the same book simultaneously, a resting order can be consumed twice (double-fill).

The single writer eliminates this by construction — one task, one book, sequential processing. Redis queue serializes all incoming orders. No locks, no CAS retry loops, no distributed consensus.

| Approach | Correctness | Complexity | Chosen? |
|----------|-------------|------------|---------|
| Single Writer (Redis Queue) | Correct by construction | Simple | **Yes** |
| Optimistic Locking (CAS) | Correct with retries | Moderate | No |
| Raft Consensus | Strongly consistent | Very high | No |
