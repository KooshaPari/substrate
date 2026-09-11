# Changelog

All notable changes to `phenotype-router` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `SPEC.md` — 1-page substrate specification (ADR-023 Rule 3.1).
- `README.md` — public-facing quickstart.
- `docs/concept.md` — concept doc (what / when / when-not / 3-layer model).
- `docs/architecture.md` — extended architecture notes (crate layout, module graph, OTel bridge, SDK contract).
- `docs/perf-budget.md` — per-call latency budget table.
- `WORKLOG.md` — worklog v2.1 schema with `device:` field (ADR-015 v2.1).
- `CHANGELOG.md` — this file.
- `LICENSE-MIT` — MIT license.
- `AGENTS.md` — AI-agent context.
- `llms.txt` — LLM-facing context (per AGENTS.md convention).
- `CODEOWNERS` — @KooshaPari owns all paths.
- `flake.nix` — nix dev shell (ADR-039).
- `.github/workflows/ci.yml` — CI gate (build + test + chaos + bench + clippy).
- `llvm-cov.toml` — 60 % line coverage gate (federated-service tier, ADR-040).
- `tests/chaos_matrix.rs` — 6 integration tests for ADR-052 §4 fault-injection framework.
- `tests/otlp_smoke.rs` — 4 integration tests for the OTLP bridge (ADR-012 / ADR-036B).
- `benches/decision.rs` — 3 criterion benchmarks (bifrost/decide, hello_world/hello, bifrost/decide_deny).
- `src/otel.rs` — `OtlpDecisionRecorder`, `TracePort`, `InMemoryTracePort` (ADR-052 §3).
- `src/tracing.rs` — `tracing` façade + `DECISION_SPAN_NAME` constant + `DecisionSpan` type alias.
- `src/chaos.rs` — `ChaosMatrix`, `ChaosKind` (5 categories), `ChaosInjector` trait, `AlwaysFailInjector`, `NeverFailInjector` (ADR-052 §4).
- `src/sdk.rs` — `LlmPort`, `DecisionPlugin`, `ConnectorPort` SDK contracts (ADR-052 §1).
- `src/plugins/{promptadapter,contextfolding,researchintel}.rs` — v13 3-plugin port wave (experimental).

### Changed
- Bumped version `0.1.0` → `0.2.0` (bootstrap + substrate quality bar).
- `Cargo.toml` features: `default = ["otlp"]`; explicit `[[bench]]` target (auto-detection disabled by `[workspace]`).
- `src/lib.rs` re-exports: `Decision`, `Request`, `Response`, `DecisionError`, `DecisionLayer`, `BifrostAdapter`, `HelloWorld`, `HelloWorldPort`, `OtelConfig`, `OtlpDecisionRecorder`, `DecisionSpan`, `LlmPort`, `DecisionPlugin`, `ConnectorPort`, `Capabilities`, `Phase`, `HealthStatus`, `PluginDecision`, `PluginError`, `LlmRequest`, `LlmResponse`, `LlmError`, `ConnectorConfig`, `ConnectorError`, `ConnectorHandle`.
- `src/decision.rs` — added `health()` method to `DecisionLayer` (hexagonal L4 contract, ADR-038).

### Fixed
- `chaos` module gating: was `#[cfg(any(test, feature = "chaos"))]` (didn't activate in integration tests); now `#[cfg(feature = "chaos")]` with a `#![cfg(feature = "chaos")]` guard on `tests/chaos_matrix.rs`.
- `tests/otlp_smoke.rs` — replaced `pheno_tracing::*` imports with in-tree `phenotype_router::otel::*` (the `pheno-tracing` crate has a pre-existing v14 cycle-4 T7 compile error referencing a non-existent `pheno_otel::metrics` module; out of scope for this bootstrap, tracked as v15 P2 follow-up).

## [0.1.0] - 2026-06-20

### Added
- Initial v12 spike: `BifrostAdapter` stub, `HelloWorld` fixture, `Decision` enum, `Request`/`Response` types, `DecisionLayer` port trait.
- `tests/hello_world.rs` — 6 integration tests.

[Unreleased]: https://github.com/KooshaPari/phenotype-router/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/KooshaPari/phenotype-router/releases/tag/v0.1.0
