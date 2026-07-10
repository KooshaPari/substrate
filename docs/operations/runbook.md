# Operations Runbook

> **Audience:** On-call operators and site-reliability engineers.
> **Scope:** The `psub-gateway` HTTP service, the `psub` CLI, the `driver-http`
> reverse-proxy driver, and the in-process `psub-core` routing layer.
> **When to use:** Anything between "it's degraded" and "it's down".

This is a living document. If you do something during an incident that isn't
here, add it to this file as part of your post-incident review. Edits to this
file route to `@KooshaPari/devops` per
[`.github/CODEOWNERS`](../../.github/CODEOWNERS).

---

## 1. Service topology

```
                 ┌───────────────────────────────────────────────┐
   client ─────► │ psub-gateway (axum 0.7, port 8080, systemd)  │
                 │   ├─ /v1/chat/completions  (OpenAI shape)     │
                 │   ├─ /v1/models            (model catalog)   │
                 │   ├─ /a2a/messages, /a2a/inbox, /a2a/tasks   │
                 │   ├─ /healthz, /health, /health/providers    │
                 │   ├─ /metrics, /metrics/prometheus           │
                 │   ├─ /management/config                      │
                 │   └─ /admin/* (admin token required)         │
                 └──────────────────┬────────────────────────────┘
                                    │  in-process (no IPC)
                                    ▼
                 ┌───────────────────────────────────────────────┐
                 │ psub-core  (router + DAG + budget)           │
                 │   ├─ Provider chain + retry / fallback        │
                 │   ├─ Circuit breaker                         │
                 │   ├─ Rate limit (per-IP, configurable)       │
                 │   └─ Budget enforcement (token / USD)        │
                 └──────────────────┬────────────────────────────┘
                                    │
            ┌───────────────────────┼──────────────────────┐
            ▼                       ▼                      ▼
        OpenAI                 Anthropic              Gemini
                                       + driver-http (reverse-proxy)
                                            │
                                            ▼
                                       upstream LLM
```

- `psub-gateway` is the **public HTTP surface**. It is the only process you
  redeploy to roll forward/back a hot fix on the routing or admin layer.
  Route table is defined in [`crates/psub-gateway/src/lib.rs:257-298`][router].
- `psub-core` is the **in-process routing engine**. There is no IPC hop — it
  runs inside the same tokio runtime as the gateway and is wired through
  `PhenotypeRouterAdapter` (see
  [`crates/psub-gateway/src/lib.rs:61`](../../crates/psub-gateway/src/lib.rs)).
- `driver-http` is the **reverse-proxy driver** for non-OpenAI-shaped upstreams
  (Anthropic, Gemini, custom). It terminates the upstream HTTP call inside the
  gateway process; there is no separate daemon.
- `psub` CLI is the operator tool — same binary as the gateway, dispatching on
  `argv[0]`. It is not a daemon and is not in the request path.

[router]: ../../crates/psub-gateway/src/lib.rs#L257-L298

### Process-to-port map

| Process          | Default port | Transport | Auth                          |
| ---------------- | ------------ | --------- | ----------------------------- |
| `psub-gateway`   | `8080`       | HTTP/1.1  | Bearer token (optional)       |
| `psub` CLI       | n/a          | n/a       | local filesystem only         |
| `driver-http`    | in-process   | HTTP/1.1  | per-provider upstream secret  |

---

## 2. Health probes

All endpoints are registered in [`crates/psub-gateway/src/lib.rs:288-293`][hp].

[hp]: ../../crates/psub-gateway/src/lib.rs#L288-L293

| Endpoint             | Method | Purpose                              | Auth         | Probe type   |
| -------------------- | ------ | ------------------------------------ | ------------ | ------------ |
| `/healthz`           | GET    | Liveness — process is up and serving | none         | liveness     |
| `/health`            | GET    | Readiness — DB reachable, providers  | none         | readiness    |
| `/health/providers`  | GET    | Per-provider reachability report     | none         | diagnostic   |
| `/metrics`           | GET    | JSON metrics snapshot                | none         | observability |
| `/metrics/prometheus`| GET    | Prometheus text format v0.0.4        | none         | scrape       |
| `/metrics/reset`     | POST   | Reset in-memory counters (admin)     | admin token  | operator     |

