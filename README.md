# substrate

**Work state:** ACTIVE · █████░░░░░ 5/6 phases · Phase 4 (parallel WaveRunner + sub-subagent fan-out + depth guard + harvest) complete

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
| `substrate-core` | core | Domain entities + lifecycle FSM, the five port traits (`EnginePort`, `RoutingPort`, `TransportPort`, `StorePort`, `DispatchApi`), `SubstrateError`. |
| `engine-spec` | core-side contract | Provider-agnostic `TaskSpec` and the `ArgvBuilder` trait. |
| `engine-forge` | adapter | `EnginePort` driving the `forge` CLI (`FORGE_BIN`); tolerant conversation-id regex, dump→`StructuredResult` normalization, PR-URL extraction. |
| `transport-file` | adapter | `TransportPort`: append-only JSONL mailboxes + lockfile-lease atomic claim. |
| `store-file` | adapter | `StorePort`: one JSON file per task/result + lockfile-lease atomic claim. |
| `substrate-app` | application | `DispatchService` implementing `DispatchApi`, generic over the three driven ports. |
| `driver-cli` | inbound adapter | `substrate` binary; composition root wiring app + adapters. |
| `arch-test` | test-only | Architecture conformance (dependency direction). |
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
