# Architecture — kooshapari/substrate

> **Status:** Phase 0 scaffold (re-mediated 2026-07-08)
> **Crate taxonomy follows post-`cb9a3e7` psub- rename.** Where older docs
> refer to `gateway/`, the renamed path is `crates/psub-gateway/`.

---

## 1. Mission

`substrate` is a Rust 2021 workspace that implements an agent-orchestration
gateway. It exposes an **OpenAI-compatible HTTP surface** (chat completions,
models listing) plus an **A2A mailbox**, all backed by a pluggable adapter
hierarchy that lets the same dispatch logic talk to CLI agents, HTTP APIs,
MCP servers, or cloud-hosted agents.

The project deliberately separates the **dispatch plumbing** (where a
request goes, in what order, on what account) from the **adapter surface**
(how a request reaches a given engine). Adapters can be swapped without
touching the routing engine; the routing engine can evolve without
breaking a single adapter.

---

## 2. Workspace layout

```
kooshapari/substrate/
├── Cargo.toml                    # workspace; rust-version=1.80
├── Cargo.lock                    # committed
├── Containerfile                 # container build
├── rust-toolchain.toml           # pinned toolchain
├── crates/
│   ├── substrate-core/           # ports / domain types / canonical error  [L0]
│   ├── substrate-app/            # use-case orchestration                    [L1]
│   ├── substrate-dag/            # DAG types + petgraph glue                [L1]
│   ├── substrate-schedule/       # cron / debounced triggers                [L1]
│   ├── substrate-skills/         # skill invocation                         [L1]
│   ├── substrate-memory/         # persistent memory                        [L1]
│   ├── substrate-serve-lock/     # single-instance guard                    [L1]
│   ├── substrate-trace/          # in-mem trace ports                       [L2]
│   ├── substrate-tui/            # ratatui dashboard                        [L3]
│   ├── psub/                     # meta-crate                               [L4]
│   ├── psub-gateway/             # OpenAI-compatible HTTP + A2A ingress   [L3]
│   ├── psub-a2a/                 # A2A outbound adapters                    [L2]
│   ├── psup-supervisor/          # process supervisor                       [L3]
│   ├── psub-wave/                # wave execution primitive                 [L3]
│   ├── psub-orchestrator/        # multi-engine orchestrator                [L2]
│   ├── psub-file-watcher/        # notify-debouncer wrapper                 [L2]
│   ├── store-file/               # file-system store adapter                [L2]
│   ├── store-sqlite/             # SQLite store adapter                     [L2]
│   ├── transport-file/           # file-mailbox transport                   [L2]
│   ├── routing-phenotype-router/ # policy/router                            [L2]
│   ├── omniroute-adapter/        # fork-only Bifrost executor (see ADR-031) [L2]
│   ├── engine-spec/              # TaskSpec / engine interface              [L2]
│   ├── engine-forge/             # forge-engine adapter                     [L2]
│   ├── engine-codex/             # codex-cloud adapter                      [L2]
│   ├── engine-claude/            # claude-code adapter                      [L2]
│   ├── engine-a2a/               # a2a-engine adapter                       [L2]
│   ├── engine-agentapi/          # Go agentapi-plusplus adapter             [L2]
│   ├── engine-conformance/       # shared engine conformance suite          [L2]
│   ├── cliproxy-adapter/         # Claude-code CLI subprocess wrapper       [L2]
│   ├── phenotype-mcp/            # MCP server (87 tools, 30 scopes)         [L2]
│   ├── driver-http/              # HTTP/REST inbound adapter                [L3]
│   ├── driver-cli/               # CLI inbound adapter                      [L3]
│   ├── driver-argv/              # argv stub                                [L3]
│   ├── driver-mcp/               # MCP server adapter                       [L3]
│   ├── context-budget/           # token-budget accounting                  [L1]
│   ├── dispatch-bridge/          # cross-engine dispatch bridge             [L2]
│   ├── runtime-process/          # tokio process runtime helpers            [L1]
│   ├── cloud-codex /            # cloud-engine adapters                    [L2]
│   ├── cloud-cursor /                                                                    
│   ├── cloud-kilo /                                                                       
│   ├── cloud-dispatch-conformance/                                                       
│   ├── wave-3lane-tests/         # 3-lane integration test fixture           [L3]
│   ├── gateway-tools/            # admin tooling                            [L3]
│   └── arch-test/                # workspace-wide conformance               [T]
├── driver-mcp/                   # separate crate (mcp server, top-level)
├── tools/                                                                                
│   ├── fake-forge/                                                                       
│   └── fake-codex-cloud/                                                                 
├── forge-daemon/                 # local fork-only daemon (excluded from workspace)
├── docs/                         # architecture, ops, guides
│   └── adr/                      # accepted ADRs (6 as of 2026-07-08)
├── processes, adrs, ops docs                                                                
└── .github/workflows/            # CI, security, release-binary, release-crates
```

Layer legend: **[L0]** innermost; **[L1]** use-cases; **[L2]** ports/drivers;
**[L3]** inbound adapters + CLI; **[L4]** meta-crate; **[T]** test-only.

