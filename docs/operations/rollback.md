# Rollback Playbook

> **Audience:** On-call operators and SREs. **Scope:** Reverting any substrate
> release — gateway, CLI, driver, schema, config, A2A queue, or binary
> provenance. **Use when:** A release made things worse and `git revert` of the
> source commit is not fast enough. This document covers the **deployed**
> rollback, not the source rollback.

Companion to [`runbook.md`](./runbook.md) (health probes, alerts) and
[`CHANGELOG.md`](../../CHANGELOG.md) (release notes).

---

## 0. How to read this playbook

Each section is self-contained and walks through one surface in four blocks:

1. **Pre-flight** — snapshots / state to capture *before* you touch anything.
2. **Rollback** — exact commands to revert to the previous good release.
3. **Verify** — commands that confirm the rollback worked.
4. **Roll forward** — escape hatch if the rollback itself makes things worse.

Surfaces, in priority order:

| § | Surface | Reversibility |
|---|---|---|
| 1 | `psub-gateway` HTTP service | Reversible (process swap) |
| 2 | `psub` CLI | Reversible (`cargo install --force`) |
| 3 | `driver-http` reverse proxy | Reversible (same artifact as §1) |
| 4 | Database / on-disk state | Snapshot + restore (forward-only schema) |
| 5 | Config changes (`SUBSTRATE_CONFIG_FILE`) | Edit the file — atomic hot reload |
| 6 | A2A message queue | Claims persist; no automatic re-lease |
| 7 | Binary provenance | Re-download tagged artifact + verify SLSA attestation |

---

## 1. `psub-gateway` HTTP service

The gateway is the only process that fronts client traffic. Three deployment
shapes are covered — pick the one that matches your host. The Dockerfile ships
`psub-gateway` to `/usr/local/bin/psub-gateway`; release tags publish
`substrate-linux` (or `substrate-windows`) artifacts from
`.github/workflows/release-binary.yml`.

### 1.1 systemd

#### Pre-flight

- [ ] Note current version: `psub-gateway --version` (or read
      `/etc/substrate/build.sha` if your unit writes one).
- [ ] Capture the previous good tag: `git tag --sort=-creatordate | head -5`.
- [ ] Snapshot runtime state:

  ```bash
  sudo systemctl show psub-gateway -p ActiveEnterTimestamp,MainPID,ExecMainStartTimestamp
  sudo cp -a /var/lib/substrate /var/lib/substrate.pre-rollback.$(date +%s)
  sudo cp /etc/systemd/system/psub-gateway.service /tmp/psub-gateway.service.bak
  ```

- [ ] Confirm the prior release artifact is still on disk:

  ```bash
  ls -lh /usr/local/bin/psub-gateway*
  ```

#### Rollback

```bash
# 1. Stop the current process.
sudo systemctl stop psub-gateway

# 2. Install the previous artifact.
sudo install -m 0755 /var/lib/substrate/artifacts/psub-gateway-vX.Y.Z /usr/local/bin/psub-gateway
# Or, if your release writes a build SHA, restore it:
echo "vX.Y.Z" | sudo tee /etc/substrate/build.sha

# 3. (Optional) restore the prior unit file if you suspect the unit itself regressed.
sudo cp /tmp/psub-gateway.service.bak /etc/systemd/system/psub-gateway.service
sudo systemctl daemon-reload

# 4. Start.
sudo systemctl start psub-gateway
```

#### Verify

```bash
systemctl is-active psub-gateway
curl -fsS http://127.0.0.1:8080/healthz     # liveness (process up)
curl -fsS http://127.0.0.1:8080/health      # readiness (DB reachable)
curl -fsS http://127.0.0.1:8080/health/providers | jq 'map(select(.ok==false))'
```

#### Roll forward (escape hatch)

`git revert` the offending commit, rebuild, and `systemctl restart`. To undo a
service-unit rollback: `sudo cp /tmp/psub-gateway.service.bak /etc/systemd/system/psub-gateway.service && sudo systemctl daemon-reload && sudo systemctl restart psub-gateway`.

### 1.2 Docker

#### Pre-flight

- [ ] Capture the current image tag: `docker inspect --format '{{.Config.Image}}' psub-gateway`.
- [ ] Snapshot the SQLite store directory (bind-mounted into the container):

  ```bash
  docker cp psub-gateway:/var/lib/substrate ./substrate-state.$(date +%s)
  ```

