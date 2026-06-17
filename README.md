# substrate

**Work-state: COMPLETE ██████████ 6/6 phases + orchestration + skills/memory + routing + process + event sourcing + dispatch planner superset**
**Status: all phases green · 150+ tests passing · clippy clean · all absorptions done**

Orchestration superset (2026-06): `SchedulePort` + `substrate-schedule` (cron/interval/daily/weekly via croner), `WorkflowPort` + `substrate-dag` (petgraph DAG: topo order, ready-set, cycle reject), `ClaimPort` + `store-sqlite` (BEGIN IMMEDIATE atomic claim + strsim fuzzy dedup).

Skills + memory superset (2026-06): `SkillPort` + `ToolRegistry` + `substrate-skills` (named invokable skills with JSON schema input validation), `MemoryPort` + `substrate-memory` (bounded ring buffer + `store-sqlite` persistent history, two-tier compose).

Routing superset (2026-06): `routing_port` in `substrate-core` (round-robin / weighted / least-used / power-of-two-choices, per-target circuit breaker Closed/Open/HalfOpen, weighted fallback chain) + `omniroute-adapter` wiring to OmniRoute providers.

Process superset (2026-06): `ProcessPort` + `runtime-process` (cross-platform managed subprocess via `command-group`: spawn in process group, status poll, wait-with-timeout, kill-group-on-timeout), `WatcherPort` + `file-watcher` (debounced filesystem events via `notify` + `notify-debouncer-mini`).

Event sourcing superset (2026-06): `EventStorePort` + `Projection`/`replay` in `substrate-core` (append-only per-aggregate event log, global monotonic sequence, duplicate-seq rejection), `SqliteEventStore` in `store-sqlite` (BEGIN IMMEDIATE seq allocation), `TaskLifecycleProjection` demo (task events → `TaskProjectionState`).

Dispatch planner superset (2026-06): `DispatchPlanner` in `substrate-app` (multi-engine capability selection + `SessionMode` Background/Foreground/InProcess, optional routing preference), `driver-cli` `plan` subcommand and `dispatch --dry-run` (print `DispatchPlan` without spawning).

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
    ┌───────────────┐  DispatchApi      │  (use-    │   TransportPort    ┌──────────────────┐
    │  driver-http   │ ───────────────▶ │  cases)  │ ◀───────────────── │  transport-file   │
    │  (REST/axum)   │                   │          │                    └──────────────────┘
    └───────────────┘                   └────┬─────┘
                                             │ depends on
    ┌───────────────┐  DispatchApi           ▼
    │  driver-mcp    │ ───────────────▶ ┌────────────────┐   RoutingPort    ┌──────────────────┐
    │  (FastMCP)     │                  │ substrate-core  │ ◀───────────────── │ omniroute-adapter │
    └───────────────┘                  │ domain + ports  │                    └──────────────────┘
                                       │ (no adapter dep)│   engine-spec: TaskSpec -> argv
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
| `substrate` | SDK facade | Re-exports domain, ports, [`DispatchPlanner`], optional `sqlite`/`forge` adapters. Single dependency for downstream repos. |
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
| `substrate-app` | application | `DispatchService` implementing `DispatchApi`, `DispatchPlanner` (engine + session-mode selection), generic over the three driven ports + optional `TracePort`. |
| `substrate-trace` | adapter | `TracePort` adapters: `NoopTrace`, `RecordingTrace` (test double), `MultiTrace` (fan-out), `AgilePlusTrace`, `TraceraTrace`. |
| `driver-cli` | inbound adapter | `substrate` binary; composition root wiring app + adapters (`dispatch`, `plan`, `--dry-run`). |
| `driver-http` | inbound adapter | `substrate-http` REST server (axum): `/v1/dispatch`, `/v1/plan`, `/v1/route`, `/v1/mailbox/*`, `/healthz`. |
| `driver-mcp` | inbound adapter | FastMCP servers (`substrate_server.py`): `substrate_dispatch` / `substrate_plan` / `substrate_route` over HTTP + team mailbox tools. |
| `omniroute-adapter` | adapter | `RoutingPort`: OmniRoute proxy config + optional routing superset (load-balance, circuit breaker, fallback). |
| `arch-test` | test-only | Architecture conformance (dependency direction). |
| `substrate-schedule` | adapter | `SchedulePort`: cron/interval/daily/weekly `next_run` via croner. |
| `substrate-dag` | adapter | `WorkflowPort`: petgraph DAG topo order, ready-set, cycle detection. |
| `substrate-skills` | adapter | `SkillPort` + `ToolRegistry`: in-memory named skills with JSON schema validation. |
| `substrate-memory` | adapter | `MemoryPort`: bounded ring buffer + two-tier compose with `store-sqlite` persistent tier. |
| `runtime-process` | adapter | `ProcessPort`: cross-platform managed subprocess (process group spawn, monitor, wait-with-timeout, kill-group) via `command-group`. |
| `file-watcher` | adapter | `WatcherPort`: debounced filesystem create/modify/remove events via `notify`. |
| `tools/fake-forge` | test fixture | Network-free stand-in for the forge CLI. |

