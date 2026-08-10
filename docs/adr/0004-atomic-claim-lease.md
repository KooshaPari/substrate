# ADR-0004 — Atomic claim/lease for the single-instance guard

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** substrate-serve-lock maintainers
- **Supersedes:** none
- **Superseded by:** none

---

## Context

A worker that uses the SQLite mailbox as its dispatch source must ensure
no second instance is reading or writing the same mailbox concurrently
(at least, not under cooperative single-leader semantics). Otherwise:

- Two dispatchers race for the same `Task` row.
- The `execute` → `complete` cycle is no longer atomic across instances.
- Trace events become non-deterministic.

A naïve file lock (`flock`) works on Linux but breaks on macOS when the
lock holder is on a network mount, and on Windows it is POSIX-only.

## Decision

`substrate-serve-lock` implements the claim using **SQLite + WAL + a
single-row `claim` table** rather than a file lock:

```sql
CREATE TABLE claim (
    id          INTEGER PRIMARY KEY,
    holder_id   TEXT    NOT NULL,
    acquired_at INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL
);
```

The lease is **TTL-based** (`expires_at = acquired_at + 30s`). The holder
must renew it every 10s by writing a new `expires_at`. A second instance
that starts while the first holds a lease will:

1. Attempt `INSERT … ON CONFLICT (id) DO UPDATE SET … WHERE expires_at <
   now`. This succeeds iff the prior holder's lease has expired.
2. If the INSERT succeeds, it becomes the holder and writes its
   `holder_id`. The previous holder's next renew-write will fail the
   WHERE clause, signalling it should exit.

The atomic guarantee comes from `rusqlite`'s per-database write
serialization (`SQLITE_OPEN_FULLMUTEX`). WAL mode gives us read
concurrency.

## Consequences

**Positive**
- Cross-platform: SQLite is the only dependency.
- The lease TTL bounds the worst-case split-brain interval to 30s.
- The same SQLite file is the mailbox store, so the claim and the
  mailbox are **one transactional unit** — we never mail a task to a
  dead holder.

**Negative**
- We accept a 30s split-brain window. Acceptable because dispatch is
  idempotent and the mailbox marks each task with its claimed holder id.
- `rusqlite` is mandatory (already a workspace dependency; ADR-0005).

## Alternatives considered

- **`flock` only.** Rejected: macOS / Windows / NFS corner cases.
- **`fcntl` POSIX advisory lock.** Rejected: same portability concerns.
- **etcd / consul lease.** Rejected: adds a runtime dependency for a
  workload that already needs SQLite.
- **Single-shot `INSERT … ON CONFLICT` without TTL.** Rejected: dead
  holder leaves the system permanently wedged.

## References

- `crates/substrate-serve-lock/src/` — implementation.
- `Cargo.toml:84` — `rusqlite = { version = "0.40", features = ["bundled"] }`
  (ADR-0005).
- ADR-0005 — SQLite as the default store.