### Probe semantics

- **Liveness (`/healthz`)** — returns `200 OK` as long as the axum router is
  serving. Use this for `livenessProbe`. A failing `/healthz` means the process
  is hung and should be killed by the orchestrator.
- **Readiness (`/health`)** — returns `200` only when the SQLite store is open
  and at least one provider is configured. Use this for `readinessProbe`.
  A failing `/health` means "do not send traffic yet", not "kill me".
- **Providers (`/health/providers`)** — JSON map of `provider_id → {ok, latency_ms, last_error}`.
  Polled on incident triage, not by the orchestrator.

### Example probes (Kubernetes)

```yaml
livenessProbe:
  httpGet: { path: /healthz, port: 8080 }
  periodSeconds: 10
  failureThreshold: 3

readinessProbe:
  httpGet: { path: /health, port: 8080 }
  periodSeconds: 5
  failureThreshold: 2
```

---

## 3. Common alerts

Each alert names the Prometheus / observability signal, the likely cause, and
the **first command to run** for triage. Do not skip the command — even a 5
second `curl` saves 5 minutes of guessing.

### 3.1 `RequestsFlatFor10m`

**Signal:** `rate(http_requests_total[10m]) == 0` while the gateway is up.
**Likely cause:** upstream outage, auth misconfiguration, or DNS poisoning.

```bash
# 1. Confirm the gateway is serving
curl -fsS http://127.0.0.1:8080/healthz

# 2. Inspect a sample 401/403
curl -i -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/v1/models

# 3. Look at last 200 log lines
journalctl -u psub-gateway -n 200 --no-pager
```

If `/healthz` is `200` but `requests_total` is flat, the gateway is healthy but
**clients can't reach it** — check LB / ingress / firewall, not the gateway.

### 3.2 `FiveXXSpike`

**Signal:** `rate(http_requests_total{status=~"5.."}[5m]) > 0.05` of total.
**Likely cause:** upstream provider outage, DB lock contention, OOM.

```bash
# 1. Top failing routes
curl -s http://127.0.0.1:8080/metrics | jq '.by_route | sort_by(-.errors)[:10]'

# 2. Per-provider health
curl -s http://127.0.0.1:8080/health/providers | jq 'to_entries | sort_by(.value.ok)'

# 3. Tail logs for ERROR level
journalctl -u psub-gateway -p err -n 200 --no-pager
```

### 3.3 `RateLimited429Spike`

**Signal:** `rate(http_requests_total{status="429"}[5m]) > 0.20`.
**Likely cause:** `rate_limit_rps` too low, hot-loop client, or upstream
provider returning 429s.

```bash
# 1. Check whether 429s come from us or from upstream
curl -s http://127.0.0.1:8080/metrics | jq '.by_status."429"'

# 2. Inspect active rate-limit buckets
journalctl -u psub-gateway --since "10 min ago" | grep -i 'rate.limit'

# 3. If upstream, see §3.4
```

### 3.4 `ProviderDown`

**Signal:** `/health/providers` reports `ok=false` for any provider for > 2 min.
**Likely cause:** upstream API outage, expired key, model deprecation.

```bash
# 1. Confirm which provider(s)
curl -s http://127.0.0.1:8080/health/providers | jq

# 2. Check provider-specific recent errors
journalctl -u psub-gateway --since "5 min ago" | grep -E 'provider=(openai|anthropic|gemini)'

# 3. Verify upstream independently
curl -fsS https://status.openai.com/ 2>&1 | head -5   # or equivalent
```

If the upstream status page is red, page the provider (see §9). If green,
the key or routing config is wrong — go to §6.

### 3.5 `SLSA / SBOM alert`

**Signal:** GitHub Security tab flags a missing attestation, or
`supply-chain-audit` workflow fails.
**Likely cause:** new dependency without provenance, release image not signed,
or SBOM drift.

```bash
# 1. Inspect the failing run
gh run list --workflow=supply-chain-audit --limit 5

# 2. Re-run after pinning the suspect dep
cargo update -p <crate> --precise <version>

# 3. Verify locally
cargo deny check
```

