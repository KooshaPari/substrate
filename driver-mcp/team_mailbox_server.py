"""Teammate-facing MCP server: send/receive messages and manage tasks."""
from __future__ import annotations

import json
import os
import uuid
from datetime import datetime, timezone
from typing import Any

from fastmcp import FastMCP

from _db import open_db

mcp = FastMCP("team-mailbox")

# Team identity comes from environment
TEAM_ID = os.environ.get("SUBSTRATE_TEAM_ID", "default")
AGENT_NAME = os.environ.get("SUBSTRATE_AGENT_NAME", "unknown")

_db = open_db()

from _sanitize import sanitize_response as _sanitize_response


@mcp.tool()
def team_send(
    to: str,
    kind: str,
    text: str,
    artifacts: list[dict] | None = None,
    in_reply_to: str | None = None,
) -> dict[str, Any]:
    """Send a message to another agent in the team."""
    msg_id = str(uuid.uuid4())
    parts = [{"type": "text", "text": text}]
    if artifacts:
        for art in artifacts:
            parts.append({"type": "file", "uri": art.get("uri", "")})
    now = datetime.now(timezone.utc).isoformat()
    _db.execute(
        "INSERT INTO mailbox (id, team_id, task_id, from_agent, to_agent, kind, parts, in_reply_to, state, created_at) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'unread', ?)",
        (msg_id, TEAM_ID, None, AGENT_NAME, to, kind, json.dumps(parts), in_reply_to, now),
    )
    _db.commit()
    return _sanitize_response({"ok": True, "id": msg_id})


@mcp.tool()
def team_inbox() -> dict[str, Any]:
    """Fetch all unread messages addressed to this agent."""
    rows = _db.execute(
        "SELECT id, from_agent, kind, parts, in_reply_to, created_at "
        "FROM mailbox WHERE team_id=? AND to_agent=? AND state='unread' ORDER BY created_at ASC",
        (TEAM_ID, AGENT_NAME),
    ).fetchall()
    messages = [
        {
            "id": r[0],
            "from": r[1],
            "kind": r[2],
            "parts": json.loads(r[3]),
            "in_reply_to": r[4],
            "created_at": r[5],
        }
        for r in rows
    ]
    return _sanitize_response({"ok": True, "count": len(messages), "messages": messages})


@mcp.tool()
def task_create(title: str, parent: str | None = None) -> dict[str, Any]:
    """Create a new task in the team's tasklist."""
    task_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc).isoformat()
    _db.execute(
        "INSERT INTO tasklist (id, team_id, title, state, owner, parent_task_id, created_at, updated_at) "
        "VALUES (?, ?, ?, 'submitted', ?, ?, ?, ?)",
        (task_id, TEAM_ID, title, AGENT_NAME, parent, now, now),
    )
    _db.commit()
    return _sanitize_response({"ok": True, "id": task_id, "state": "submitted"})


@mcp.tool()
def task_update(task_id: str, state: str, note: str | None = None) -> dict[str, Any]:
    """Update a task's state."""
    now = datetime.now(timezone.utc).isoformat()
    _db.execute(
        "UPDATE tasklist SET state=?, updated_at=?, note=? WHERE id=? AND team_id=?",
        (state, now, note, task_id, TEAM_ID),
    )
    _db.commit()
    return _sanitize_response({"ok": True, "id": task_id, "state": state})


if __name__ == "__main__":
    mcp.run()
