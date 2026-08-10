# ADR-0003 — Canonical Error type in substrate-core

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** substrate-core maintainers
- **Supersedes:** none
- **Superseded by:** none

---

## Context

Each adapter (gateway, store-sqlite, engine-\*) starts as an island with
its own `anyhow::Error` or its own error enum. Errors that originate in
SQLite, an HTTP upstream, or a parsed JSON body bubble up as
`anyhow::Error` and lose their semantic shape by the time they reach an
HTTP handler. The HTTP layer must either pattern-match on `Display`
strings (brittle) or convert to `StatusCode` heuristically.

We need:
1. A single error type that knows which layer the error came from.
2. A way to add context without re-allocating.
3. An `IntoResponse` for axum handlers, but **only at the gateway layer**
   (core must stay web-framework agnostic).

## Decision

`substrate-core/src/error.rs` defines a single `pub enum Error` (thiserror)
that captures every well-known failure mode at the workspace boundary:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config: {0}")]
    Config(String),
    #[error("storage: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("transport: {0}")]
    Transport(#[from] std::io::Error),
    #[error("dispatch: {0}")]
    Dispatch(#[from] DispatchError),
    #[error("engine: {0}")]
    Engine(#[from] EngineError),
    #[error("context: {0}")]
    Context(#[from] anyhow::Error),
    #[error("upstream: {0}")]
    Upstream(#[from] UpstreamError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rate limited")]
    RateLimited { retry_after: Duration },
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error("circuit open: {0}")]
    CircuitOpen(String),
}
```

- Adapters implement `From<…> for Error` for their domain-specific error
  enums. Calls to `?` automatically convert.
- `.context("…")` (anyhow) is allowed and produces a layered `Error::Context`
  for diagnosis.
- **`Error` does NOT depend on axum, hyper, or tokio.** The conversion to
  HTTP status codes lives in `psub-gateway/src/error.rs::IntoResponse`.
  This keeps the core framework-agnostic.

## Consequences

**Positive**
- A single error type means callers can `match` exhaustively.
- `tracing::error!(error = %err, …)` produces uniform logs.
- Adding a new failure mode is a single enum variant change.

**Negative**
- The enum grows over time. We accept this; rustc will tell us which
  match arms became non-exhaustive.
- `From<anyhow::Error>` is a one-way highway back to dynamic context.
  We accept the trade.

## Alternatives considered

- **Plain `anyhow::Result<T>` everywhere.** Rejected: loses the
  semantic shape; HTTP layer can't pick a status code from the type.
- **`error-stack` crate.** Rejected: pulls in another dependency for
  marginal benefit over `thiserror` + `anyhow::context`.
- **Per-adapter error enums with conversion traits.** Rejected: 6 enums
  to maintain, 36 conversion impls, no canonical dispatch.

## References

- `crates/substrate-core/src/error.rs` — the canonical `Error`.
- `crates/psub-gateway/src/lib.rs` — `IntoResponse` impl (gateway-only).
- `CONTRIBUTING.md` §7 — error context convention.
