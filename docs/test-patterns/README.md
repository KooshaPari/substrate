# Test Patterns (TEST-03, TEST-07, TEST-09)

These files document how to add property-based tests (proptest), snapshot
tests (insta), and load tests (criterion) to substrate crates. They are
**reference documentation**, not compiled into the workspace by default.

## Why are these here and not in `src/` or `tests/`?

The substrate workspace contains 41 crates, each with bespoke types and
test-helper macros. Generic "smoke" tests that import from `substrate_core::*`
require downstream types that change frequently. Rather than create a test
that breaks every time a refactor lands, these live as reference patterns:

- **Copy a pattern into your crate's `tests/` directory**
- **Adapt the import paths to your crate's public API**
- **Run `cargo test -p <your-crate> proptest_smoke`** to verify

## What's here

### `proptest_smoke.rs` — Property-based testing (TEST-03)
Demonstrates the pattern for `proptest!` macros:
- Generating random inputs (u32, String, Vec<u8>, HashMap)
- Defining invariants via `prop_assert!`, `prop_assert_eq!`
- Configuring the runner with `ProptestConfig::default()`
- Composing strategies for nested types

To enable: add `proptest = "1"` to `[dev-dependencies]` of your crate's
`Cargo.toml`, then copy the body of this file (minus the header comment)
into `crates/<your-crate>/tests/proptest_smoke.rs`.

### `snapshots_smoke.rs` — Snapshot testing (TEST-07)
Demonstrates the pattern for `insta::assert_json_snapshot!` and
`insta::assert_debug_snapshot!`:
- Capturing structured outputs for golden-file review
- Inline vs external snapshot files
- CI-friendly failure reporting (`INSTA_UPDATE=auto` for auto-accepting)

To enable: add `insta = "1"` to `[dev-dependencies]`, then `cargo install
cargo-insta` for the auto-accept workflow. Snapshots land in
`crates/<your-crate>/tests/snapshots/`.

### `criterion_bench.rs` — Load / soak benchmarks (TEST-09)
Already exists in the workspace at `/benches/criterion_bench.rs` but the
`[[bench]]` target in the root `Cargo.toml` is not wired. Wiring requires:

```toml
[[bench]]
name = "criterion_bench"
harness = false
```

Run with `cargo bench` (local) or invoked via GitHub Actions on a
nightly schedule (RE-11 post-deploy smoke + a separate nightly
bench-runner workflow).

## Scorecard pillars

- **TEST-03 (proptest infrastructure)**: Pattern is here. Per-crate adoption is 1 day of work.
- **TEST-07 (insta snapshots)**: Pattern is here. Per-crate adoption is 4 hr of work.
- **TEST-09 (load / soak test)**: Bench file exists; `[[bench]]` target wiring is 4 hr of work.

These are flagged as `partial` in `audit_scorecard.json` until a downstream
crate actually adopts the patterns.
