"""Substrate MCP server: dispatch/plan/route over HTTP plus team mailbox tools."""
from __future__ import annotations

import json
import os
import uuid
from datetime import datetime, timezone
from typing import Any

from fastmcp import FastMCP

from _db import open_db
import _http
from _sanitize import sanitize_response

mcp = FastMCP("substrate")

TEAM_ID = os.environ.get("SUBSTRATE_TEAM_ID", "default")
AGENT_NAME = os.environ.get("SUBSTRATE_AGENT_NAME", "lead")

_db: Any = None


def _get_db():
    global _db
    if _db is None:
        _db = open_db()
    return _db


def _prompt_body(
    prompt: str,
    *,
    engine: str | None = None,
    cwd: str | None = None,
    mode: str | None = None,
) -> dict[str, Any]:
    body: dict[str, Any] = {
        "prompt": prompt,
        "cwd": cwd or os.getcwd(),
    }
    if engine is not None:
        body["engine"] = engine
    if mode is not None:
        body["mode"] = mode
    return body


def _validate_prompt(prompt: str, cwd: str | None) -> str | None:
    if not prompt or not prompt.strip():
        return "prompt must not be empty"
    resolved = cwd or os.getcwd()
    if not resolved or not str(resolved).strip():
        return "cwd must not be empty"
    return None


@mcp.tool()
def substrate_dispatch(
    prompt: str,
    engine: str | None = None,
    cwd: str | None = None,
    mode: str | None = None,
) -> dict[str, Any]:
    """Dispatch a prompt to substrate via the HTTP API (spawns engine)."""
    err = _validate_prompt(prompt, cwd)
    if err:
        return sanitize_response({"error": err})
    body = _prompt_body(prompt, engine=engine, cwd=cwd, mode=mode)
    return sanitize_response(_http.post_json("/v1/dispatch", body))


@mcp.tool()
def substrate_plan(
    prompt: str,
    engine: str | None = None,
    cwd: str | None = None,
) -> dict[str, Any]:
    """Dry-run: return the dispatch plan without spawning an engine."""
    err = _validate_prompt(prompt, cwd)
    if err:
        return sanitize_response({"error": err})
    body = _prompt_body(prompt, engine=engine, cwd=cwd)
    return sanitize_response(_http.post_json("/v1/plan", body))


@mcp.tool()
def substrate_route(task: dict[str, Any]) -> dict[str, Any]:
    """Route a task dict through substrate's routing port."""
    if not isinstance(task, dict):
        return sanitize_response({"error": "task must be an object"})
    prompt = str(task.get("prompt", ""))
    cwd = str(task.get("cwd", "") or os.getcwd())
    if not prompt.strip():
        return sanitize_response({"error": "task.prompt must not be empty"})
    if not cwd.strip():
        return sanitize_response({"error": "task.cwd must not be empty"})
    return sanitize_response(_http.post_json("/v1/route", {"task": task}))


@mcp.tool()
def team_send(
    to: str,
    kind: str,
    text: str,
    artifacts: list[dict] | None = None,
    in_reply_to: str | None = None,
) -> dict[str, Any]:
    """Send a message to a team member."""
    msg_id = str(uuid.uuid4())
    parts = [{"type": "text", "text": text}]
    if artifacts:
        for art in artifacts:
            parts.append({"type": "file", "uri": art.get("uri", "")})
    now = datetime.now(timezone.utc).isoformat()
    _get_db().execute(
        "INSERT INTO mailbox (id, team_id, task_id, from_agent, to_agent, kind, parts, in_reply_to, state, created_at) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'unread', ?)",
        (msg_id, TEAM_ID, None, AGENT_NAME, to, kind, json.dumps(parts), in_reply_to, now),
    )
    _get_db().commit()
    return sanitize_response({"ok": True, "id": msg_id})


@mcp.tool()
def team_inbox() -> dict[str, Any]:
    """Fetch all unread messages addressed to this agent."""
    rows = _get_db().execute(
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
    return sanitize_response({"ok": True, "count": len(messages), "messages": messages})


@mcp.tool()
def task_list() -> dict[str, Any]:
    """List all tasks in the team's tasklist."""
    rows = _get_db().execute(
        "SELECT id, title, state, owner, parent_task_id, created_at, updated_at "
        "FROM tasklist WHERE team_id=? ORDER BY created_at ASC",
        (TEAM_ID,),
    ).fetchall()
    tasks = [
        {
            "id": r[0],
            "title": r[1],
            "state": r[2],
            "owner": r[3],
            "parent_task_id": r[4],
            "created_at": r[5],
            "updated_at": r[6],
        }
        for r in rows
    ]
    return sanitize_response({"ok": True, "count": len(tasks), "tasks": tasks})


if __name__ == "__main__":
    mcp.run()