- [ ] Record env vars:

  ```bash
  docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' psub-gateway > /tmp/psub-gateway.env
  ```

#### Rollback

```bash
# 1. Stop and remove the running container (volume is preserved).
docker stop psub-gateway && docker rm psub-gateway

# 2. Start with the previous tag. Pin by digest for repeatability.
docker run -d --name psub-gateway \
  --restart unless-stopped \
  -p 8080:8080 \
  -v substrate-data:/var/lib/substrate \
  -v /etc/substrate/config.toml:/etc/substrate/config.toml:ro \
  --env-file /tmp/psub-gateway.env \
  ghcr.io/kooshapari/substrate:vX.Y.Z
```

#### Verify

```bash
docker ps --filter name=psub-gateway --format '{{.Status}}'
docker logs --tail 50 psub-gateway | grep -i "listening\|ready"
curl -fsS http://127.0.0.1:8080/healthz
```

#### Roll forward

```bash
docker pull ghcr.io/kooshapari/substrate:vX.Y.Z-fixed
docker stop psub-gateway && docker rm psub-gateway
docker run -d --name psub-gateway --restart unless-stopped \
  -p 8080:8080 -v substrate-data:/var/lib/substrate \
  -v /etc/substrate/config.toml:/etc/substrate/config.toml:ro \
  --env-file /tmp/psub-gateway.env \
  ghcr.io/kooshapari/substrate:vX.Y.Z-fixed
```

### 1.3 Kubernetes

#### Pre-flight

- [ ] Confirm current rollout:

  ```bash
  kubectl -n substrate get deploy psub-gateway -o jsonpath='{.spec.template.spec.containers[0].image}{"\n"}'
  kubectl -n substrate get rs -l app=psub-gateway
  ```

- [ ] Capture the SQLite PVC content if it lives on a `ReadWriteOnce` volume:

  ```bash
  kubectl -n substrate exec deploy/psub-gateway -- sqlite3 /var/lib/substrate/mailbox.db ".backup '/var/lib/substrate/mailbox.snapshot.db'"
  kubectl -n substrate cp substrate/data-pod:/var/lib/substrate/mailbox.snapshot.db ./mailbox.snapshot.db
  ```

- [ ] Note recent revision history:

  ```bash
  kubectl -n substrate rollout history deploy/psub-gateway
  ```

#### Rollback

```bash
# 1. Undo the most recent rollout (k8s picks the previous ReplicaSet).
kubectl -n substrate rollout undo deploy/psub-gateway

# 2. Or roll back to a specific revision (find in `rollout history`).
kubectl -n substrate rollout undo deploy/psub-gateway --to-revision=3

# 3. Watch the rollout converge.
kubectl -n substrate rollout status deploy/psub-gateway --timeout=5m
```

#### Verify

```bash
kubectl -n substrate get pods -l app=psub-gateway -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.status.containerStatuses[0].image}{"\n"}{end}'
kubectl -n substrate exec deploy/psub-gateway -- curl -fsS http://127.0.0.1:8080/healthz
kubectl -n substrate exec deploy/psub-gateway -- curl -fsS http://127.0.0.1:8080/health
```

#### Roll forward

```bash
kubectl -n substrate rollout restart deploy/psub-gateway
# Or pin a new image and let the Deployment controller roll:
kubectl -n substrate set image deploy/psub-gateway psub-gateway=ghcr.io/kooshapari/substrate:vX.Y.Z-fixed
```

---

## 2. `psub` CLI

The `psub` binary is the operator CLI. It is *not* a daemon — rolling it back
is local. The package version lives in `Cargo.toml` workspace `version = "X.Y.Z"`.

### Pre-flight

- [ ] Capture installed version: `psub --version`.
- [ ] Find the previous good version in the registry:

  ```bash
  cargo search psub --limit 20
  # or: gh release list -R KooshaPari/substrate --limit 10
  ```

### Rollback

```bash
# 1. Pin to a known-good version. --locked prevents transitive drift
#    against the version's original Cargo.lock.
cargo install psub --locked --version X.Y.Z --force

# 2. Confirm binary is on PATH and reflects the downgrade.
which psub && psub --version
```

### Verify

```bash
psub --version                          # X.Y.Z
psub healthcheck --url http://127.0.0.1:8080/healthz
psub config show                        # if your CLI version supports it
```

### Roll forward

