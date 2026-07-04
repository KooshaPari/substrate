# Supervisor durability contract

The `supervisor` crate manages one teammate lane: it starts an engine session,
records a team task, reads that agent's durable mailbox, and resumes the engine
with claimed messages. The durable backing store today is
`store_sqlite::SqliteMailboxStore`.

## Task lock semantics

Mailbox delivery uses a compare-and-set claim on the SQLite `mailbox` row:

1. A sender posts a row with `state='unread'`.
2. A supervisor calls `inbox(team_id, agent_name)` and receives unread rows in
   `created_at` order.
3. Before doing engine work, the supervisor calls `claim(message_id)`.
4. `claim` executes `UPDATE mailbox SET state='delivered' WHERE id=? AND
   state='unread'` and returns `true` only when exactly one row changed.
5. If two supervisors race for the same message, only one update can observe the
   `unread` state. The winner proceeds; the loser receives `ClaimConflict` and
   must not process the message.
6. After the engine resume succeeds, the winner calls `consume(message_id)`,
   which changes the row to `state='consumed'` and sets `consumed_at`.

This is an at-most-once processing lock. It prevents duplicate processing under
contention, but it does not currently lease or automatically requeue messages
that were claimed and then abandoned before `consume`.

## Mailbox durability

With `SqliteMailboxStore::open(path)`, the following state is durable across a
process crash or restart:

- `mailbox` rows posted by `team_send` or `MailboxStore::post`.
- Message state transitions: `unread`, `delivered`, and `consumed`.
- `consumed_at` timestamps.
- `tasklist` rows created for supervisor lanes.
- Task state transitions made through `task_update`.

The following state is not durable:

- In-memory `Supervisor` fields before recovery (`conv_id`, `task_id`).
- In-flight engine process state outside the engine's own resume mechanism.
- A claimed-but-unconsumed mailbox message's original `unread` visibility.

Use `open_in_memory()` only for tests that do not need crash durability.

## Recovery contract

`Supervisor::spawn(prompt)` starts the engine, wires the mailbox, creates a
tasklist row with title `spawn:<conv_id>`, and marks that task `working`.

After a crash or restart, construct a new supervisor with the same `team_id`,
`agent_name`, and SQLite database, then call `recover_active()` before pumping:

```rust
let mut supervisor = Supervisor::new(engine, store, LaneConfig::new(team, agent));
let recovered = supervisor.recover_active()?;
if recovered {
    supervisor.pump_loop(100).await?;
}
```

Recovery reads the durable `tasklist`, filters to rows owned by this agent whose
title starts with `spawn:`, and selects the newest task in `submitted`,
`working`, or `input_required` state. It restores the supervisor's in-memory
`conv_id` and `task_id` from that row. The next `pump_one` then resumes the
engine using the recovered conversation id and processes unread mailbox rows
from SQLite.

Recovery does not replay an event log and does not restart an engine from a raw
conversation dump. It relies on the engine adapter's `resume(conv_id, prompt)`
support for the recovered conversation id.

## MCP tools and tables

The local MCP tools use the same SQLite schema:

- `team_send` inserts `unread` rows into `mailbox`.
- `team_inbox` lists `unread` rows for the configured `SUBSTRATE_TEAM_ID` and
  `SUBSTRATE_AGENT_NAME`.
- `task_list` reads rows from `tasklist`.

The Rust supervisor and Python MCP tools are interoperable as long as they point
at the same `SUBSTRATE_DB` file.
