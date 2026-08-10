# ADR-0002 — Hexagonal ports/adapters architecture

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** substrate-core maintainers
- **Supersedes:** none
- **Superseded by:** none

---

## Context

Substrate exposes many ingress surfaces (HTTP, A2A, CLI, MCP, argv) and
many downstream engines (forge, codex, claude, a2a, agentapi, plus cloud
variants). Naïve layering would couple engines to gateway code or
duplicate dispatch logic across drivers, breaking under any new engine.

We need a layout that:

1. Lets us add a new engine without touching the gateway crate.
2. Lets us add a new driver without touching engine crates.
3. Makes the dispatch surface and the routing surface independently
   swappable.
4. Forces every layer to compile against a single source of truth for
   domain types.

## Decision

We adopt **hexagonal architecture** as the binding layout:

- `substrate-core` is the innermost layer. It defines:
  - Port traits (`DispatchApi`, `RoutingPort`, `MailboxStore`, `TracePort`,
    `Engine`).
  - Domain types (`Task`, `RoutingDecision`, `StructuredResult`, `TaskSpec`).
  - The canonical `Error` enum (see ADR-0003).
  - **It depends only on std + workspace-internal L0 utilities.**
- Adapters (gateway, drivers, store-sqlite, store-file, transport-file,
  engine-\*) implement the port traits. They depend on `substrate-core`.
- Use-cases (substrate-app, substrate-dag, substrate-schedule,
  substrate-skills, context-budget, dispatch-bridge) compose adapters.
- Inbound drivers (driver-http, driver-cli, driver-mcp, psub-gateway,
  substrate-tui) wire concrete adapters at a composition root.

**Forbidden:** `substrate-core` may not depend on any other workspace
crate. This is verified by `cargo tree --invert substrate-core` returning
only `std`.

## Consequences

**Positive**
- Adding an engine is a new crate; nothing in the gateway or core moves.
- We can swap `PhenotypeRouterAdapter` for a different router without
  touching dispatch code.
- The same `DispatchService` powers the HTTP gateway and the CLI driver —
  both wire the same adapters, just at different ingress layers.

**Negative**
- New contributors must understand the layer rules before they can land
  code. The rule of thumb: "where does the trait live? — that's the
  adapter's home."
- Trait-object dispatch (via `Arc<dyn …>`) costs a thin vtable hop in
  the hot path. Measured impact is negligible.

## Alternatives considered

- **Single-crate module layout.** Rejected: layer bleeding would emerge
  inevitably; PR review would lose signal.
- **WASM-component style.** Rejected: not yet a stable Rust story in 2026.
- **Database-driven dispatch table.** Rejected: removes type safety and
  doesn't carry enough information per-request.

## References

- `crates/substrate-core/src/ports.rs` — DispatchApi, RoutingPort.
- `crates/substrate-core/src/mailbox_port.rs` — MailboxStore.
- `crates/substrate-core/src/trace.rs` — TracePort.
- `crates/driver-http/src/lib.rs:42-76` — composition root.
- `ARCHITECTURE.md` §3 — dependency direction diagram.