If the alert fires on a **release tag**, **freeze the release pipeline** until
the provenance check passes. See §7 for the redeploy procedure once resolved.

---

## 4. Logs

### 4.1 Where to look

| Runtime        | Location                                                | Tail command                                                |
| -------------- | ------------------------------------------------------- | ----------------------------------------------------------- |
| systemd        | journal (`psub-gateway.service`)                        | `journalctl -u psub-gateway -f`                             |
| Docker         | container stderr → `docker logs psub-gateway`           | `docker logs -f --tail 200 psub-gateway`                    |
| Kubernetes     | kubelet → `kubectl logs`                                | `kubectl -n substrate logs -l app=psub-gateway --tail=200` |
| Bare-metal dev | stderr of `cargo run -p psub-gateway`                   | (whatever the terminal captured)                            |

Structured fields you can grep for: `request_id`, `provider`, `model`,
`status`, `latency_ms`, `session_id`, `error.kind`.

### 4.2 Enable debug logging

Set `RUST_LOG=debug` (or `trace` for full axum internals) and restart the
gateway. In each runtime:

```bash
# systemd — drop-in override
sudo systemctl edit psub-gateway
# add:
#   [Service]
#   Environment=RUST_LOG=debug,psub_gateway=trace
sudo systemctl restart psub-gateway

# Docker
docker run -e RUST_LOG=debug,psub_gateway=trace ... psub-gateway

# Kubernetes
kubectl -n substrate set env deploy/psub-gateway RUST_LOG=debug,psub_gateway=trace
kubectl -n substrate rollout restart deploy/psub-gateway
```

`debug` adds request / response framing. `trace` adds full axum tower
middleware output — only enable for < 5 min during an active incident, the
volume is large.

---

## 5. Configuration reload

`psub-gateway` watches the file at `$SUBSTRATE_CONFIG_FILE` (TOML) and applies
live changes without restart. The watcher is implemented in
[`crates/psub-gateway/src/config_watcher.rs:76`][cw] and wired in
[`crates/psub-gateway/src/lib.rs:325`][cw-wire]. Events are debounced by
200 ms — multiple writes collapse into one reload.

[cw]:       ../../crates/psub-gateway/src/config_watcher.rs#L76
[cw-wire]:  ../../crates/psub-gateway/src/lib.rs#L325

### Live-reloadable fields

| Field             | Type            | Notes                                          |
| ----------------- | --------------- | ---------------------------------------------- |
| `auth_token`      | `Option<String>`| Swap the bearer token without restart          |
| `rate_limit_rps`  | `u32`           | `0` = unlimited                                |
| `retry_attempts`  | `u32`           | Defaults to 3                                  |
| `enabled_providers`| `Vec<String>`   | Empty = all built-ins enabled                  |

Anything **not** in this list (e.g. `bind` address) requires a full restart.

### Procedure

```bash
# 1. Validate the TOML locally before pointing the gateway at it
tomlq . /etc/substrate/config.toml

# 2. Atomic write (rename is what notify watches for)
sudo install -m 0640 /tmp/config.toml /etc/substrate/config.toml.new
sudo mv /etc/substrate/config.toml.new /etc/substrate/config.toml

# 3. Confirm the watcher picked it up
journalctl -u psub-gateway -n 20 --no-pager | grep config_watcher
# expected: "[config_watcher] reloaded config from /etc/substrate/config.toml"
```

Parse errors are **non-fatal** — the previous valid config stays in effect.
If you see `[config_watcher] parse error in … (keeping previous config)`,
your new config has a syntax error; fix and retry.

---

## 6. Recovery playbooks

### 6.1 Wedged gateway

Symptoms: `/healthz` hangs, no new log lines, axum tasks stalled. The
gateway is alive but not making progress.