## Rust SDK

Downstream repos (thegent, Eidolon, Agentora, sharecli) can depend on substrate instead of reimplementing dispatch:

```toml
[dependencies]
substrate = { git = "https://github.com/KooshaPari/substrate", package = "substrate" }
# optional adapters:
# substrate = { git = "...", package = "substrate", features = ["sqlite", "forge"] }
```

```rust
use substrate::{
    DispatchPlanner, EngineCandidate, EngineCapabilities, PlanRequest, SessionMode, TaskSpec,
    Task, TaskState, EnginePort, StorePort, TransportPort, DispatchApi,
};

let spec = TaskSpec::new("implement feature X", "/my/repo");
let plan = DispatchPlanner::plan(&PlanRequest {
    spec: &spec,
    engines: &[],
    explicit_engine: Some("forge"),
    session_mode: Some(SessionMode::Foreground),
    routing_engine: Some("forge"),
})?;
```

Published crates (publish-ready, `cargo publish --dry-run` green): `substrate-core`, `a2a`, `engine-spec`, `substrate-app`, and the `substrate` facade. Default features: `app` + `spec`. Optional: `a2a`. HTTP REST surface: `driver-http` workspace crate (not published; git/path dep).

## HTTP API

Non-Rust consumers (Go agentapi-plusplus, TS OmniRoute) can drive substrate over REST via `driver-http`:

```sh
# Start the server (bind + state from env; optional bearer auth)
export SUBSTRATE_HTTP_BIND=127.0.0.1:8080
export SUBSTRATE_STATE_DIR=.substrate
export SUBSTRATE_HTTP_AUTH_TOKEN=   # optional; omit for local dev
export FORGE_BIN=/path/to/fake-forge   # or real forge

cargo run -p driver-http --bin substrate-http
```

| Method | Path | Body | Response |
|--------|------|------|----------|
| `GET` | `/healthz` | — | `{ "status": "ok" }` |
| `POST` | `/v1/plan` | `{ "engine?", "cwd", "prompt", "mode?" }` | `DispatchPlan` |
| `POST` | `/v1/dispatch` | `{ "engine?", "cwd", "prompt", "mode?" }` | `StructuredResult` |
| `POST` | `/v1/route` | `{ "task": Task }` | `RoutingDecision` |
| `POST` | `/v1/mailbox/send` | `a2a::Message` | `201 Created` |
| `GET` | `/v1/mailbox/inbox?team=&to=` | — | `[Message]` |
| `GET` | `/v1/tasks?team=` | — | `[a2a::Task]` |

```sh
# Dry-run plan (no engine spawn)
curl -s localhost:8080/v1/plan \
  -H 'Content-Type: application/json' \
  -d '{"engine":"forge","cwd":"/tmp","prompt":"echo hi"}'

# Dispatch with fake forge (offline)
curl -s localhost:8080/v1/dispatch \
  -H 'Content-Type: application/json' \
  -d '{"engine":"forge","cwd":"/tmp","prompt":"echo hi"}'

# Optional bearer auth (when SUBSTRATE_HTTP_AUTH_TOKEN is set)
curl -s localhost:8080/v1/plan \
  -H "Authorization: Bearer $SUBSTRATE_HTTP_AUTH_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"cwd":"/tmp","prompt":"hi"}'
```

