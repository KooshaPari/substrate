# psub-rename-migration

> **Migration guide for the 7-crate `psub-` prefix rename (commit `cb9a3e7`).**
>
> Read this if you depend on substrate crates, run the gateway binary,
> pull the container image, or write `.toml` config files.

## 1. Why the rename

The substrate workspace shipped to crates.io as `v0.1.0` from this repo,
but seven crate names collided with already-claimed registry names.
`cargo publish` rejects a `name = "..."` whose value is already owned by
another user, even if the local crate is unrelated.

Collisions documented in commit `cb9a3e7`:

| Old crate name | crates.io owner                                       |
| -------------- | ----------------------------------------------------- |
| `substrate`    | `parity-crate-owner` (paritytech / Frontier)          |
| `gateway`      | `kkharji` (`gateway-rs` family)                       |
| `a2a`          | `eopb`                                                |
| `file-watcher` | `softprops`                                           |
| `orchestrator` | `elasticrash`                                         |
| `supervisor`   | `shockham`                                            |
| `wave`         | `flxzt`                                               |

Resolution: every colliding crate is renamed with a `psub-` prefix
(**P**henotype **sub**strate), matching the `psub` binary that has shipped
since v0.2.0. The `psub-` namespace is unique to this repo, so the
collision cannot recur unless another Phenotype workspace re-uses it.

No functional change — module structure, public API, and the 2,254-test
suite are all preserved.

## 2. Mapping table

Verified against `crates/` on `main` (post-rename):

| Old crate dir         | New crate dir              | Old package name | New package name      |
| --------------------- | -------------------------- | ---------------- | --------------------- |
| `crates/substrate`    | `crates/psub`              | `substrate`      | `psub`                |
| `crates/gateway`      | `crates/psub-gateway`      | `gateway`        | `psub-gateway`        |
| `crates/a2a`          | `crates/psub-a2a`          | `a2a`            | `psub-a2a`            |
| `crates/file-watcher` | `crates/psub-file-watcher` | `file-watcher`   | `psub-file-watcher`   |
| `crates/orchestrator` | `crates/psub-orchestrator` | `orchestrator`   | `psub-orchestrator`   |
| `crates/supervisor`   | `crates/psub-supervisor`   | `supervisor`     | `psub-supervisor`     |
| `crates/wave`         | `crates/psub-wave`         | `wave`           | `psub-wave`           |

Crates **not** renamed (no collision): `substrate-{core,app,trace,schedule,dag,skills,memory,tui,serve-lock}`, `engine-*`, `transport-file`, `store-*`, `driver-*`, `runtime-process`, `phenotype-mcp`, `context-budget`, `dispatch-bridge`, `cloud-*`, `omniroute-adapter`, `routing-phenotype-router`, `arch-test`, `gateway-tools`, `wave-3lane-tests`, `cliproxy-adapter`, `tools/fake-*`.

## 3. Cargo.toml consumers

Update both the dependency declaration and the version pin. Workspace
version after rename is `0.3.0`.

```toml
# before
[dependencies]
substrate    = "0.1"
gateway      = "0.1"
a2a          = "0.1"
file-watcher = "0.1"
orchestrator = "0.1"
supervisor   = "0.1"
wave         = "0.1"

# after
[dependencies]
psub                = "0.1"
psub-gateway        = "0.1"
psub-a2a            = "0.1"
psub-file-watcher   = "0.1"
psub-orchestrator   = "0.1"
psub-supervisor     = "0.1"
psub-wave           = "0.1"
```

### `[patch.crates-io]`

Path patch (most common inside the substrate monorepo):

```toml
# before
[patch.crates-io]
gateway = { path = "../gateway" }

# after
[patch.crates-io]
psub-gateway = { path = "../psub-gateway" }
```

Git patch — change only the table key, and advance the tag to `v0.3.0`:

```toml
# before
[patch."https://github.com/KooshaPari/substrate"]
gateway = { git = "https://github.com/KooshaPari/substrate", tag = "v0.2.0" }

# after
[patch."https://github.com/KooshaPari/substrate"]
psub-gateway = { git = "https://github.com/KooshaPari/substrate", tag = "v0.3.0" }
```

Pre-rename tags still resolve the old names; use `v0.3.0` for the new
names.

## 4. Source consumers

The **package** name uses a hyphen (`psub-gateway`); the **module** path
uses an underscore (`psub_gateway`).

```rust
// before
use gateway::router::Router;
use a2a::task::{Task, TaskState};
use substrate::ports::DispatchPlanner;

// after
use psub_gateway::router::Router;
use psub_a2a::task::{Task, TaskState};
use psub::ports::DispatchPlanner;
```

### Bulk rewrite with `sed`

Run from the repo root (long-to-short ordering avoids double-substitution):

```bash
find . -type f -name '*.rs' \
  -not -path './target/*' -not -path './.claude/*' \
  -exec sed -i \
    -e 's/\buse gateway::/use psub_gateway::/g' \
    -e 's/\buse a2a::/use psub_a2a::/g' \
    -e 's/\buse file_watcher::/use psub_file_watcher::/g' \
    -e 's/\buse orchestrator::/use psub_orchestrator::/g' \
    -e 's/\buse supervisor::/use psub_supervisor::/g' \
    -e 's/\buse wave::/use psub_wave::/g' \
    -e 's/\buse substrate::/use psub::/g' \
    {} +
```

For pre-2018-edition code, also run:

```bash
find . -type f -name '*.rs' -not -path './target/*' \
  -exec sed -i 's/^extern crate gateway;/extern crate psub_gateway;/' {} +
```

Audit afterwards (should be empty apart from worktrees):

```bash
git grep -nE '\b(use|extern crate) (gateway|a2a|file_watcher|orchestrator|supervisor|wave|substrate)::' -- '*.rs'
```

## 5. Binary consumers

Three binaries; canonical names match package names, not the historical `substrate-*` ones:

| Old binary          | New binary     | Source crate                       |
| ------------------- | -------------- | ---------------------------------- |
| `substrate` (CLI)   | `psub`         | `crates/psub` (CLI via `cargo run -p psub`) |
| `substrate-gateway` | `psub-gateway` | `crates/psub-gateway`              |
| `substrate-http`    | `driver-http`  | `crates/driver-http` (unchanged)   |

systemd / Procfile fragment:

```ini
# before
ExecStart=/usr/local/bin/substrate-gateway

# after
ExecStart=/usr/local/bin/psub-gateway
```

> The `[[bin]]` target in `crates/psub-gateway/Cargo.toml` is still
> `substrate-gateway`; the Dockerfile overrides with `--bin psub-gateway`.
> Plain `cargo build` produces `target/{debug,release}/substrate-gateway`
> — rename post-build or pass `--bin psub-gateway`.

### Environment variables

Env-var names unchanged. Set read by `crates/psub-gateway` (`grep -rE 'env::var' src/`):

| `SUBSTRATE_GATEWAY_BIND`              | Listen address (default `127.0.0.1:20128`) |
| `SUBSTRATE_GATEWAY_AUTH_TOKEN`        | Bearer token for `/admin/*`          |
| `SUBSTRATE_STATE_DIR`                 | Root for sqlite stores (default `./.substrate`) |
| `SUBSTRATE_CONFIG_FILE`               | Path to `config.toml` (hot-reloaded) |
| `SUBSTRATE_AUDIT_LOG`                 | JSONL audit log path; unset disables |
| `SUBSTRATE_ADMIN_TOKEN`               | Token for admin endpoints            |
| `SUBSTRATE_MAX_TOKENS_PER_SESSION`    | Per-session token cap                |
| `SUBSTRATE_MAX_COST_USD_PER_SESSION`  | Per-session USD cap                  |
| `SUBSTRATE_RETRY_ATTEMPTS`            | Upstream retry count                 |
| `SUBSTRATE_RETRY_BASE_MS`             | Base backoff (full jitter)           |
| `PSUB_GATEWAY_BIND`                   | Docker-only override (`0.0.0.0:8080`) |
| `SUBSTRATE_HOME`                      | Docker runtime root (`/var/lib/substrate`) |

