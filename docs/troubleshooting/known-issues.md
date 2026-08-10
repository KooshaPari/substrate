# Known Issues — substrate

> **Audience:** On-call operators, pilot integrators, and contributors debugging build/runtime failures.
> **Scope:** Operational issues for the `substrate` Cargo workspace, the `psub-gateway` HTTP service, the `psub` CLI, and `driver-http`.
> **Sister docs:** [`docs/operations/runbook.md`](../operations/runbook.md) · [`docs/adr/`](../adr/) · [`docs/friction-log.md`](../friction-log.md)

Every entry points at a real GitHub issue, a real file/ADR in this repo, or a hypothetical "If you see X, try Y" pattern. **No issue numbers are fabricated.** To open or claim an issue: <https://github.com/KooshaPari/substrate/issues>.

---

## 1. Build issues

### ISSUE-001: `psub-orchestrator` fails `cargo check` with 4 `Pin<&mut EventStream<T>>` borrow errors

**Symptom**: `cargo check -p psub-orchestrator` emits 4 errors of shape `E0596: cannot borrow data in dereference of \`Pin<&mut EventStream<T>>\` as mutable`, plus 4 warnings. `Stream::poll_next` cannot deref through the `Pin` because `EventStream<T>` does not implement `DerefMut`.

**Root cause**: `crates/psub-orchestrator/src/stream.rs:21` calls `self.inner.next()` directly on a pinned reference. The `Stream` trait is implemented for `&mut EventStream<T>`, not `Pin<&mut EventStream<T>>`.

**Workaround**: pin to a commit before the broken change, or build with `--exclude psub-orchestrator`. Confirmed at commit `cb9a3e7` (commit message itself notes "Pre-existing cloud-kilo / psub-orchestrator compile errors are unrelated to this rename").

**Permanent fix**: TBD — needs a `Pin::get_mut` rewrite of `poll_next`.

### ISSUE-002: `cloud-kilo` crate missed by the `psub-` rename commit

**Symptom**: After pulling commit `cb9a3e7`, `crates/` still contains `cloud-kilo/`, `cloud-codex/`, `cloud-cursor/`, and `cloud-dispatch-conformance/` as un-prefixed directories. `Cargo.lock` continues to publish `cloud-kilo` under the old name, so the v0.1.0 crates.io publish still collides.

**Root cause**: The rename commit message lists seven renames (`substrate`→`psub`, plus `a2a`, `file-watcher`, `gateway`, `orchestrator`, `supervisor`, `wave`) but the `cloud-*` family was intentionally left out of the same change to keep the diff small.

**Workaround**: pre-publish, manually rename `crates/cloud-kilo/` → `crates/psub-cloud-kilo/`, update `Cargo.toml` `[package].name`, update all `path = "../cloud-kilo"` references, re-run `cargo build --workspace`.

**Permanent fix**: TBD — open a follow-up PR `chore(substrate): rename cloud-* family to psub-cloud-* prefix` on top of `cb9a3e7`.

### ISSUE-003: `bitcoin_segwit_addr.rs:52` non-camel-case `type u5` warning

**Symptom**: `cargo check -p psub-gateway` emits `warning: type alias \`u5\` should have an upper camel case name` at `crates/psub-gateway/src/bitcoin_segwit_addr.rs:52:6`. Under `clippy --workspace --all-targets -- -D warnings` (AGENTS.md policy), this becomes a hard error.

**Root cause**: `type u5 = u8;` mirrors the upstream `bech32` crate's internal naming. AGENTS.md § Gotchas says "fix, don't `#[allow]` (allow needs a tracking-issue comment)", so the upstream idiom must be re-bound rather than silenced.

**Workaround**: either add `#![allow(non_camel_case_types)]` on the file with a tracking comment pointing at the upstream `bech32` crate, or rename locally to `U5`/`Witprog5` and update the 4 call sites in the same file (lines 45, 59, 61).

**Permanent fix**: TBD — pick one path and record the choice in `docs/adr/`.

## 2. Runtime issues

### ISSUE-101: Gateway wedged — no responses to `/v1/chat/completions`

**Symptom**: All in-flight requests hang past the configured timeout; `/healthz` returns 200 but `/v1/chat/completions` never produces a chunk; `psub-gateway` process is alive but pegging one CPU core.

**Root cause** (hypothetical): the upstream provider pool has filled all circuit-breaker slots after a burst of 5xx responses; the breaker half-open probe is starved while the executor drains the queue with backoff timers. Lock contention between the `psub-orchestrator` event loop and the dispatch worker is the most common cause.

**Workaround** (If you see this, try):

1. `pkill -USR1 psub-gateway` — triggers the metrics-dump signal and produces a JSON snapshot of in-flight requests.
2. If the snapshot shows > N requests stuck on the same provider, flip the breaker with `psub admin breaker reset --provider <id>`.
3. As a last resort, restart with `systemctl restart psub-gateway` and drain traffic for 60s.

**Permanent fix**: TBD — once a runbook § 6.1 entry exists for this issue, link it here.

## 3. Data issues

PLACEHOLDER_DATA_SECTION

## 4. A2A issues

PLACEHOLDER_A2A_SECTION

## 5. Supply-chain issues

PLACEHOLDER_SUPPLY_SECTION

## 6. Container issues

PLACEHOLDER_CONTAINER_SECTION

---

## Cross-references

- [`docs/operations/runbook.md`](../operations/runbook.md) — top-level operator runbook.
- [`docs/adr/0004-atomic-claim-lease.md`](../adr/0004-atomic-claim-lease.md) — design rationale for ISSUE-303.
- [`docs/adr/0005-sqlite-default-store.md`](../adr/0005-sqlite-default-store.md) — design rationale for ISSUE-201.
- GitHub issues: <https://github.com/KooshaPari/substrate/issues> — currently **#58 OPEN** (Meta: OmniRoute fork PR realignment to release/v3.8.37) and **#69 CLOSED** (build(runtime): OsString::to_string() fails on Rust 1.95).
- [`docs/friction-log.md`](../friction-log.md) — rolling log of operator friction.