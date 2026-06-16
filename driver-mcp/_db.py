"""Thin SQLite wrapper shared by team_mailbox_server and lead_server."""
from __future__ import annotations

import sqlite3
import os
from typing import Any


def get_db_path() -> str:
    path = os.environ.get("SUBSTRATE_DB", "substrate.db")
    return path


def open_db() -> sqlite3.Connection:
    conn = sqlite3.connect(get_db_path(), check_same_thread=False)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    _ensure_schema(conn)
    return conn


def _ensure_schema(conn: sqlite3.Connection) -> None:
    conn.executescript("""
        CREATE TABLE IF NOT EXISTS mailbox (
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
    """)
    conn.commit()