---

## 3. Dependency direction

Edges point inward — adapters depend on the core, never the reverse:

```
                   [L4 meta-crate]
                         │
        ┌────────────────┴────────────────┐
        │                                 │
    [L3 inbound adapters]         [L3 TUI / metrics / tools]
    driver-http, driver-cli,       psub-gateway, substrate-tui,
    driver-mcp, driver-argv        gateway-tools
        │                                 │
        └────────────────┬────────────────┘
                         │
        ┌────────────────┴────────────────┐
        │                                 │
    [L2 ports/drivers]              [L2 policy/router]
    engine-*, store-*, psub-a2a,    routing-phenotype-router,
    omniroute-adapter,              omniroute-adapter
    cliproxy-adapter
        │                                 │
        └────────────────┬────────────────┘
                         │
        ┌────────────────┴────────────────┐
        │                                 │
    [L1 use-cases]                 [L1 helpers]
    substrate-app, substrate-dag,  context-budget,
    substrate-skills,              substrate-schedule,
    substrate-memory,              substrate-serve-lock,
    dispatch-bridge                runtime-process
        │                                 │
        └────────────────┬────────────────┘
                         │
                  [L0 substrate-core]
                  ports, domain types,
                  canonical Error
```

**Forbidden:** an L0 / L1 crate depending on an L2 / L3 crate.

**Verified by:** `cargo tree --invert substrate-core` returns only std + third-party.

---

## 4. Core abstractions (substrate-core)

`substrate-core` defines the **port traits** that the rest of the workspace
implements. Adapters are called through dyn-compatible trait objects so the
composition root can choose concrete implementations at runtime.

| Trait (port)            | Defined in                          | Implementations                  |
|-------------------------|-------------------------------------|-----------------------------------|
| `DispatchApi`           | `crates/substrate-core/src/ports.rs`| `DispatchService` in `substrate-app` |
| `RoutingPort`           | `crates/substrate-core/src/ports.rs`| `PhenotypeRouterAdapter`          |
| `MailboxStore`          | `crates/substrate-core/src/mailbox_port.rs` | `SqliteMailboxStore`       |
| `TracePort`             | `crates/substrate-core/src/trace.rs`| `NoopTrace`, `RecordingTrace`, `AgilePlusTrace`, `TraceraTrace` |
| `TaskSpec`              | `crates/substrate-core/src/spec.rs` | engine-side: `EngineTaskSpec`     |

See:
- `crates/substrate-core/src/ports.rs` (DispatchApi, RoutingPort)
- `crates/substrate-core/src/mailbox_port.rs` (MailboxStore)
- `crates/substrate-core/src/trace.rs` (TracePort + 3 event types)
- `crates/substrate-core/src/domain.rs` (Task, RoutingDecision, StructuredResult)
- `crates/substrate-core/src/error.rs` (canonical Error)

### Composition root

Concrete adapters are wired at the composition root in `driver-http`:

```
driver-http/src/lib.rs:42-76
  AppState::new(state_dir) ->
      DispatchService(ForgeEngine, FileStore, FileTransport)
      PhenotypeRouterAdapter::default()
      SqliteMailboxStore::open(...)
```

---

## 5. Request pipeline

A request to `POST /v1/chat/completions` flows:

```
Client
  │  POST /v1/chat/completions
  ▼
[psub-gateway]  OpenAI surface + auth + rate-limit + circuit-breaker
  │
  ▼
[driver-http]   Generic dispatch surface (axum Router)
  │
  ▼
[substrate-app] DispatchPlanner.plan(PlanRequest)
  │
  ├──▶ task-aware router  →  [routing-phenotype-router]
  │                          (policy: cost / latency / affinity)
  │
  ├──▶ engine selected  →  engine-forge | engine-codex | engine-claude | engine-a2a | ...
  │
  ├──▶ engine-forge dispatch → dispatch-bridge → forge-engine
  │
  └──▶ trace event  →  substrate-trace (RecordingTrace / AgilePlus / Tracera)
```

---

## 6. Crate-by-crate map

