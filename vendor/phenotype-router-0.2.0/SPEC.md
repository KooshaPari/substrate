# `phenotype-router` — Specification (v0.3.0)

> **Substrate role:** Phenotype-owned router **decision layer** (ADR-050 / ADR-051,
> §8 router-architecture ACCEPTED 2026-06-20). Hexagonal L4 Port/Adapter
> primitive (ADR-038) with OTel-native OTLP span emission (ADR-012 / ADR-036B).

## 1. Purpose

Provide the **decision layer** for the Phenotype LLM-routing architecture. The
decision layer receives a `Request` and returns a `Response` whose `Decision`
is one of `Allow | Defer | Deny(reason)`. Every adapter in the fleet
(`BifrostAdapter`, future `SmartFallback`, future plugin-chain adapters) plugs
into this layer via the [`DecisionLayer`](src/decision.rs) port trait.

## 2. Substrate placement (ADR-023 Rule 3)

`phenotype-router` is a `pheno-*-lib` — pure reusable Rust library, single
concern (decision layer), single crate. It is the **canonical substrate** for
all LLM-routing decisions in the Phenotype fleet.

## 3. Public API

```rust
// Decision port contract (ADR-050 / ADR-038)
pub trait DecisionLayer: Send + Sync {
    fn name(&self) -> &str;
    fn adapter_kind(&self) -> &str { self.name() }
    fn health(&self) -> Result<(), DecisionError> { Ok(()) }
    fn decide(&self, req: &Request) -> Response;
}

// In-tree adapters
pub struct BifrostAdapter;     // stub mirroring Bifrost's allow/deny rules
pub struct HelloWorld;         // no-op fixture, returns Allow

// OTel substrate (ADR-012 / ADR-036B / ADR-052 §3)
pub struct OtlpDecisionRecorder { /* ... */ }
pub struct TraceOperation { /* name, kind, attributes */ }
pub trait TracePort: Send + Sync { fn submit(&self, op: TraceOperation) -> TraceResult; }
pub struct InMemoryTracePort;   // for tests

// Chaos matrix (ADR-052 §4)
pub struct ChaosMatrix { /* 5 fault kinds */ }
pub enum ChaosKind { Timeout, Malformed, Unreachable, Transient, Overload }
pub trait ChaosInjector { fn inject(&self, s: &ChaosScenario) -> FaultOutcome; }
```

## 4. Feature flags

| Flag     | Default | Purpose                                          |
|----------|:-------:|--------------------------------------------------|
| `otlp`   |   ON    | Enables the OTLP bridge (ADR-012).               |
| `bifrost`|  OFF    | Stub; future FFI bridge (ADR-050).               |
| `chaos`  |  OFF    | Pulls in `rand`; gates the chaos matrix.         |

## 5. Quality bar (ADR-023 Rule 3.1)

- **Spec:** this file (1 page).
- **Docs:** `README.md` + `docs/concept.md` (concept doc).
- **Tests:** 32 unit + 6 chaos + 6 hello-world + 4 OTLP smoke + 8 e2e = 56 total.
- **Observability:** `OtlpDecisionRecorder` ships spans via the in-tree `TracePort`.
- **Coverage gate:** 60 % (federated-service tier per ADR-040); enforced via CI.
- **CI gate:** `.github/workflows/ci.yml` runs `cargo build` + `cargo test` +
  `cargo test --features chaos` + `cargo bench --no-run` + `cargo clippy`.
- **Worklog:** `WORKLOG.md` (v2.1 schema, includes `device:` field).

## 6. Roadmap

- **v0.3.0 (this release):** v12 spike, hexagonal contract, OTel bridge,
  chaos matrix, criterion bench.
- **v0.4.0:** Bifrost FFI bridge (Go-side `phenotype-gateway/packages/bifrost`
  → Rust `BifrostAdapter`).
- **v0.5.0:** 9-plugin chain (ADR-052) — `intelligentrouter`, `smartfallback`,
  `learning`, `promptadapter`, `contextfolding`, `voyage`, `researchintel`,
  `contentsafety`, `toolrouter`.
- **v1.0.0:** Stable substrate API; `phenotype-gateway` consumer wired.

## 7. Non-goals

- **Not a transport.** Bifrost owns transport (ADR-051); the decision layer
  is one-way (calls into Bifrost, never the reverse).
- **Not a plugin runtime.** The 9-plugin chain is v0.5.0; v0.3.0 ships the
  decision-layer contract that plugins will plug into.
- **Not an OTel SDK.** The recorder produces OTel-compatible `TraceOperation`
  values; consumers wire the SDK of their choice.
