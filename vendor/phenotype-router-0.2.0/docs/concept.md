# Concept: The Decision Layer

> Companion to [`SPEC.md`](../SPEC.md). Read this to understand *why* the
> decision layer is split out from the transport layer and the plugin chain.

## The three layers

The Phenotype router architecture (ADR-050, ADR-051, §8 router-architecture
ACCEPTED 2026-06-20) is split into three layers with **one-way** dependencies:

```
┌─────────────────────────────────────────────────────────────┐
│                      Plugin chain (v0.5)                    │
│   intelligentrouter · smartfallback · learning · promptad…  │
│   contextfolding · voyage · researchintel · contentsafety  │
│   toolrouter (+ Q3-2026 vector-store slot)                  │
└────────────────────────┬────────────────────────────────────┘
                         │ calls into (no return)
┌────────────────────────▼────────────────────────────────────┐
│                  Decision layer (v0.3, this crate)          │
│   phenotype-router: BifrostAdapter · HelloWorld · plugins  │
│   DecisionLayer port (ADR-038 hexagonal L4)                │
│   OtlpDecisionRecorder (ADR-012 / ADR-036B)                 │
└────────────────────────┬────────────────────────────────────┘
                         │ calls into (no return)
┌────────────────────────▼────────────────────────────────────┐
│                Transport layer (Bifrost, ADR-051)           │
│   Go-side phenotype-gateway/packages/bifrost                │
│   HTTP/gRPC/streaming, retry/circuit-breaker/health         │
└─────────────────────────────────────────────────────────────┘
```

The arrow direction is **load-bearing**: each layer may call the layer
*below*, never the reverse. This is the substrate boundary the v11 plan
established and the v12 plan carved out.

## Why a separate decision layer?

Three reasons, in order of importance:

### 1. The decision is a *fleet-wide* concern

Every LLM-routing decision in the Phenotype fleet — `phenotype-gateway`,
`phenotype-registry`'s policy engine, `phenotype-auth`'s rate limiter, the
future `phenotype-cost` budget controller — must answer the same
question: *"for this `(adapter, request)` tuple, is the answer `Allow`,
`Defer`, or `Deny(reason)`?"* Centralising the question in a substrate
port trait makes the answer **portable** and **observable**.

### 2. The decision is **pure compute**

`decide()` is a hot-path function. It must not perform network I/O
(unless the adapter has explicitly opted in via `CapNetworkIO` per
ADR-052 §1). The decision layer is therefore a *pure* Rust library
that compiles to a static binary — no goroutines, no FFI, no async
runtime (the async overlay is reserved for the v0.4.0 plugin chain).

### 3. The decision is **observable**

Every `decide()` call emits an OTel-compatible span
(`phenotype.router.decision` per ADR-052 §3). The span carries the
adapter name, request id, decision kind, and (for `Deny`) the reason.
This gives the fleet one canonical way to trace *why* a request was
routed to model A vs model B, denied for safety, deferred to a
fallback, etc.

## Hexagonal L4 contract (ADR-038)

`DecisionLayer` is a hexagonal Port trait. Every adapter is an Adapter.

```rust
pub trait DecisionLayer: Send + Sync {
    fn name(&self) -> &str;                    // stable id (OTLP attr)
    fn adapter_kind(&self) -> &str { self.name() }  // schema tag
    fn health(&self) -> Result<(), DecisionError> { Ok(()) }  // liveness
    fn decide(&self, req: &Request) -> Response;  // hot path
}
```

Four methods, mirroring the [`pheno-port-adapter`] reference impl:
`name`, `adapter_kind`, `health`, `decide`. The defaults for
`adapter_kind` and `health` keep in-tree adapters minimal; the v0.5
plugin chain will override `health()` to surface circuit-breaker state
(ADR-006).

## OTLP substrate (ADR-012 / ADR-036B)

The recorder produces an OTel-compatible `TraceOperation` per
`decide()` call. The shape is fixed by ADR-052 §3:

| Attribute                            | Value                       |
|--------------------------------------|-----------------------------|
| `name`                               | `phenotype.router.decision` |
| `kind`                               | `SpanKind::Internal`        |
| `phenotype.router.adapter`           | `adapter.name()`            |
| `phenotype.router.request.id`        | `request.id`                |
| `phenotype.router.decision.kind`     | `decision.kind_str()`       |
| `phenotype.router.decision.reason`   | (deny only) `decision.reason`|
| `phenotype.router.service.name`      | `OtelConfig.service_name`   |

The recorder does **not** wire the OTel SDK — that's the consumer's
job (one-line `TracePort` impl against `pheno_tracing::adapters::*` or
the OTel SDK of their choice). This keeps the substrate dep footprint
minimal and avoids forcing every consumer through the same OTel SDK.

## Chaos matrix (ADR-052 §4, L25)

The decision layer must remain **valid** under *any* fault injected
at the adapter boundary. The chaos module exposes a fault-injection
framework with five categories:

| Kind         | Meaning                                   |
|--------------|-------------------------------------------|
| `Timeout`    | Plugin times out within `deadline`.       |
| `Malformed`  | Plugin returns a malformed payload.       |
| `Unreachable`| Plugin is unreachable (network).          |
| `Transient`  | Plugin returns a transient 5xx error.     |
| `Overload`   | Plugin is overloaded; substrate sheds.    |

`AlwaysFailInjector` is the strict-mode test driver; `NeverFailInjector`
is the control. The chaos test (`cargo test --features chaos`) drives
both adapters through every category and asserts the layer still
returns a valid `Response`.

## When to use this crate

Use `phenotype-router` if you are:

- Building a new LLM-routing service and need a substrate-grade decision
  layer with built-in OTLP span emission.
- Adding a new adapter (Bifrost replacement, SmartFallback, plugin
  pre-routing) and want to plug into a fleet-wide contract.
- Wiring observability for routing decisions and want one canonical
  span shape across the fleet.

## When NOT to use this crate

Don't use `phenotype-router` for:

- **Transport.** That's `phenotype-gateway/packages/bifrost` (Go).
- **Plugin chains.** That's v0.5.0; this release ships the contract.
- **OTel SDK wiring.** Consumers wire the SDK; the recorder just
  produces spans.
- **A complete routing solution.** Use `phenotype-router` as one
  layer in a three-layer architecture (plugin chain → decision →
  transport).

## See also

- [`SPEC.md`](../SPEC.md) — 1-page specification.
- [`../WORKLOG.md`](../WORKLOG.md) — worklog v2.1.
- [`perf-budget.md`](perf-budget.md) — perf budget table.
- [`architecture.md`](architecture.md) — extended architecture notes.
