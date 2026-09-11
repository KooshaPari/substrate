# Architecture notes

> Extended companion to [`SPEC.md`](../SPEC.md) and [`concept.md`](concept.md).
> Read this if you are adding a new adapter or wiring the substrate into a
> new consumer.

## Crate layout

```
phenotype-router/
├── Cargo.toml              # standalone crate; [workspace] for bench auto-detect
├── SPEC.md                 # 1-page spec
├── README.md               # public-facing docs
├── llms.txt                # LLM-facing context (per AGENTS.md convention)
├── WORKLOG.md              # v2.1 schema (with device: field)
├── CHANGELOG.md            # cliff-style
├── LICENSE-MIT             # MIT license
├── AGENTS.md               # AI-agent context
├── CODEOWNERS              # @KooshaPari owns all
├── flake.nix               # nix dev shell (ADR-039)
├── .github/
│   └── workflows/ci.yml    # CI gate
├── src/
│   ├── lib.rs              # public exports
│   ├── decision.rs         # DecisionLayer port + Decision/Request/Response
│   ├── bifrost_adapter.rs  # BifrostAdapter (stub; future FFI)
│   ├── hello_world.rs      # HelloWorld fixture
│   ├── otel.rs             # OtlpDecisionRecorder + TracePort + InMemoryTracePort
│   ├── tracing.rs          # tracing façade + DECISION_SPAN_NAME constant
│   ├── chaos.rs            # chaos matrix (ADR-052 §4)
│   ├── sdk.rs              # LlmPort / DecisionPlugin / ConnectorPort (ADR-052)
│   └── plugins/            # 3-plugin port wave (v13)
├── tests/
│   ├── hello_world.rs      # hello-world integration (6 tests)
│   ├── decision_e2e.rs     # end-to-end (8 tests)
│   ├── otlp_smoke.rs       # OTLP smoke (4 tests, ADR-012/036B)
│   └── chaos_matrix.rs     # chaos matrix (6 tests, L25)
├── benches/
│   └── decision.rs         # criterion bench (3 targets)
└── docs/
    ├── concept.md          # concept doc
    ├── architecture.md     # this file
    └── perf-budget.md      # perf budget table
```

## Module dependency graph

```
                    ┌──────────────┐
                    │  plugins/*   │  (v0.5 plugin chain — v13 port wave)
                    └──────┬───────┘
                           │ impl sdk::{LlmPort, DecisionPlugin, ConnectorPort}
                           ▼
┌──────────┐  uses   ┌──────────────┐
│  sdk.rs  │◀────────│   lib.rs     │
└────┬─────┘         └──────┬───────┘
     │                      │ re-exports
     │ impl                 ▼
     ▼                ┌─────────────────┐
┌──────────────┐      │                 │
│  decision.rs │◀─────│  consumer code  │
│  (Port trait)│      │                 │
└──────┬───────┘      └─────────────────┘
       │ impl DecisionLayer
       ▼
┌──────────────────┐
│  bifrost_adapter │
│  hello_world     │
│  (in-tree)       │
└──────────────────┘

  decision.rs  ───uses──▶  otel.rs (TraceOperation, TracePort)
  otel.rs       ───produces──▶  ADR-052 §3 attribute shape
```

## How the OTel bridge works

```
   decide(req)
       │
       ▼
   Response { decision, trace }
       │
       ├──▶ OtlpDecisionRecorder::build_operation(adapter, req, &resp)
       │       │
       │       ▼
       │   TraceOperation {
       │       name: "phenotype.router.decision",
       │       kind: SpanKind::Internal,
       │       attributes: {
       │           "phenotype.router.adapter":          adapter.name(),
       │           "phenotype.router.request.id":       req.id,
       │           "phenotype.router.decision.kind":    resp.decision.kind_str(),
       │           "phenotype.router.decision.reason":  (deny only),
       │           "phenotype.router.service.name":     config.service_name,
       │       },
       │   }
       │
       └──▶ TracePort::submit(op)   ──▶  OTel SDK (consumer-wired)
```

The recorder is a **port** (`TracePort`); the consumer wires the OTel
SDK of their choice. The in-tree `InMemoryTracePort` is the test
adapter; production adapters are written in the consumer's `main.rs`
(one-line shim around `pheno_tracing::adapters::*` or
`opentelemetry-otlp`).

## SDK contract (ADR-052)

The SDK module (`src/sdk.rs`) defines three port traits that future
plugin-chain adapters will implement:

```rust
#[async_trait]
pub trait LlmPort: Send + Sync {
    async fn send(&self, req: LlmRequest) -> Result<LlmResponse, LlmError>;
    fn capabilities(&self) -> Capabilities;
}

pub trait DecisionPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn phase(&self) -> Phase;
    fn apply(&self, decision: &PluginDecision) -> Result<PluginDecision, PluginError>;
}

#[async_trait]
pub trait ConnectorPort: Send + Sync {
    async fn connect(&self, cfg: &ConnectorConfig) -> Result<ConnectorHandle, ConnectorError>;
    fn capabilities(&self) -> Capabilities;
}
```

`Capabilities` is a bitflag set the plugin author fills in to declare
whether the plugin performs network I/O, holds state, or must run
pre-routing. The plugin-chain orchestrator (v0.5.0) uses
`capabilities()` to schedule plugins correctly.

## v13 3-plugin port wave

`src/plugins/{promptadapter,contextfolding,researchintel}.rs` are
the v13 port wave from the `argis-extensions` reference (Go →
Rust). They ship as **experimental** in v0.3.0 and graduate to
**stable** in v0.5.0 once the plugin-chain orchestrator lands.

Each plugin:

- Implements one of the three SDK traits.
- Carries a `PREDICTIVE.md` next to its source documenting the
  ADR-047 4-criterion predictive-DRY check.
- Has unit tests in a `#[cfg(test)] mod tests` block.
- Emits OTel-compatible `tracing` spans per ADR-012 / ADR-036B.

## Where the substrate ends

`phenotype-router` does **not** own:

- The OTel SDK (consumer-wired via `TracePort`).
- The Bifrost FFI bridge (v0.4.0, in `phenotype-gateway`).
- The plugin chain orchestrator (v0.5.0).
- The transport layer (Bifrost, Go-side, ADR-051).

`phenotype-router` is a *pure* Rust library. It compiles to a
static binary with no runtime dependencies beyond `tokio` (test-only)
and `tracing` / `tracing-subscriber` (re-exported for plugin
authors).
