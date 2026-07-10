# ADR-0005 — SQLite as the default store (rusqlite, bundled)

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** store-sqlite maintainers
- **Supersedes:** none
- **Superseded by:** none

---

## Context

The substrate needs a persistent store. Options:

| Candidate | Pros | Cons |
|---|---|---|
| `rusqlite` | direct SQL, zero network, embedded | no async story, manual migration |
| `diesel` | type-safe DSL, async via `diesel-async` | larger compile cost, schema-first coupling |
| `sqlx` | async, compile-time-checked queries | ties us to async runtime; compile-time macros add 1m+ to dev builds |
| `sled` | pure Rust, embedded | single-writer; long-term maintenance concerns |
| `redb` | pure Rust, ACID | early-stage; schema migration story is thin |
| Postgres | mature, multi-writer | adds a runtime dep; embeds poorly |
| RocksDB | very fast | C++ dep; binary bloat; overkill for our QPS |

We need:

- Embedded (no runtime server)
- Async-friendly adapter surface (the gateway is async)
- Schema migrations
- Bounded binary size

## Decision

We pick **`rusqlite`** with the **`bundled`** feature.

- `rusqlite = { version = "0.40", features = ["bundled"] }` (Cargo.toml:84).
- Migrations live as numbered SQL files under
  `crates/store-sqlite/migrations/`, applied by
  `crates/store-sqlite/src/migrate.rs` at startup.
- A thin `tokio::task::spawn_blocking` wrapper
  (`crates/store-sqlite/src/async_bridge.rs`) bridges async callers to
  the synchronous rusqlite API. This is the same pattern
  `tokio-postgres` uses under the hood for fallback paths.

The **`bundled` feature** is mandatory in production builds: it pins the
exact SQLite version, removes the system-libsqlite dependency, and makes
binary distribution reproducible.

## Consequences

**Positive**
- Single dependency: no Postgres / MySQL / etc. to deploy.
- Predictable performance (no network jumps).
- WAL mode gives us concurrent readers + a single writer.
- Migrations are transparent SQL files — auditable in `git diff`.

**Negative**
- We must accept that "scale out" means "shard the SQLite file", not
  "add another replica". This is fine for substrate's envelope
  (dispatch is naturally single-leader; see ADR-0004).
- `spawn_blocking` adds a small overhead per call. Acceptable for the
  read-heavy dispatch path.

## Alternatives considered

- **`sqlx`** — rejected for compile-time macros in CI; would add ~10
  minutes to the test wall-clock.
- **`diesel` + `diesel-async`** — rejected for compile-time penalty
  and for forcing an async-only schema design.
- **Postgres** — rejected because runtime deps defeat the embeddable
  story substrate was designed for.

## References

- `Cargo.toml:84` — `rusqlite = "0.40" features = ["bundled"]`.
- `crates/store-sqlite/src/` — store + migrate + async_bridge.
- ADR-0004 — atomic claim built on the same SQLite file.
- ADR-0002 — ports/adapters pattern lets us swap `SqliteMailboxStore`
  out without rewriting dispatch.