| Crate | Layer | Purpose | Public surface (top-level) |
|---|---|---|---|
| `substrate-core` | L0 | ports, domain types, error | `ports`, `domain`, `trace`, `error`, `mailbox_port`, `spec` |
| `substrate-app` | L1 | use-case orchestration | `DispatchService`, `DispatchPlanner`, `PlanRequest`, `SessionMode` |
| `substrate-dag` | L1 | DAG types over petgraph | DAG + serialization |
| `substrate-schedule` | L1 | cron + debounced triggers | `croner` wrapper |
| `substrate-skills` | L1 | skill invocation | registry + sandbox |
| `substrate-memory` | L1 | persistent memory | extraction/injection/retrieval |
| `substrate-serve-lock` | L1 | single-instance guard | `fs2`-based lock |
| `substrate-trace` | L2 | trace port adapters | `NoopTrace`, `RecordingTrace`, `AgilePlusTrace`, `TraceraTrace` |
| `substrate-tui` | L3 | ratatui dashboard | TUI app shell |
| `psub-gateway` | L3 | OpenAI-compatible HTTP + A2A ingress + admin + audit log + rate-limit + circuit-breaker + bounded body + metrics | full axum router |
| `psub-a2a` | L2 | A2A outbound adapters | skill modules |
| `psup-supervisor` | L3 | process supervisor | runtime helpers |
| `psub-wave` | L3 | wave execution primitive | wave API |
| `psub-orchestrator` | L2 | multi-engine orchestrator | composite dispatch |
| `psub-file-watcher` | L2 | `notify-debouncer-mini` wrapper | hot-reload helper |
| `store-file` | L2 | file-system store adapter | `FileStore` |
| `store-sqlite` | L2 | SQLite store adapter | `SqliteConfigStore`, `SqliteMailboxStore` |
| `transport-file` | L2 | file-mailbox transport | `FileTransport` |
| `routing-phenotype-router` | L2 | policy/router | `PhenotypeRouterAdapter` |
| `omniroute-adapter` | L2 | fork-only Bifrost executor | `BifrostBackendExecutor` |
| `engine-spec` | L2 | `TaskSpec` / engine interface | trait |
| `engine-forge` | L2 | forge-engine adapter | `ForgeEngine` |
| `engine-codex` | L2 | codex-cloud adapter | engine impl |
| `engine-claude` | L2 | claude-code adapter | engine impl |
| `engine-a2a` | L2 | a2a-engine adapter | engine impl |
| `engine-agentapi` | L2 | Go agentapi-plusplus adapter | engine impl |
| `engine-conformance` | L2 | shared engine conformance suite | tests |
| `cliproxy-adapter` | L2 | Claude-code CLI subprocess wrapper | CLI orchestrator |
| `phenotype-mcp` | L2 | MCP server (87 tools, 30 scopes) | registry + transport |
| `driver-http` | L3 | HTTP/REST inbound adapter | axum Router |
| `driver-cli` | L3 | CLI inbound adapter | command parser |
| `driver-argv` | L3 | argv stub | CLI bootstrap |
| `driver-mcp` | L3 | MCP server adapter | tool handlers |
| `context-budget` | L1 | token-budget accounting | budget tracker |
| `dispatch-bridge` | L2 | cross-engine dispatch bridge | bridge protocol |
| `runtime-process` | L1 | tokio process runtime helpers | subprocess wrappers |
| `cloud-codex` / `cloud-cursor` / `cloud-kilo` / `cloud-dispatch-conformance` | L2 | cloud-engine adapters | engine impls |
| `wave-3lane-tests` | T | 3-lane integration test fixture | tests |
| `gateway-tools` | L3 | admin tooling | helper scripts |
| `arch-test` | T | workspace-wide conformance | macro + tests |
| `psub` | L4 | meta-crate | re-exports |

---

## 7. Key design decisions

| Decision | Why | ADR |
|---|---|---|
| Hexagonal ports/adapters (no upward deps) | Swap adapters without touching the routing engine | [ADR-0002](./adr/0002-hexagonal-ports-adapters.md) |
| Canonical Error in core, `.context()` everywhere | Uniform error-handling pattern across adapters | [ADR-0003](./adr/0003-canonical-error-type.md) |
| Atomic claim lease for single-instance guard | Multiple gateway instances should not race on the same mailbox slot | [ADR-0004](./adr/0004-atomic-claim-lease.md) |
| `rusqlite` with `bundled` feature | Direct SQL + zero runtime cost + reproducible builds | [ADR-0005](./adr/0005-sqlite-default-store.md) |
| `tracing` + port-based emission | Application code is trace-backend agnostic | [ADR-0006](./adr/0006-structured-tracing.md) |
| Polyglot gateway (HTTP + A2A + CLI) | One substrate, many ingress surfaces | [ADR-0001](./adr/0001-gateway-polyglot.md) |

---

## 8. CI / verification

Every PR runs:

| Gate | Command | Source |
|---|---|---|
| format | `cargo fmt --all -- --check` | `ci.yml` |
| build | `cargo build --workspace` | `ci.yml` |
| test | `cargo test --workspace` | `ci.yml` |
| lint | `cargo clippy --all-targets -- -D warnings` | `ci.yml` |
| deny | `cargo deny check` | `ci.yml` (added 2026-07-08 by PR-A) |
| secrets | gitleaks | `security.yml` |
| advisories | `cargo audit` | `security.yml` |

Release gates (post-tag): SLSA provenance + CycloneDX SBOM + GHCR image.

---

## 9. Where to start

| You are trying to... | Read this |
|---|---|
| understand a request end-to-end | §5 |
| add a new engine adapter | §3 (depend direction), §6 (engine-* rows), `engine-conformance` template |
| add a new inbound protocol | §4 (ports), `driver-http/src/lib.rs` (composition root), `psub-gateway/src/lib.rs` (axum handler template) |
| change the routing policy | `crates/routing-phenotype-router/src/`, `ARCH-12` |
| ship a release | `docs/ops/RELEASE.md` (TBW), CI release workflows |