`cargo install psub --locked --version X.Y.Z+1 --force`. The `--force` flag
overwrites the existing `~/.cargo/bin/psub` binary in place.

---

## 3. `driver-http` reverse proxy

`driver-http` ships **in-process** with `psub-gateway` in the reference
deployment (see `runbook.md` §1, `crates/psub-gateway/Cargo.toml:14-26`), so
rolling it back is the same artifact swap as §1. If you run it as a separate
daemon (`target/release/driver-http`, see `crates/driver-http/Cargo.toml:11`):

### Pre-flight

- [ ] `ps aux | grep driver-http` — confirm whether it runs standalone or
      embedded inside the gateway.
- [ ] Snapshot: same commands as §1.

### Rollback

```bash
# Standalone deployment — same shape as §1.1/§1.2/§1.3 with the
# driver-http unit / container / Deployment instead of psub-gateway.
sudo systemctl restart driver-http          # after restoring the prior binary
```

### Verify

```bash
curl -fsS http://127.0.0.1:<driver-http-port>/healthz
journalctl -u driver-http -n 50 --no-pager
```

### Roll forward

Same as §1 — re-deploy the fixed binary; the gateway re-resolves
`driver-http` on the next request.

---

## 4. Database / on-disk state

`substrate` uses **SQLite with WAL** (`PRAGMA journal_mode=WAL`,
`crates/store-sqlite/src/schema.rs:7`). The schema is initialised with
`CREATE TABLE IF NOT EXISTS` and has **no formal migration framework**. There
is no `down.sql` — every schema change is forward-only. That means
**rolling back schema = snapshot + restore**.

### 4.1 What is durable

| Table | File | Purpose |
|---|---|---|
| `mailbox` | `mailbox.db` | A2A messages (`unread`/`delivered`/`consumed`) |
| `tasklist` | `mailbox.db` | Supervisor tasks |
| `work_queue` | `claim.db` (if split) | Atomic-claim work items |
| `gateway_config` | `gateway.db` | `ConfigEntry` kv pairs |
| `memory`, `event_log` | `gateway.db` | Memory + audit trail |

Source: `crates/store-sqlite/src/schema.rs:8-72`.

### 4.2 Pre-flight — snapshot *before* you do anything

```bash
# Online snapshot — SQLite .backup is safe under WAL.
sqlite3 /var/lib/substrate/mailbox.db ".backup '/var/lib/substrate/mailbox.db.$(date +%s).snap'"
sqlite3 /var/lib/substrate/gateway.db ".backup '/var/lib/substrate/gateway.db.$(date +%s).snap'"

# File-system snapshot (preferred on a live host with WAL — captures -wal too).
sudo systemctl stop psub-gateway        # OR freeze with sqlite3 .backup above
sudo rsync -a /var/lib/substrate/ /var/lib/substrate.pre-rollback.$(date +%s)/
sudo systemctl start psub-gateway
```

### 4.3 Rollback — schema or seed regression

There is **no `down` migration**. If `vX.Y.Z` introduced a schema change that
broke you, the only safe rollback is:

```bash
# 1. Stop the gateway so SQLite is not holding the WAL.
sudo systemctl stop psub-gateway

# 2. Restore the prior snapshot.
sudo mv /var/lib/substrate/mailbox.db /var/lib/substrate/mailbox.db.broken
sudo mv /var/lib/substrate/gateway.db /var/lib/substrate/gateway.db.broken
sudo cp /var/lib/substrate.pre-rollback.XXX/mailbox.db /var/lib/substrate/mailbox.db
sudo cp /var/lib/substrate.pre-rollback.XXX/gateway.db /var/lib/substrate/gateway.db

# 3. Restore the prior binary (§1) so the schema code matches.
sudo install -m 0755 /var/lib/substrate/artifacts/psub-gateway-vX.Y.Z /usr/local/bin/psub-gateway

# 4. Start.
sudo systemctl start psub-gateway
```

> **Why binary + snapshot together?** If you restore only the snapshot but
> keep the new binary, the new code will see an older schema it does not
> understand. Both must move in lockstep.

### 4.4 Verify

```bash
sqlite3 /var/lib/substrate/mailbox.db "PRAGMA integrity_check;"
sqlite3 /var/lib/substrate/mailbox.db ".schema mailbox"
sqlite3 /var/lib/substrate/mailbox.db "SELECT count(*), state FROM mailbox GROUP BY state;"
curl -fsS http://127.0.0.1:8080/health
```

