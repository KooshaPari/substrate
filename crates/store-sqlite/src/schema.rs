//! SQLite schema initialization.

use rusqlite::{Connection, Result};

/// Initialize the database schema (idempotent via `CREATE TABLE IF NOT EXISTS`).
pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mailbox (
            id          TEXT PRIMARY KEY,
            team_id     TEXT NOT NULL,
            task_id     TEXT,
            from_agent  TEXT NOT NULL,
            to_agent    TEXT NOT NULL,
            kind        TEXT NOT NULL,
            parts       TEXT NOT NULL,
            in_reply_to TEXT,
            state       TEXT NOT NULL DEFAULT 'unread',
            created_at  TEXT NOT NULL,
            consumed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_mailbox_to ON mailbox(team_id, to_agent, state);

        CREATE TABLE IF NOT EXISTS tasklist (
            id              TEXT PRIMARY KEY,
            team_id         TEXT NOT NULL,
            title           TEXT NOT NULL,
            state           TEXT NOT NULL DEFAULT 'submitted',
            owner           TEXT NOT NULL,
            parent_task_id  TEXT,
            requirement_id  TEXT,
            epic_id         TEXT,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL,
            note            TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_tasklist_team ON tasklist(team_id);

        CREATE TABLE IF NOT EXISTS work_queue (
            id          TEXT PRIMARY KEY,
            queue       TEXT NOT NULL,
            body        TEXT NOT NULL,
            state       TEXT NOT NULL DEFAULT 'pending',
            claimed_by  TEXT,
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_work_queue_pending ON work_queue(queue, state, created_at);

        CREATE TABLE IF NOT EXISTS memory (
            id          TEXT PRIMARY KEY,
            mem_key     TEXT NOT NULL,
            content     TEXT NOT NULL,
            created_at  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_memory_created ON memory(created_at DESC);

        CREATE TABLE IF NOT EXISTS event_log (
            aggregate_id  TEXT NOT NULL,
            aggregate_seq INTEGER NOT NULL,
            global_seq    INTEGER NOT NULL,
            payload       TEXT NOT NULL,
            occurred_at   INTEGER NOT NULL,
            PRIMARY KEY (aggregate_id, aggregate_seq)
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_event_log_global ON event_log(global_seq);
        CREATE INDEX IF NOT EXISTS idx_event_log_aggregate ON event_log(aggregate_id, aggregate_seq);",
    )
}