No `GATEWAY_*` env vars exist post-rename. Old `GATEWAY_BIND=...` style vars have no effect — migrate to the matching `SUBSTRATE_GATEWAY_*`.

## 6. Config file consumers

`psub-gateway` reads a single TOML file via `SUBSTRATE_CONFIG_FILE`. The
schema lives at `crates/psub-gateway/src/config.rs::GatewayConfig`. The
rename did **not** change any TOML keys, because keys were already
distinct from crate names (`[[providers]]`, `[fallback]`, `[metrics]`):

```toml
# keys unchanged before / after
[gateway]
bind      = "127.0.0.1:20128"
state_dir = "./.substrate"

[[providers]]
name        = "deepseek"
type        = "openai"
base_url    = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"

[fallback]
chain = ["deepseek", "kilocode"]
```

TOML files require no rewrite. Update only custom schema validators keyed off the crate name.

## 7. Docker consumers

The published image and entrypoint both changed:

| Old                                  | New                              |
| ------------------------------------ | -------------------------------- |
| Image: `substrate-gateway:latest`    | Image: `psub-gateway:0.3.0`      |
| Entrypoint: `substrate-gateway`      | Entrypoint: `psub-gateway`       |
| ENV: `SUBSTRATE_PORT=3000`           | ENV: `PSUB_GATEWAY_BIND=0.0.0.0:8080` |

OCI image builds from `Dockerfile` (not `Containerfile`, which still references the legacy `gateway` package — see rollback). Volume mounts:

```bash
# before
docker run -v ./config.toml:/etc/substrate/config.toml:ro \
           substrate-gateway:latest

# after
docker run \
  -v ./config.toml:/etc/substrate/config.toml:ro \
  -v substrate-data:/var/lib/substrate \
  -p 8080:8080 \
  psub-gateway:0.3.0
```

`/var/lib/substrate` holds SQLite stores under `.substrate/`. Mount as a named volume to persist across container recreations.

## 8. Rollback

`cb9a3e7` is a pure rename commit — every changed line is a path or identifier change — so `git revert` produces a clean mirror restoring the old `gateway` / `a2a` / etc. package names:

```bash
git revert --no-edit cb9a3e7
cargo build --workspace
```

Conflicts only appear if you have unmerged work touching the same lines; resolve with `git revert --abort` and cherry-pick manually.

For a partial rollback (restore one crate, keep the rest), it's faster to re-apply the original commit against current `main` than to surgically revert one crate.

## 9. Timeline

| Date (UTC)              | Commit    | Event                                                         |
| ----------------------- | --------- | ------------------------------------------------------------- |
| 2026-07-08 10:28        | `cb9a3e7` | Rename committed. 7 crates → `psub-` prefix.                  |
| 2026-07-08 10:30+       | follow-up | Workspace version bumped to `0.3.0` for first post-rename publish. |
| 2026-07-15 (planned)    | release   | First crates.io publish under `psub-*` names. Registry policy forbids later claiming the old names. |
| 2026-10-15 (planned)    | cleanup   | Stale references in `Containerfile`, `compose/substrate-gateway.json`, and `README.md` removed. |

Until 2026-10-15, three auxiliary files still mention old names (doc-only; no impact on the v0.3.0 publish):

- `Containerfile` line 5: `cargo build --release --locked -p gateway` → `-p psub-gateway`
- `compose/substrate-gateway.json`: `run_command: "cargo run -p gateway"` → `cargo run -p psub-gateway`
- `README.md`: `cargo run -p gateway` quick-start command

These will be cleaned up after one quarter of stable production use.