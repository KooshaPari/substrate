# ADR-0006 — Structured tracing via a `TracePort`

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** substrate-trace maintainers
- **Supersedes:** none
- **Superseded by:** none

---

## Context

We need three layers of observability:

1. **Per-task lifecycle events** — `TaskRegistered`, `TaskCompleted`,
   `TaskFailed`. These are domain events with structured payloads. They
   must work without a network.
2. **Per-request timing/logging** — `tracing` spans around I/O and
   dispatch boundaries.
3. **Cross-process correlation** — request-id propagation between
   inbound (gateway) and outbound (engine).

The naïve answer is "just `println!` everywhere" or "just use
`tracing::info!` everywhere". But we want **the application layer to be
unaware of the trace backend**, just like it is unaware of the storage
backend (ADR-0005).

## Decision

We adopt a two-tier observability model:

### Tier 1: domain events via `TracePort`

`substrate-core/src/trace.rs` defines:

```rust
#[async_trait]
pub trait TracePort: Send + Sync {
    async fn task_registered(&self, event: TaskRegistered);
    async fn task_completed(&self, event: TaskCompleted);
    async fn task_failed(&self, event: TaskFailed);
}
```

Concrete adapters in `substrate-trace`:

| Adapter | Backed by |
|---|---|
| `NoopTrace` | discards everything (default) |
| `RecordingTrace` | `Arc<Mutex<Vec<TraceEvent>>>` for tests |
| `MultiTrace` | fans events to N traces |
| `AgilePlusTrace` | POST to AgilePlus API |
| `TraceraTrace` | POST to Tracera API |

`AppState` (composition root, `driver-http/src/lib.rs`) wires one in.
The application layer never names a concrete trace.

### Tier 2: request-level spans via `tracing`

`psub-gateway/src/lib.rs` and `driver-http/src/lib.rs` install a
`tracing_subscriber::fmt` subscriber and add `#[tracing::instrument]`
to handlers. The gateway uses `tower_http::trace::TraceLayer` for
timing.

The `tracing` crate is added as a workspace dep:

```
tracing = "0.1"   # Cargo.toml:99
```

A future enhancement (P1.3) will plug in OpenTelemetry via `tracing-otel`.

## Consequences

**Positive**
- Application code is trace-backend agnostic.
- Domain events flow over the same type system as storage (port + impl).
- Tests can assert on `RecordingTrace::events()` without a network.
- Operator can wire `AgilePlusTrace` or `TraceraTrace` (or both via
  `MultiTrace`) without recompiling the binary.

**Negative**
- Two APIs in play (`TracePort` and `tracing`). We accept: domain
  events are structured, request logs are stream-of-text.
- `TracePort` is async, so a sync caller must `.await` it. This is
  already true of every other port (DispatchApi, RoutingPort).

## Alternatives considered

- **One API only (`tracing` macro everywhere).** Rejected: domain
  events become entangled with the backend; tests can't assert
  without a subscriber.
- **NoTrace.** Rejected: we lose the structured `TaskRegistered` →
  `TaskCompleted` correlation per task id.
- **Custom channel + worker.** Rejected: would require rebuilding
  what `tracing` already gives us for the request layer.

## References

- `crates/substrate-core/src/trace.rs` — `TracePort`, `TaskRegistered`,
  `TaskCompleted`, `TaskFailed`.
- `crates/substrate-trace/src/` — concrete adapters.
- `crates/driver-http/src/lib.rs` — composition root wiring.
- `crates/psub-gateway/src/lib.rs` — `tracing` integration.
- ADR-0002 — port/adapter pattern; this ADR is its observability twin.
