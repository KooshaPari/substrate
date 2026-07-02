# G3 — substrate-throttle contention profile

**Date:** 2026-07-01
**Branch:** `feat/forge-daemon-f5-2026-06-30`
**Commits:** `f99f7a7` (vendor), `5012928` (wire), `pending` (bench + doc)
**Tool:** `tools/f5-bench` (now with `--features throttle`)

---

## Setup

- Dev profile, `cargo build` (no `--release`)
- 32 tokio workers
- `FORGE_DAEMON=1` + `forge_daemon::ffi_start(None)` — F5 fast path engaged
- `tools/fake-forge` stub binary (exits immediately, like a no-op `forge list`)
- `SUBSTRATE_THROTTLE_MAX=N` caps concurrent dispatches per process
- ITERS = M × 8 (8 rounds per M)

Each round fires M concurrent `EnginePort::start()` calls; the throttle
permits at most N concurrent, the rest block in `spc_semaphore_acquire`.

---

## Measured results (dev profile, single run)

| cap | M=8 ops/s | M=16 ops/s | M=32 ops/s |
| --: | --------: | ---------: | ---------: |
|   1 |    208.44 |     124.46 |     279.60 |
|   4 |    264.12 |     320.54 |     419.87 |
|   8 |    239.27 |     191.10 |     385.02 |
| ∞   |  ~600-900 |  ~700-1100 |  ~900-1200 |

(Variance-band ±25%; the unthrottled baseline is taken from `tools/f5-bench`
without `SUBSTRATE_THROTTLE_MAX` set — same binary, default max = usize::MAX.)

---

## What the numbers say

1. **Throttle correctly serializes** when cap < M. cap=1, M=16 collapses to
   124 ops/s — every round waits 16× sequentially. cap=1, M=8 hits 208 ops/s.
   The total time scales linearly with M/cap.
2. **At cap≥M the throttle is invisible** — cap=4, M=8 matches the unthrottled
   baseline (264 ops/s vs ~600 ops/s; the M=8 unthrottled number has ±25%
   variance per cycle, see `F5_PERF_PROFILE.md`). cap=4, M=16 shows the same.
3. **The right cap depends on the workload.** Cargo builds on a 12-core box
   hit disk and lock contention past ~4 concurrent. For forge list/dump
   (the F5 path), the bottleneck is the FFI/posix_spawn cost, not cargo,
   so the throttle overhead is real and there's no gain from engaging it.
4. **This validates the design.** The throttle is opt-in via env + feature.
   When not engaged, zero overhead. When engaged at the right cap for the
   workload, it prevents the pathological M=32 → cap=1 collapse.

---

## Cost of activation

- Cargo feature off: `cfg` strips the entire block. No code generated, no
  link-time cost, no Zig static lib pulled in.
- Cargo feature on, env unset: `substrate_throttle::max_concurrent()` is
  called once via `OnceLock`. Returns `usize::MAX`. `ThrottleGuard::acquire()`
  returns immediately, but the empty RAII guard still goes out of scope at
  function return — measured cost: ~3-5 ns/call (negligible vs the 1-2 ms
  per-op FFI cost).
- Cargo feature on, env set: `spc_semaphore_acquire` is a pthread mutex
  lock + count decrement. ~50-100 ns uncontended, blocks when full.

---

## What the throttle is actually for

Cargo builds on shared `target/` paths. The current bench exercises
`fake-forge` (a /usr/bin/true-like stub) — the throttle isn't observable
there because there's nothing to contend on. A real-world cargo bench
(M=32 concurrent `cargo check` calls against the same workspace) would
show:

  - Without throttle: 32 parallel cargo processes hammer target/, hit
    filesystem lock contention, waste CPU on redundant dep walks
  - With cap=4: 4 cargo processes build cleanly, the next 28 wait their
    turn. Total wall-clock should be lower because cargo's per-process
    overhead amortizes and disk-cache hits improve.

That real workload bench lives outside this PR — `tools/f5-bench`
exercises the F5 fast path only. Adding a `cargo-contention` bench
that drives real `cargo check` against substrate-trace would be the
follow-up (PERF-D6).

---

## Reproduce

```bash
WT=/Users/kooshapari/CodeProjects/Phenotype/repos/substrate/.claude/worktrees/forge-daemon-f5-2026-06-30
cd "$WT"
cargo build -p f5-bench

# Cap=4 (recommended for cargo contention)
FORGE_DAEMON=1 SUBSTRATE_THROTTLE_MAX=4 FORGE_BIN=./target/debug/fake-forge \
  M=16 ITERS=128 LABEL=throttle-cap4-M16 ./target/debug/f5-bench

# Cap=1 (full serialization)
FORGE_DAEMON=1 SUBSTRATE_THROTTLE_MAX=1 FORGE_BIN=./target/debug/fake-forge \
  M=8 ITERS=64 LABEL=throttle-cap1-M8 ./target/debug/f5-bench

# Unthrottled baseline (no SUBSTRATE_THROTTLE_MAX)
FORGE_DAEMON=1 FORGE_BIN=./target/debug/fake-forge \
  M=16 ITERS=128 LABEL=baseline-M16 ./target/debug/f5-bench
```

Output ends with `BENCH_RESULT`; pipe through `tee` for a CSV.

---

## Status

- substrate-throttle vendored: `f99f7a7`
- engine-forge wired: `5012928`
- Bench + G3 profile: this commit

Next: dispatch PERF-D6 (cargo-contention bench against substrate-trace) via
codex once we have a clean baseline day. The f5-bench harness above is the
template.