```bash
# 1. Confirm it really is wedged (not just slow)
curl --max-time 3 -fsS http://127.0.0.1:8080/healthz || echo WEDGED

# 2. Get the PID
PID=$(systemctl show -p MainPID psub-gateway | cut -d= -f2)

# 3. Trigger a graceful state-dump + soft restart
sudo kill -SIGUSR1 "$PID"

# 4. Watch for the dump to land
journalctl -u psub-gateway -n 50 --no-pager | grep -E 'SIGUSR1|dump|shutdown'

# 5. If still wedged after 30s, escalate to SIGTERM, then SIGKILL
sudo kill -SIGTERM  "$PID" && sleep 10
sudo kill -SIGKILL   "$PID"   # last resort
```

If `SIGUSR1` is not wired in your build (verify with `grep SIGUSR1` in the
crate), jump straight to `SIGTERM`. The orchestrator will respawn the
process.

### 6.2 Data dir corruption

Symptoms: `readinessProbe` fails with SQLite I/O errors; `journalctl` shows
`database disk image is malformed` or `file is not a database`.

```bash
# 1. Stop the gateway so nothing writes during repair
sudo systemctl stop psub-gateway

# 2. Run integrity check
sudo -u substrate sqlite3 /var/lib/substrate/store.db "PRAGMA integrity_check;"

# 3a. If "ok" — restart, you're done
sudo systemctl start psub-gateway

# 3b. If errors — try .dump | .reload (lossy but preserves schema)
sudo -u substrate sqlite3 /var/lib/substrate/store.db ".dump" \
  | sudo -u substrate sqlite3 /var/lib/substrate/store.db.new
sudo mv /var/lib/substrate/store.db.new /var/lib/substrate/store.db

# 4. If still broken — restore from backup (see §6.3)
```

### 6.3 Total data loss

When the data dir is gone (disk failure, accidental `rm -rf`) and no hot
backup is reachable:

```bash
# 1. Stop the gateway
sudo systemctl stop psub-gateway

# 2. Restore from the latest nightly snapshot
sudo rsync -a /var/backups/substrate/$(date +%Y%m%d)/ \
              /var/lib/substrate/

# 3. If no snapshot, cold-start with an empty data dir and reseed provider
#    credentials via the admin API
sudo mkdir -p /var/lib/substrate
sudo chown substrate:substrate /var/lib/substrate
sudo systemctl start psub-gateway

# 4. Reseed credentials
psub admin providers set openai --api-key "$OPENAI_KEY"
psub admin providers set anthropic --api-key "$ANTHROPIC_KEY"
```

Open a SEV-2 incident — cold-start loses audit log and budget history.

### 6.4 Suspected key compromise

A provider API key, the gateway `auth_token`, or the admin token is exposed
(stolen laptop, leaked env var, secret in a commit).

```bash
# 1. Rotate upstream provider key at the provider console (OpenAI, etc.)
#    Do this FIRST — the gateway can keep running with a stale key for ~30s.

# 2. Rotate the gateway auth_token live (no restart required)
sudo install -m 0640 /tmp/config.toml /etc/substrate/config.toml.new
sudo mv /etc/substrate/config.toml.new /etc/substrate/config.toml

# 3. Rotate the admin token — requires restart
sudo systemctl edit psub-gateway
# add:  Environment=PSUB_ADMIN_TOKEN=$(openssl rand -hex 32)
sudo systemctl restart psub-gateway

# 4. Audit: who used the old token in the last 24h
journalctl -u psub-gateway --since "24 hours ago" \
  | grep -E 'admin|token' | head -200
```

Page the security on-call (see §9) for SEV-1 if the key had `write` scope or
if the audit log shows unauthorized access.

---

## 7. Restart / redeploy procedures

### 7.1 systemd

```bash
# Rolling restart
sudo systemctl restart psub-gateway
sudo systemctl status psub-gateway --no-pager

# Tail logs to confirm clean startup
journalctl -u psub-gateway -n 100 -f
# expect: "gateway listening on 0.0.0.0:8080"
```

Unit file lives at `/etc/systemd/system/psub-gateway.service`. Override with
`systemctl edit` — do not edit the file directly.

### 7.2 Docker

```bash
# Pull and re-create (compose)
cd /opt/substrate
docker compose pull psub-gateway
docker compose up -d psub-gateway
docker compose logs -f --tail 200 psub-gateway
```

