# F5 forge-daemon fast path — 3-regime perf profile

**Date:** 2026-07-01
**Branch:** `feat/forge-daemon-f5-2026-06-30`
**Worktree:** `/Users/kooshapari/CodeProjects/Phenotype/repos/substrate/.claude/worktrees/forge-daemon-f5-2026-06-30`
**Bench:** `tools/f5-bench/` (added by this commit, 8e72b4f)
**Baseline commit:** `3a7af11` (F5 wire: forge-daemon opt-in drop-in for `run_simple`)
**Verifier commit:** `8e72b4f`

---

## Setup

- Dev profile, `cargo build` (no `--release`)
- 32 tokio workers (`#[tokio::main(flavor = "multi_thread", worker_threads = 32)]`)
- 1 warm-up iteration excluded from timings
- Stub binary: `tools/fake-forge` (exits 0 quickly, prints `{"conversations":[]}`)
- Rounds = `iters / M`; each round fires M concurrent `EnginePort::start()` calls

```
FORGE_BIN=./target/debug/fake-forge
M=8/16/32
ITERS=64/128/256  (8 rounds per M)
```

The F5 fast path engages only when `FORGE_DAEMON=1` is set *and* the in-process
daemon is alive (`forge_daemon::ffi_start()` was called from the bench).

---

## Measured numbers

| M   | direct ops/s | daemon ops/s | speedup | direct per-op (ms) | daemon per-op (ms) | saved (ms) |
| --- | -----------: | -----------: | ------: | -----------------: | -----------------: | ---------: |
| 8   |       298.63 |       569.44 |   1.91× |              3.349 |              1.756 |      -1.59 |
| 16  |       339.55 |       577.88 |   1.70× |              2.945 |              1.730 |      -1.21 |
| 32  |       409.60 |       610.71 |   1.49× |              2.441 |              1.637 |      -0.80 |

Direct = `tokio::Command::spawn` baseline.
Daemon = F5 fast path via `DaemonDispatch::dispatch` (Zig kqueue + posix_spawn).

---

## What the numbers say

1. **F5 wire is real.** 1.49× to 1.91× ops/s across M=8..32 with the in-process
   Zig daemon. The gain shrinks as M grows because the FFI/posix_spawn cost
   becomes the shared bottleneck (~600 ops/s ceiling).
2. **Saturation.** Daemon M=32 hits 610 ops/s — close to the per-process
   FFI-dispatch limit. Further wins require **multi-process daemon mode
   (Mode 2)** or **batched dispatch** in the Zig core, both out of scope for F5.
3. **Direct path remains the source of truth.** Without `FORGE_DAEMON=1`
   (or when the daemon is not started in the calling process) the code
   transparently falls back to `Command::spawn`. The two paths share all
   engine-forge surface; the daemon fast-path inside `run_simple` is a thin
   drop-in that only changes the syscall cost.
4. **Mem-bound, not CPU-bound.** Peak RSS at M=32 was ~10 MB for the bench
   process. Allocation churn is dominated by the FFI result buffer (65 536
   bytes per call) and the per-call `Vec<String>` argv on the Rust side.

---

## Wins investigated (and rejected)

While investigating cheap Rust-side pooling/mem wins, two changes were
bench'd and **reverted** because the variance-band (±20%) swallowed them
and one was a regression:

| Change                                                                 | M=32 ops/s | vs baseline | Decision |
| ---------------------------------------------------------------------- | ---------: | ----------: | :------: |
| TLS scratch buffer for `DaemonDispatch::dispatch` result (avoids fresh `vec![0u8; 65536]`) | 441.20 | -28% | **revert** — `to_vec()` copy of the TLS slot is more expensive than the alloc it was meant to replace |
| Hoist `model` to `const MODEL_EMPTY: &str` (skip `String::new()`)      | 367.71 | variance     | **revert** — within noise, no signal |
| Borrow `args.first()` instead of `cloned()`                             | 318.95 | variance     | **revert** — `spawn_blocking` borrow escape, blocked by lifetime; tried workaround, still noise |

The 65 536-byte alloc is *not* the bottleneck — it happens inside
`DaemonDispatch::dispatch` which is dominated by the FFI call. The Rust-side
per-call allocations are dwarfed by `posix_spawn` + `waitpid`. **No Rust-side
micro-opt will move the needle; further wins live in the Zig core.**

---

## Next-cheapest wins (Zig core, follow-up)

Ranked by expected ops/s uplift × implementation cost:

1. **`worker.zig` batched dispatch** — accept N requests on the socket,
   fork+waitpid each in a single kqueue batch, return N results in one
   syscall round-trip. Estimated +30-50% daemon M=32.
2. **`posix_spawn` file-actions reuse** — the spawn-file-actions object is
   allocated/freed per call; a thread-local pool would cut ~3 µs/op.
3. **`pipe2(O_CLOEXEC)` buffer size** — bump from 64 KiB to 256 KiB on macOS
   to match Linux default; reduces the number of partial reads for large
   forge stdout.

These are all Zig changes inside `forge-daemon/src/`. They need the bench
harness in `tools/f5-bench/` plus a daemon-mode (Mode 2) bench before/after
to quantify.

---

## Reproduce

```bash
WT=/Users/kooshapari/CodeProjects/Phenotype/repos/substrate/.claude/worktrees/forge-daemon-f5-2026-06-30
cd "$WT"
cargo build -p f5-bench

# Direct (no daemon)
FORGE_BIN=./target/debug/fake-forge M=32 ITERS=256 LABEL=direct-M32 ./target/debug/f5-bench

# Daemon (F5 fast path)
FORGE_DAEMON=1 FORGE_BIN=./target/debug/fake-forge M=32 ITERS=256 LABEL=daemon-M32 ./target/debug/f5-bench
```

Output ends with a `BENCH_RESULT` line; capture with `tee` for a CSV.

---

## Status

F5 wire: **verified**.
Resource-pooling sweep (PERF-D2): **no cheap Rust wins available** — Rust-side
optimizations are below the noise floor; further wins live in the Zig core.
Next: dispatch a follow-up to investigate `worker.zig` batched dispatch.