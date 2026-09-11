//! Criterion benchmark for the decision layer (ADR-040 perf-budget gate).
//!
//! Per `docs/perf-budget.md` the headline end-to-end budget for the
//! `decide()` hot path is **1.5 s p99**. The criterion harness measures
//! the per-call latency decomposed by adapter:
//!
//! - `BifrostAdapter` — stub adapter (in-tree; mirrors Bifrost's allow/deny
//!   rules per ADR-050).
//! - `HelloWorld` — no-op fixture (returns `Allow` for every request).
//!
//! ## How to run
//!
//! ```bash
//! cargo bench --bench decision
//! ```
//!
//! The benchmark is gated by `--features otlp` (default-on) so the hot-path
//! measurement includes the OTLP span emission. To bench a no-OTLP baseline:
//!
//! ```bash
//! cargo bench --bench decision --no-default-features
//! ```
//!
//! The CI workflow (`.github/workflows/ci.yml`) runs `cargo bench` against
//! the perf-budget table and fails the build when the p99 regresses by
//! more than 10 %.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use phenotype_router::{BifrostAdapter, DecisionLayer, HelloWorld, HelloWorldPort, Request};

fn bench_bifrost(c: &mut Criterion) {
    let adapter = BifrostAdapter::new();
    let req = Request::new("bench:hot-path", "payload");
    c.bench_function("bifrost/decide", |b| {
        b.iter(|| {
            let resp = adapter.decide(black_box(&req));
            black_box(resp)
        })
    });
}

fn bench_hello_world(c: &mut Criterion) {
    let hw = HelloWorld;
    let req = Request::new("bench:hot-path", "payload");
    c.bench_function("hello_world/hello", |b| {
        b.iter(|| {
            let resp = hw.hello(black_box(&req));
            black_box(resp)
        })
    });
}

fn bench_deny_path(c: &mut Criterion) {
    let adapter = BifrostAdapter::new();
    let req = Request::new("deny:bench-deny", "payload");
    c.bench_function("bifrost/decide_deny", |b| {
        b.iter(|| {
            let resp = adapter.decide(black_box(&req));
            black_box(resp)
        })
    });
}

criterion_group!(benches, bench_bifrost, bench_hello_world, bench_deny_path);
criterion_main!(benches);