For a fresh image, tag explicitly: `docker compose pull psub-gateway:vX.Y.Z`.

### 7.3 Kubernetes

```bash
# Standard rolling restart — image unchanged
kubectl -n substrate rollout restart deploy/psub-gateway
kubectl -n substrate rollout status  deploy/psub-gateway --timeout=120s

# Bump image to a new tag
kubectl -n substrate set image deploy/psub-gateway \
  psub-gateway=ghcr.io/kooshapari/psub-gateway:vX.Y.Z
kubectl -n substrate rollout status deploy/psub-gateway --timeout=120s

# Rollback if the rollout is bad
kubectl -n substrate rollout undo deploy/psub-gateway
```

Confirm with the readiness probe (§2) before declaring success.

---

## 8. 5-minute on-call triage checklist

When the page goes off, run this list top-to-bottom. Most incidents resolve
inside one of these steps.

1. **`/healthz`** — is the process alive?
   `curl --max-time 3 -fsS http://127.0.0.1:8080/healthz`
2. **`/health`** — is it ready to serve?
   `curl --max-time 3 -fsS http://127.0.0.1:8080/health`
3. **`/health/providers`** — which upstream is sick?
   `curl -s http://127.0.0.1:8080/health/providers | jq`
4. **`/metrics`** — error rate, p99, rate-limit hits.
   `curl -s http://127.0.0.1:8080/metrics | jq '.errors, .latency_p99_ms'`
5. **Logs** — last 200 lines, ERROR first.
   `journalctl -u psub-gateway -p err -n 200 --no-pager`
6. **Recent deploy** — did something land in the last hour?
   `git log --since "1 hour ago" --oneline`
   `kubectl -n substrate rollout history deploy/psub-gateway`
7. **Upstream status** — is it just us?
   `curl -fsS https://status.openai.com/ 2>&1 | head -5`
8. **Suspect config drift?** Diff the live `FileConfig` against the last
   known-good TOML in git:
   `curl -s http://127.0.0.1:8080/management/config | jq`
9. **Mitigate first, root-cause second.** If users are blocked, restart (see
   §7) or revert the latest deploy before diving deeper.
10. **Page out** per §9 if you're still stuck after 5 minutes.

---

## 9. Escalation matrix

| Severity | Definition                                          | Response time | First responder       | Escalation path                                    |
| -------- | --------------------------------------------------- | ------------- | --------------------- | -------------------------------------------------- |
| **SEV-1** | Total outage OR key compromise in last 24h           | **page immediately**, 24/7 | Primary on-call       | → Secondary on-call (15 min) → Eng manager (30 min) → VP-Eng (60 min) |
| **SEV-2** | Major degradation (5xx > 5%, single provider down) | acknowledge ≤ 15 min, business hours | Primary on-call | → Secondary on-call (30 min) → Eng manager (2 h) |
| **SEV-3** | Minor degradation, single non-critical provider flapping | next business day | Primary on-call       | log to incident tracker; resolve in normal sprint cadence |

### Paging channels

- **Primary:** PagerDuty service `psub-gateway-prod` (rotation in `#oncall-psub`).
- **Security** (key compromise only): `#sec-incidents` + page `@security-oncall`.
- **Provider outages** (OpenAI / Anthropic / Gemini): file a ticket at the
  provider's status page; do **not** wait on their support.

### Communications

- **Internal:** `#incidents` Slack channel — post a status update every 15 min
  during SEV-1, every 60 min during SEV-2.
- **External:** customer-facing status updates go through the comms team via
  `#comms-bridge`; do not post customer-facing copy yourself.

---

## 10. Code ownership

Ops-lane edits to this file and the rest of `docs/operations/` route to
`@KooshaPari/devops` per
[`.github/CODEOWNERS:81`](../../.github/CODEOWNERS#L81). Changes to
`/crates/psub-gateway/`, `/crates/driver-http/`, and the rest of the
inbound-adapter tree route to `@KooshaPari/gateway` (see
[`.github/CODEOWNERS:40-50`](../../.github/CODEOWNERS#L40-L50)). When a
playbook change crosses both boundaries (e.g. a new endpoint), request
reviewers from both teams in the PR template.