### 4.5 Roll forward

Re-deploy the fixed binary *and* migrate the database forward with an explicit
`ALTER TABLE` / backfill script — do not rely on `CREATE TABLE IF NOT EXISTS`
to "fix" anything (it is a no-op on a table that already exists).

### 4.6 What is *not* durable

In-memory `Supervisor` fields before recovery (`conv_id`, `task_id`),
in-flight engine process state, and a claimed-but-unconsumed mailbox row's
`delivered` state — per `crates/psub-supervisor/README.md:39-44`. A
restart-with-snapshot restores the on-disk state but loses any in-flight
claims; the supervisor's `recover_active()` replays from `tasklist`.

---

## 5. Config changes (`SUBSTRATE_CONFIG_FILE`)

`psub-gateway` watches the file pointed at by `SUBSTRATE_CONFIG_FILE` and
applies changes atomically via [`ConfigWatcher`](../../crates/psub-gateway/src/config_watcher.rs).
Events are debounced 200 ms; **parse errors keep the previous valid config in
effect** (fail-safe). The watcher is wired in
`crates/psub-gateway/src/lib.rs:303-336`. Live-reloadable fields:

| Field | Effect |
|---|---|
| `auth_token` | Bearer token for protected routes |
| `rate_limit_rps` | Per-IP requests-per-second cap (`0` = unlimited) |
| `retry_attempts` | Upstream retry attempts on transient errors |
| `enabled_providers` | Allow-list (empty = all built-ins) |

> **Bind address / port changes require a restart.** They are not in
> `FileConfig`.

### Pre-flight

- [ ] `echo "$SUBSTRATE_CONFIG_FILE"` — confirm the path.
- [ ] Snapshot: `sudo cp -a "$SUBSTRATE_CONFIG_FILE"{,.bak.$(date +%s)}`.
- [ ] Tail the watcher logs: `journalctl -u psub-gateway -f | grep config_watcher`.

### Rollback

```bash
# Atomic rewrite — write to a temp file and rename, so the watcher
# sees a single replacement event.
sudo cp /etc/substrate/config.toml.bak.<ts> /tmp/config.toml
sudo install -m 0644 /tmp/config.toml /etc/substrate/config.toml
```

The watcher picks up the change within 200 ms. No restart needed.

### Verify

```bash
journalctl -u psub-gateway -n 20 --no-pager | grep "config_watcher"
# Should print: "[config_watcher] reloaded config from /etc/substrate/config.toml"

# Confirm the new values are live.
curl -fsS -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/v1/models | jq
```

### Roll forward

Edit the file again. To force a full restart (e.g. for a bind change):
`sudo systemctl restart psub-gateway`.

---

## 6. A2A message queue

The mailbox uses **CAS claims**, not leases. The atomic primitive is
`UPDATE mailbox SET state='delivered' WHERE id=? AND state='unread'` — see
`crates/store-sqlite/src/store.rs:195-202`. Per the supervisor contract in
`crates/psub-supervisor/README.md:24-26`:

> This is an at-most-once processing lock. It prevents duplicate processing
> under contention, but it does **not** currently lease or automatically
> requeue messages that were claimed and then abandoned before `consume`.

That has three operational consequences:

1. **A rolled-back gateway does *not* auto-recover abandoned claims.** A
   crashed gateway leaves `state='delivered'` rows that no other worker will
   pick up — they are stuck until `recover_active()` re-classifies them
   (manual) or an operator resets them.
2. The serve-lock (`crates/substrate-serve-lock/`) is a *pidfile advisory
   lock*, **not a TTL lease**. A dead holder is detected via `kill(pid, 0)`
   and the next process transparently takes over. No timer-based recovery.
3. There is no 1-hour TTL on anything in the A2A queue. The "TTL" concept
   does not apply at the substrate layer.

### Pre-flight

```bash
sqlite3 /var/lib/substrate/mailbox.db <<'SQL'
.headers on
SELECT state, count(*) FROM mailbox GROUP BY state;
SELECT id, team_id, from_agent, to_agent, state, created_at
  FROM mailbox WHERE state='delivered'
  ORDER BY created_at DESC LIMIT 20;
SQL
```

### Rollback

If the new gateway mis-claimed messages (e.g. claimed everything to a dead
worker), reset them so `recover_active()` can re-claim:

