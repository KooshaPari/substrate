# substrate

> AI dispatch gateway and TUI — proxy, rate-limit, retry, and observe LLM traffic.

## Features
- **Gateway** (axum 0.8): SSE passthrough, rate limiting, retry with full jitter, fallback chains
- **Audit log**: rotating JSONL, 50MB limit
- **Budget tracking**: per-session token/cost budgets via X-Session-Id
- **Prometheus metrics**: GET /metrics/prometheus
- **Admin API**: provider toggle, config updates, admin token auth
- **Config hot-reload**: notify crate file watcher, 200ms debounce
- **SLA checking**: P50/P95/P99 latency violation detection (defaults 200/500/1000ms)
- **TUI**: ratatui dashboard with animated boot sequence, live log panel

## Quick Start
```
process-compose up
# or:
cargo run -p gateway
cargo run -p substrate-tui
```

## API
| Endpoint | Description |
|----------|-------------|
| GET /health | Gateway health |
| GET /health/providers | Circuit breaker states |
| GET /metrics/prometheus | Prometheus format |
| POST /admin/providers/:id/toggle | Enable/disable provider |
| GET /budget/:session_id | Budget status |

## Deploy
```
podman build -t substrate-gateway .
podman run -p 3000:3000 substrate-gateway
```