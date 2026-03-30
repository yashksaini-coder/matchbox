# Getting Started

## Prerequisites

- [Rust](https://rustup.rs/) (stable, 2021 edition)
- [Docker](https://docs.docker.com/get-docker/) (for Redis)

## Quick Start

```bash
# 1. Start Redis
docker compose up -d

# 2. Run tests
cargo test --workspace

# 3. Start the server
RUST_LOG=info cargo run -p server

# 4. Submit a sell order
curl -X POST localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"sell","price":50,"qty":10}'

# 5. Submit a matching buy order
curl -X POST localhost:8080/orders \
  -H "Content-Type: application/json" \
  -d '{"side":"buy","price":50,"qty":10}'

# 6. Check the order book (should be empty after match)
curl localhost:8080/orderbook

# 7. Watch fills in real-time
websocat ws://localhost:8080/ws
```

## Without Docker

If you have Redis installed locally:

```bash
# Start Redis
redis-server

# Run the server (Redis defaults to localhost:6379)
RUST_LOG=info cargo run -p server
```

## Building

```bash
# Debug build
cargo build --workspace

# Release build
cargo build --workspace --release

# Run tests
cargo test --workspace

# Lint
cargo clippy --all-targets

# Format
cargo fmt --all
```