```bash
# 1. Snapshot the mailbox first.
sqlite3 /var/lib/substrate/mailbox.db ".backup '/var/lib/substrate/mailbox.db.$(date +%s).snap'"

# 2. Re-open any messages stuck in 'delivered' (be careful: this re-enables
#    duplicate processing for any in-flight worker that hasn't called
#    consume yet).
sqlite3 /var/lib/substrate/mailbox.db <<'SQL'
UPDATE mailbox
   SET state='unread'
 WHERE state='delivered'
   AND consumed_at IS NULL;
SQL

# 3. Roll the gateway back per §1.
sudo systemctl restart psub-gateway
```

### Verify

```bash
sqlite3 /var/lib/substrate/mailbox.db "SELECT state, count(*) FROM mailbox GROUP BY state;"
# Expect: 'unread' growing, 'delivered' draining, 'consumed' monotonic.

# On the supervisor side, after restart:
psub supervisor status --team <TEAM_ID> --agent <AGENT_NAME>
```

### Roll forward

Re-deploy the fixed binary; new claims will use the correct code path. If
you reset `delivered` → `unread` above, expect *some* duplicate processing
for messages that were actually mid-flight when you reset — the application
layer must be idempotent or you'll need to drain those messages manually.

---

## 7. Binary provenance

`substrate` builds release binaries via `.github/workflows/release-binary.yml`
on tag push (`v*`), and signs them with SLSA Build Level 3 provenance per
`.github/workflows/security.yml:76-86`. Rolling back means re-downloading the
prior tag's binary and verifying its attestation.

> **Status caveat:** `audit_scorecard.json` flags SC-10/RE-13 as `missing`
> for current releases. Treat SLSA verification as best-effort until the
> `slsa-provenance` job runs end-to-end on the tag you are rolling back to.

### Pre-flight

```bash
# Confirm the previous good tag exists.
gh release list -R KooshaPari/substrate --limit 10

# Locate the artifact + attestation.
gh release view vX.Y.Z -R KooshaPari/substrate \
  --json assets --jq '.assets[] | select(.name|test("substrate-linux"))'
```

### Rollback

```bash
# 1. Download the prior tag's binary.
gh release download vX.Y.Z -R KooshaPari/substrate \
  -p "substrate-linux" -D /tmp/substrate-rollback
chmod +x /tmp/substrate-rollback/substrate-linux

# 2. Verify the SLSA provenance attestation. This proves the binary
#    was built by the official GitHub Actions workflow from the tagged
#    commit.
gh attestation verify /tmp/substrate-rollback/substrate-linux \
  --repo KooshaPari/substrate \
  --signer-workflow KooshaPari/substrate/.github/workflows/release-binary.yml
# Exit code 0 = provenance verified; non-zero = DO NOT deploy.

# 3. Install.
sudo install -m 0755 /tmp/substrate-rollback/substrate-linux /usr/local/bin/psub-gateway

# 4. Restart per §1.
sudo systemctl restart psub-gateway
```

### Verify

```bash
psub-gateway --version
sha256sum /usr/local/bin/psub-gateway
# Cross-check against the SHA256 in the GitHub release notes for vX.Y.Z.
```

### Roll forward

Re-run with `vX.Y.Z-fixed` instead. If the rollback's provenance verification
*failed* (which is possible if the attestation was never generated for that
tag — see `audit_scorecard.json`), you can manually pin to the source commit:

```bash
git checkout vX.Y.Z
cargo build --release --locked --bin psub-gateway
sudo install -m 0755 target/release/psub-gateway /usr/local/bin/psub-gateway
```

This is a **last-resort** path because it bypasses the release pipeline. Log
the deviation in the incident report and open a follow-up to backfill the
attestation for the next release.

---

## Appendix A — One-page checklist

```
[ ] Snapshot SQLite stores (§4.2)
[ ] Capture current binary version (§1)
[ ] Snapshot config file (§5)
[ ] Snapshot the systemd unit / k8s Deployment / docker-compose (§1)
[ ] Stop the gateway (§1)
[ ] Restore prior binary + snapshot together (§4.3)
[ ] Restore prior config if needed (§5)
[ ] Reset stuck A2A claims (§6)
[ ] Restart (§1)
[ ] Verify /healthz, /health, /health/providers (§1)
[ ] Verify SLSA attestation (§7)
[ ] File incident report; link the rollback commit + this playbook section
```