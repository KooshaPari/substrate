# substrate

**Work-state: COMPLETE ██████████ 6/6 phases + orchestration + skills/memory + routing + process + event sourcing superset**
**Status: all phases green · 150+ tests passing · clippy clean**

Orchestration superset (2026-06): `SchedulePort` + `substrate-schedule` (cron/interval/daily/weekly via croner), `WorkflowPort` + `substrate-dag` (petgraph DAG: topo order, ready-set, cycle reject), `ClaimPort` + `store-sqlite` (BEGIN IMMEDIATE atomic claim + strsim fuzzy dedup).

Skills + memory superset (2026-06): `SkillPort` + `ToolRegistry` + `substrate-skills` (named invokable skills with JSON schema input validation), `MemoryPort` + `substrate-memory` (bounded ring buffer + `store-sqlite` persistent history, two-tier compose).

Routing superset (2026-06): `routing_port` in `substrate-core` (round-robin / weighted / least-used / power-of-two-choices, per-target circuit breaker Closed/Open/HalfOpen, weighted fallback chain) + `omniroute-adapter` wiring to OmniRoute providers.

Process superset (2026-06): `ProcessPort` + `runtime-process` (cross-platform managed subprocess via `command-group`: spawn in process group, status poll, wait-with-timeout, kill-group-on-timeout), `WatcherPort` + `file-watcher` (debounced filesystem events via `notify` + `notify-debouncer-mini`).

Event sourcing superset (2026-06): `EventStorePort` + `Projection`/`replay` in `substrate-core` (append-only per-aggregate event log, global monotonic sequence, duplicate-seq rejection), `SqliteEventStore` in `store-sqlite` (BEGIN IMMEDIATE seq allocation), `TaskLifecycleProjection` demo (task events → `TaskProjectionState`).

A hexagonal (ports-and-adapters) spine for dispatching agent tasks to coding
engines such as [forge]. The **core** holds pure contracts; **adapters** plug
concrete engines, transports, and stores into those contracts; the
**application** wires them at a single composition root.

## Hexagonal architecture

```
                 driving side                         driven side
              (inbound adapters)                  (outbound adapters)

    ┌───────────────┐                                ┌──────────────────┐
    │  driver-cli    │  DispatchApi      EnginePort  │  engine-forge     │
    │  (substrate)   │ ───────────────▶ ┌──────────┐ ◀─────────────────  │  (forge CLI)
    └───────────────┘                   │          │                    └──────────────────┘
                                        │ substrate │   StorePort        ┌──────────────────┐
                                        │  -app     │ ◀───────────────── │  store-file       │
    ┌───────────────┐  DispatchApi      │  (use-    │                    └──────────────────┘
    │  (future       │ ───────────────▶ │  cases)  │   TransportPort    ┌──────────────────┐
    │   HTTP/MCP)    │                   │          │ ◀───────────────── │  transport-file   │
    └───────────────┘                   └────┬─────┘                    └──────────────────┘
                                             │ depends on
                                             ▼
                                     ┌────────────────┐   RoutingPort (port defined, Phase 1 adapter)
                                     │ substrate-core  │
                                     │ domain + ports  │   engine-spec: TaskSpec -> argv
                                     │ (no adapter dep)│
                                     └────────────────┘
```

**Dependency rule (enforced):** `substrate-core` depends only on `serde`,
`serde_json`, `thiserror`, `uuid`, and `async-trait` (needed to express async
port traits). It never depends on an adapter. `crates/arch-test` parses
`substrate-core/Cargo.toml` and fails the build if any `engine-*`,
`transport-*`, `store-*`, `driver-*`, or `*-adapter` dependency appears.

## Crates

| Crate | Layer | Responsibility |
|-------|-------|----------------|
| `substrate-core` | core | Domain entities + lifecycle FSM, port traits (`EnginePort`, `RoutingPort`, `TransportPort`, `StorePort`, `DispatchApi`, `SchedulePort`, `WorkflowPort`, `ClaimPort`, `SkillPort`, `ToolRegistry`, `MemoryPort`, `ProcessPort`, `WatcherPort`, `EventStorePort`), routing superset (`RoutingStrategy`, circuit breaker, fallback chain), `Projection`/`replay`, `TracePort` + event structs, `SubstrateError`. |
| `engine-spec` | core-side contract | Provider-agnostic `TaskSpec` and the `ArgvBuilder` trait. |
| `engine-forge` | adapter | `EnginePort` driving the `forge` CLI (`FORGE_BIN`); tolerant conversation-id regex, dump→`StructuredResult` normalization, PR-URL extraction. |
| `engine-codex` | adapter | `EnginePort` driving the `codex` CLI (`CODEX_BIN`; `CODEX_INTEGRATION=1` for real calls). |
| `engine-claude` | adapter | `EnginePort` driving the `claude` CLI (`CLAUDE_BIN`; `CLAUDE_INTEGRATION=1` for real calls). |
| `engine-agentapi` | adapter | `EnginePort` HTTP adapter for agentapi-plusplus (`AGENTAPI_ENDPOINT`; `AGENTAPI_INTEGRATION=1`). |
| `engine-conformance` | test harness | `assert_engine_conformance<E>` — runs the harness-agnostic contract suite against any adapter, offline. |
| `transport-file` | adapter | `TransportPort`: append-only JSONL mailboxes + lockfile-lease atomic claim. |
| `store-file` | adapter | `StorePort`: one JSON file per task/result + lockfile-lease atomic claim. |
| `store-sqlite` | adapter | `MailboxStore`, `ClaimPort`, `MemoryPort`, `EventStorePort` (append-only event log + global seq). |
| `substrate-app` | application | `DispatchService` implementing `DispatchApi`, generic over the three driven ports + optional `TracePort`. |
| `substrate-trace` | adapter | `TracePort` adapters: `NoopTrace`, `RecordingTrace` (test double), `MultiTrace` (fan-out), `AgilePlusTrace`, `TraceraTrace`. |
| `driver-cli` | inbound adapter | `substrate` binary; composition root wiring app + adapters. |
| `omniroute-adapter` | adapter | `RoutingPort`: OmniRoute proxy config + optional routing superset (load-balance, circuit breaker, fallback). |
| `arch-test` | test-only | Architecture conformance (dependency direction). |
| `substrate-schedule` | adapter | `SchedulePort`: cron/interval/daily/weekly `next_run` via croner. |
| `substrate-dag` | adapter | `WorkflowPort`: petgraph DAG topo order, ready-set, cycle detection. |
| `substrate-skills` | adapter | `SkillPort` + `ToolRegistry`: in-memory named skills with JSON schema validation. |
| `substrate-memory` | adapter | `MemoryPort`: bounded ring buffer + two-tier compose with `store-sqlite` persistent tier. |
| `runtime-process` | adapter | `ProcessPort`: cross-platform managed subprocess (process group spawn, monitor, wait-with-timeout, kill-group) via `command-group`. |
| `file-watcher` | adapter | `WatcherPort`: debounced filesystem create/modify/remove events via `notify`. |
| `tools/fake-forge` | test fixture | Network-free stand-in for the forge CLI. |

## Quickstart

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings

# Run a fully offline dispatch through the fake forge:
cargo run -p driver-cli --bin substrate -- \
  dispatch --engine forge --fake --cwd . "echo hi"
```

## Task lifecycle FSM

`Submitted → Working → InputRequired → Working → Completed`, with `Failed` and
`Cancelled` reachable from any non-terminal state. Terminal states have no
outgoing edges. Enforced by `TaskState::can_transition` / `Task::advance`.

[forge]: https://github.com/antinomyhq/forge
