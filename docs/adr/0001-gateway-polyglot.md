# ADR 0001: Gateway polyglot language strategy

## Status

Accepted

## Context

Substrate is a hexagonal (ports-and-adapters) system: `substrate-core` holds pure
contracts; adapters plug concrete engines, transports, stores, and drivers into those
contracts; the application layer wires them at a single composition root. The
dependency rule is enforced (`arch-test`): core never depends on an adapter.

The **gateway** is a new inbound driver that exposes an OpenAI-compatible HTTP surface
(`/v1/chat/completions`, `/v1/models`), mounts the A2A mailbox at `/a2a`, and provides
thin management endpoints backed by `store-sqlite`. It sits on the driving (inbound) side
alongside `driver-cli`, `driver-http`, and `driver-mcp`.

As the gateway grows, some components will be latency-sensitive (token streaming,
batch inference prep, SIMD-heavy transforms), some will be long-lived network services
(observability sidecars, provider health probes), and most will simply implement existing
`substrate-core` ports. A single-language mandate would either sacrifice performance on
hot paths or force FFI/port-shim overhead where Rust adapters already exist.

We need a language strategy that preserves the hexagonal spine, keeps the composition
root coherent, and lets each component pick a language on merit.

## Decision

Adopt a **merit-based polyglot** strategy for gateway components:

| Layer | Language | Role |
|-------|----------|------|
| **Spine** | **Rust** (fixed) | Core ports, domain, composition root, primary HTTP gateway binary |
| **Hot-leaf cores** | **Zig** or **Mojo** | Leaf compute kernels where microseconds and SIMD matter |
| **Service boundaries** | **Go** (by merit) | Standalone sidecar services at clear process/network boundaries |
| **Port adapters** | **Rust** (direct) | Implementations of existing `substrate-core` port traits |

### Rust spine (non-negotiable)

The spine stays Rust:

- **`substrate-core`** — domain entities, port traits, routing superset, event sourcing
  contracts. No adapter dependencies.
- **Composition root** — the gateway binary wires `RoutingPort`, `MailboxStore`,
  `SqliteConfigStore`, and HTTP routes (axum) in one place, matching `driver-http` and
  `substrate-app` patterns.
- **Primary HTTP surface** — axum router, auth middleware, OpenAI-shaped handlers, A2A
  mount. Reuses existing crates (`omniroute-adapter`, `store-sqlite`, `a2a`).
- **Arch enforcement** — `arch-test` and workspace `Cargo.toml` dependency direction
  remain the single source of truth for layering.

Rust is chosen for the spine because it already owns the port graph, provides
memory-safe concurrency without a GC pause on the request path, ships a mature async
ecosystem (tokio/axum) aligned with `driver-http`, and keeps FFI boundaries explicit
(one direction: hot leaves call *into* Rust ports, not the reverse).

### Zig / Mojo — hot-leaf cores

Use Zig or Mojo for **leaf** compute modules that sit at the bottom of the call graph
(no further outbound port calls), when profiling shows the leaf dominates p99 latency.

**Criteria (all should be true):**

1. **Hot path** — invoked per token, per frame, or per request on the critical path;
   expected to account for a measurable share of wall time.
2. **SIMD / numeric intensity** — benefits from explicit vectorization, structure-of-arrays
   layouts, or GPU-style kernels (Mojo) that outperform idiomatic Rust without `unsafe`
   sprawl.
3. **Determinism** — output must be stable across runs for a given input (routing weights,
   tokenizer merges, fixed-point quant tables). Nondeterministic GPU paths require an
   explicit opt-in ADR amendment.
4. **Narrow ABI** — exposes a small C ABI or serialized blob interface; no port trait
   implementations in Zig/Mojo.

Zig fits CPU-bound, allocation-controlled kernels (custom token filters, SIMD JSON
escaping). Mojo fits numeric/ML-adjacent kernels where the Mojo stdlib and compiler
target SIMD/GPU. The gateway spine loads these as shared libraries or invokes them via a
thin Rust shim; they never import `substrate-core`.

### Go — services by merit

Use Go for **standalone services** at a clear process boundary when Go's strengths
outweigh unified in-process Rust.

**Criteria (majority should be true):**

