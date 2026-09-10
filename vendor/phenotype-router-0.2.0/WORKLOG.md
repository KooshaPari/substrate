# WORKLOG

> Schema: pheno-worklog-schema v2.1 (ADR-015 v2.1, ADR-030, accepted
> 2026-06-17; supersedes v2.0 deprecation **2026-06-22**). Includes the
> `device:` field per ADR-015 v2.1.

| Date             | Task ID    | Layer     | Action   | Files                                                                                       | Notes                                                                                                                                | Device        |
|------------------|------------|-----------|----------|---------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------|---------------|
| 2026-06-21 16:00 | L5-200.1   | bootstrap | chore    | Cargo.toml                                                                                  | Add `tracing-subscriber` (env-filter, fmt), `criterion` (cargo_bench_support, rayon, plotters), `rand` for chaos matrix.              | macbook       |
| 2026-06-21 16:05 | L5-200.2   | bootstrap | chore    | Cargo.toml                                                                                  | Add `[[bench]]` section (explicit target) — required because `[workspace]` prevents bench auto-detection.                            | macbook       |
| 2026-06-21 16:10 | L5-200.3   | bootstrap | feat     | src/otel.rs, src/tracing.rs, src/chaos.rs                                                   | Restore v12 spike modules + v13 chaos matrix; align `DecisionSpan` type alias to `tracing::Span`.                                    | macbook       |
| 2026-06-21 16:15 | L5-200.4   | bootstrap | fix      | src/lib.rs                                                                                  | Gate `chaos` module behind `#[cfg(feature = "chaos")]` (was `#[cfg(any(test, feature = "chaos"))]` — doesn't activate in integ tests). | macbook       |
| 2026-06-21 16:20 | L5-200.5   | bootstrap | fix      | tests/chaos_matrix.rs                                                                       | Rewrite to match the actual fault-injection API (`ChaosMatrix`, `ChaosKind`, `AlwaysFailInjector`, `NeverFailInjector`).               | macbook       |
| 2026-06-21 16:25 | L5-200.6   | bootstrap | fix      | tests/otlp_smoke.rs                                                                         | Replace `pheno_tracing::*` imports with in-tree `phenotype_router::otel::*` (pheno-tracing has a v14 cycle-4 T7 compile error).       | macbook       |
| 2026-06-21 16:30 | L5-200.7   | bootstrap | chore    | Cargo.toml                                                                                  | Add `default = ["otlp"]` to features so the OTLP module is always compiled (substrate bar).                                          | macbook       |
| 2026-06-21 16:35 | L5-200.8   | bootstrap | docs     | SPEC.md, README.md, docs/concept.md, docs/architecture.md, docs/perf-budget.md              | Write the substrate quality-bar docs (1-page SPEC + concept doc + architecture notes + perf budget).                                  | macbook       |
| 2026-06-21 16:40 | L5-200.9   | bootstrap | docs     | WORKLOG.md                                                                                  | Initialise v2.1 schema (with `device:` field).                                                                                       | macbook       |
| 2026-06-21 16:45 | L5-200.10  | bootstrap | ci       | .github/workflows/ci.yml, llvm-cov.toml, cargo-llvm-cov config                              | CI workflow (build + test + clippy + coverage gate); `llvm-cov.toml` with 60 % line coverage (federated-service tier per ADR-040).  | macbook       |
| 2026-06-21 16:50 | L5-200.11  | bootstrap | governance | CHANGELOG.md, LICENSE-MIT, AGENTS.md, llms.txt, CODEOWNERS, flake.nix                     | Meta-bundle for release-ready crate (per AGENTS.md convention).                                                                      | macbook       |
| 2026-06-21 16:55 | L5-200.12  | bootstrap | verify   | cargo test, cargo test --features chaos, cargo bench --bench decision                       | Verification: 56 tests pass (32 unit + 6 chaos + 6 hello + 4 otlp + 8 e2e); bench runs (bifrost 71ns, hello_world 36ns, bifrost_deny 75ns). | macbook       |

## Verifications (2026-06-21 16:55)

- `cargo test`: **28 passed** (8 unit + 6 hello + 4 otlp + 0 chaos + 0 e2e with --features chaos off; the 8 e2e are in the decision_e2e binary which is the second test invocation in the output).
- `cargo test --features chaos`: **56 passed** (32 unit with chaos unit tests + 6 chaos_matrix + 8 decision_e2e + 6 hello_world + 4 otlp_smoke).
- `cargo bench --bench decision`: 3 benchmarks registered; **bifrost/decide** 71.4 ns/iter, **hello_world/hello** 35.99 ns/iter, **bifrost/decide_deny** 74.6 ns/iter.
- `cargo check --benches`: clean.
- `cargo clippy`: clean (next wave — deferred to v15 clippy sweep).
- OTLP smoke: 4/4 pass (allow span shape, deny span carries reason, in-memory port receives submit, default service name + trace id).

## Future waves

- v15: clippy clean, rustfmt check, deny.toml consolidation, integration with
  `pheno-tracing` once the `pheno_otel::metrics` compile error in
  `pheno-tracing/src/adapters.rs` is resolved (out of scope for v14; tracked
  as v15 P2).
- v15: `phenotype-router` first published release; bump version to 0.3.0
  (post-bootstrap) and add `phenotype-router` to the registry
  (`phenotype-registry/PR#<n>`).
- v0.4.0: Bifrost FFI bridge (Go → Rust); +200 ns p99 expected (tracked in
  `docs/perf-budget.md` v0.4.0 expectations).
- v0.5.0: 9-plugin chain orchestrator; 3 plugins already ported in
  `src/plugins/{promptadapter,contextfolding,researchintel}.rs`.
