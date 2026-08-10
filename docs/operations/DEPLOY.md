# Deployment Guide

> **Audience:** Operators deploying `substrate-gateway` and `substrate-http` binaries.
> **Environments:** Development (docker-compose), Staging (single node), Production (multi-node behind LB).

---

## 1. Prerequisites

| Requirement | Minimum version | Notes |
|---|---|---|
| Rust toolchain | 1.80+ | `rustup target add x86_64-unknown-linux-gnu` for cross-compile |
| Docker (container path only) | 24+ | For GHCR image builds |
| systemd (bare-metal path) | 250+ | Unit files provided below |
| OpenTelemetry Collector (optional) | any | Only needed if `OTEL_EXPORTER_OTLP_ENDPOINT` is set |

---

## 2. Build

### Native binary

```bash
# Release build (optimized for size; see Cargo.toml [profile.release])
cargo build --release --workspace --exclude fuzz

# Binaries produced at:
#   target/release/substrate-gateway  (OpenAI-compatible HTTP gateway)
#   target/release/substrate-http     (HTTP dispatch driver)
#   target/release/substrate          (CLI)
```

### Docker image (GHCR)

```bash
docker build -t ghcr.io/kooshapari/substrate/gateway:latest .
# Multi-arch build:
docker buildx build --platform linux/amd64,linux/arm64 \
  -t ghcr.io/kooshapari/substrate/gateway:latest \
  --push .
```

---

## 3. Configuration

All configuration is via environment variables. See each binary's `config.rs` for defaults.

### Shared

| Variable | Required | Default | Description |
|---|---|---|---|
| `STATE_DIR` | no | `/var/lib/substrate` | Data directory (SQLite DBs, mailboxes) |
| `AUTH_TOKEN` | no | — | Bearer token for protected routes |
| `RUST_LOG` | no | `info` | Tracing filter expression |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | no | — | OTLP gRPC endpoint (e.g. `http://otel-collector:4317`) |

### substrate-gateway only

| Variable | Required | Default | Description |
|---|---|---|---|
| `BIND` | no | `0.0.0.0:8080` | HTTP listen address |
| `SUBSTRATE_CONFIG_FILE` | no | — | Path to TOML config file (hot-reloadable) |
| `SUBSTRATE_ADMIN_TOKEN` | no | — | Token for `/admin/*` routes |
| `SUBSTRATE_AUDIT_LOG` | no | — | Path to JSONL audit log file |

### substrate-http only

| Variable | Required | Default | Description |
|---|---|---|---|
| `BIND` | no | `0.0.0.0:8081` | HTTP listen address |

---

## 4. Deployment methods

### 4.1 systemd (bare-metal / VM)

**`/etc/systemd/system/substrate-gateway.service`:**

```ini
[Unit]
Description=Substrate gateway
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=substrate
Group=substrate
EnvironmentFile=/etc/substrate/gateway.env
ExecStart=/usr/local/bin/substrate-gateway
Restart=always
RestartSec=5
TimeoutStopSec=30
LimitNOFILE=65536
AmbientCapabilities=CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
```

**`/etc/systemd/system/substrate-http.service`:**

```ini
[Unit]
Description=Substrate HTTP driver
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=substrate
Group=substrate
EnvironmentFile=/etc/substrate/http.env
ExecStart=/usr/local/bin/substrate-http
Restart=always
RestartSec=5
TimeoutStopSec=30
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

**Deploy steps:**

```bash
sudo useradd --system --shell /usr/sbin/nologin --home-dir /var/lib/substrate substrate
sudo mkdir -p /var/lib/substrate /etc/substrate
sudo cp target/release/substrate-gateway /usr/local/bin/
sudo cp target/release/substrate-http /usr/local/bin/
sudo cp config/prod/gateway.env /etc/substrate/gateway.env
sudo cp config/prod/http.env /etc/substrate/http.env
sudo systemctl daemon-reload
sudo systemctl enable --now substrate-gateway substrate-http
```

### 4.2 Docker (container)

**`docker-compose.yml`:**

```yaml
version: "3.9"
services:
  gateway:
    image: ghcr.io/kooshapari/substrate/gateway:latest
    ports:
      - "8080:8080"
    environment:
      STATE_DIR: /data
      AUTH_TOKEN: ${AUTH_TOKEN}
      RUST_LOG: info,psub_gateway=debug
      BIND: 0.0.0.0:8080
    volumes:
      - substrate-data:/data
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://127.0.0.1:8080/healthz"]
      interval: 30s
      timeout: 5s
      retries: 3

volumes:
  substrate-data:
```

**Run:**

```bash
docker compose up -d
```

### 4.3 Kubernetes (YAML)

Minimal `Deployment` for the gateway:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: substrate-gateway
spec:
  replicas: 2
  selector:
    matchLabels:
      app: substrate-gateway
  template:
    metadata:
      labels:
        app: substrate-gateway
    spec:
      containers:
        - name: gateway
          image: ghcr.io/kooshapari/substrate/gateway:latest
          ports:
            - containerPort: 8080
              name: http
          envFrom:
            - secretRef:
                name: substrate-gateway-env
          livenessProbe:
            httpGet:
              path: /healthz
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 30
          readinessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 15
          resources:
            requests:
              cpu: 250m
              memory: 256Mi
            limits:
              cpu: 1000m
              memory: 512Mi
---
apiVersion: v1
kind: Service
metadata:
  name: substrate-gateway
spec:
  selector:
    app: substrate-gateway
  ports:
    - port: 8080
      targetPort: http
      name: http
```

---

## 5. Verification

After deploying, verify the service is healthy:

```bash
# Liveness
curl -fsS http://localhost:8080/healthz && echo "LIVE"

# Readiness
curl -fsS http://localhost:8080/health | jq .

# Provider health
curl -fsS http://localhost:8080/health/providers | jq .

# Chat completion (requires auth if AUTH_TOKEN is set)
curl -fsS http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -d '{"model":"openai/gpt-4","messages":[{"role":"user","content":"hello"}]}' | jq .
```

Logs should show:
```
substrate-gateway starting bind=0.0.0.0:8080 state_dir=/var/lib/substrate
```

---

## 6. Rollback

See `docs/operations/rollback.md` for the full rollback playbook. Quick summary:

```bash
# systemd: replace binary and restart
sudo cp target/release/substrate-gateway /usr/local/bin/substrate-gateway
sudo systemctl restart substrate-gateway

# Docker: re-tag previous image
docker compose down
docker compose up -d
```
