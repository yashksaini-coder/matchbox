# Configuration

Matchbox is configured entirely through environment variables.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis connection URL |
| `PORT` | `8080` | HTTP server listen port |
| `RUST_LOG` | *(none)* | Log level filter |

## Examples

```bash
# Default — connects to local Redis on 6379, serves on 8080
cargo run -p server

# Custom port
PORT=3000 cargo run -p server

# Remote Redis
REDIS_URL=redis://redis.example.com:6379 cargo run -p server

# Debug logging
RUST_LOG=debug cargo run -p server

# Module-specific logging
RUST_LOG=server::engine_worker=debug,info cargo run -p server
```

## Docker Compose

The included `docker-compose.yml` runs Redis:

```yaml
services:
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    command: redis-server --appendonly no
```

```bash
docker compose up -d      # Start Redis
docker compose down        # Stop and remove
docker compose logs redis  # View Redis logs
```