Enable as a library: `driver-http = { git = "https://github.com/KooshaPari/substrate", package = "driver-http" }`.

## MCP SDK (`driver-mcp`)

> **Canonical SSOT:** Deployable MCP server packages live in
> [PhenoMCPServers](https://github.com/KooshaPari/PhenoMCPServers) (`servers/substrate/`).
> `driver-mcp/` here is a **runtime convenience copy** for local development.
> Per [ADR-019](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/019-mcp-runtime-implementation-deps.md),
> long-term wiring imports from PhenoMCPServers — do not fork tool definitions in substrate.

Python [FastMCP](https://github.com/jlowin/fastmcp) servers expose substrate to MCP clients (forge, codex, claude, OmniRoute A2A). The primary entrypoint is `substrate_server.py`, which proxies dispatch/plan/route to `driver-http` and keeps team mailbox tools local.

```sh
# Start substrate HTTP (required for dispatch/plan/route tools)
export SUBSTRATE_HTTP_URL=http://127.0.0.1:8080   # default
export SUBSTRATE_HTTP_AUTH_TOKEN=                # optional bearer
cargo run -p driver-http --bin substrate-http

# Run the MCP server (stdio)
pip install -r driver-mcp/requirements.txt
python driver-mcp/substrate_server.py
```

| MCP tool | HTTP / backend | Description |
|----------|----------------|-------------|
| `substrate_dispatch` | `POST /v1/dispatch` | Run a prompt through substrate (spawns engine). Args: `prompt`, optional `engine`, `cwd`, `mode`. |
| `substrate_plan` | `POST /v1/plan` | Dry-run dispatch plan (no spawn). Args: `prompt`, optional `engine`, `cwd`. |
| `substrate_route` | `POST /v1/route` | Route a `task` object via OmniRoute adapter. |
| `team_send` | local SQLite | Send a message to another agent. |
| `team_inbox` | local SQLite | Fetch unread messages for this agent. |
| `task_list` | local SQLite | List team tasks. |

Phase 2 servers remain available: `lead_server.py` (lead inbox + `task_list`), `team_mailbox_server.py` (teammate inbox + `task_create` / `task_update`). Responses pass through `_sanitize_response` allowlist before returning to MCP clients.

Config: `SUBSTRATE_HTTP_URL` (default `http://127.0.0.1:8080`), `SUBSTRATE_HTTP_AUTH_TOKEN`, `SUBSTRATE_TEAM_ID`, `SUBSTRATE_AGENT_NAME`, `SUBSTRATE_DB`.

```sh
pip install -r driver-mcp/requirements.txt
pytest driver-mcp/
```

## Budget LLM routing (`driver-argv`)

There is **no** `cheap-llm-mcp` repo. Budget / tier routing lives in the `driver-argv`
crate and the `substrate argv` CLI subcommand (multi-provider argv builder absorbed from
thegent-dispatch). Use argv for CLI-only routing; use `driver-mcp/dispatch_server.py` or
PhenoMCPServers `substrate-dispatch` for OmniRoute tier MCP tools.

```sh
cargo run -p driver-cli --bin substrate -- argv --provider forge --prompt "hello" --dry-run
```

## Quickstart

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings

# Run a fully offline dispatch through the fake forge:
cargo run -p driver-cli --bin substrate -- \
  dispatch --engine forge --fake --cwd . "echo hi"

# Dry-run: print the dispatch plan without spawning:
cargo run -p driver-cli --bin substrate -- \
  plan --engine forge --cwd . "echo hi"
```

## Task lifecycle FSM

`Submitted → Working → InputRequired → Working → Completed`, with `Failed` and
`Cancelled` reachable from any non-terminal state. Terminal states have no
outgoing edges. Enforced by `TaskState::can_transition` / `Task::advance`.

[forge]: https://github.com/antinomyhq/forge