1. **Service boundary** — separate deployable (sidecar, health aggregator, provider probe
   daemon) communicating over HTTP/gRPC or Unix socket, not an in-process adapter.
2. **Concurrency-heavy I/O** — many concurrent outbound connections, fan-out health checks,
   or long-lived watches where goroutines simplify the code without blocking the gateway
   tokio runtime.
3. **Operational fit** — static binary, simple cross-compile, or team ownership already in
   Go (e.g. OmniRoute-adjacent tooling).
4. **No port trait implementation** — Go services speak protocol DTOs; Rust gateway
   translates to/from `substrate-core` domain types at the boundary.

Go must not replace the spine or implement `RoutingPort`, `MailboxStore`, or other core
traits. Those remain Rust adapters in the workspace.

### Direct Rust — port reuse wins

Implement adapters **directly in Rust** when an existing port abstraction already covers
the concern.

**Criteria (any is sufficient):**

1. **Port reuse** — the component is an inbound driver or outbound adapter for a trait
   already defined in `substrate-core` (`RoutingPort`, `MailboxStore`, `EventStorePort`,
   etc.).
2. **No FFI overhead** — in-process call; hot path would pay serialization + boundary
   crossing for no gain.
3. **Workspace cohesion** — shares types with `store-sqlite`, `omniroute-adapter`, or
   `substrate-app`; a second language would duplicate contracts.
4. **Safety policy** — `#![forbid(unsafe_code)]` crate policy applies unless elevated by
   a follow-up ADR for a specific hot leaf.

Default rule: **if it implements a port, it is Rust.**

### Language selection checklist

Before introducing Zig, Mojo, or Go for a new gateway component:

1. Name the port(s) or protocol boundary.
2. Show profiling or complexity evidence for the language choice.
3. Confirm the component does not add adapter dependencies to `substrate-core`.
4. Document the FFI/protocol contract in the crate README or a follow-up ADR.

## Consequences

### Positive

- Spine remains one language, one build graph, one arch-test surface.
- Hot paths can be optimized without contaminating core with `unsafe` or FFI.
- Go sidecars can scale and deploy independently of the gateway release cycle.
- Existing Rust adapters (`store-sqlite`, `omniroute-adapter`, `a2a`) are reused with zero
  shim cost.

### Negative

- Multiple toolchains (Rust + optional Zig/Mojo/Go) increase CI and contributor onboarding
  cost.
- FFI boundaries require explicit ABI versioning and sanitizer coverage on hot leaves.
- Cross-language debugging is harder than pure Rust; distributed traces must span process
  boundaries for Go sidecars.

### Neutral

- `driver-http` remains the reference for non-gateway HTTP consumers; gateway follows the
  same axum patterns with additional routes.
- Python (`driver-mcp`) is unchanged; this ADR governs the gateway and its adjacency only.

## Alternatives considered

### 1. Rust-only gateway

Reject hot-leaf offload; implement all logic in Rust with `unsafe` where needed.

- **Pros:** Single toolchain, simplest CI.
- **Cons:** SIMD/GPU kernels become `unsafe` Rust or external processes anyway; p99
  latency harder to recover without duplicating effort Zig/Mojo already optimize for.

### 2. Go gateway spine

Implement the primary HTTP gateway in Go (like many API gateways).

- **Pros:** Strong concurrency story, familiar ops story.
- **Cons:** Duplicates `substrate-core` port graph in Go or pays CGO/RPC for every
  `RoutingPort` call; breaks hexagonal dependency rule enforcement; abandons existing
  `driver-http` / axum investment.

### 3. WASM plugins for hot paths

Load `.wasm` modules for leaf compute.

- **Pros:** Sandbox, portable, no CGO.
- **Cons:** SIMD support and startup latency vary; tooling immature for our SIMD-heavy
  targets; adds a runtime dependency on the gateway hot path.

### 4. Unrestricted polyglot (any language anywhere)

No spine rule; ports implementable in any language.

- **Pros:** Maximum flexibility.
- **Cons:** `arch-test` and `substrate-core` become unenforceable; composition root
  fragments; FFI overhead on every port call.

## References

- [README — Hexagonal architecture](../../README.md)
- Gateway crate: `crates/gateway` (OpenAI surface, A2A mount, management config)
- `crates/arch-test` — core dependency direction enforcement
