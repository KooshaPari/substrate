"""Shared response sanitizer for MCP tools."""
from __future__ import annotations

from typing import Any

ALLOWED_KEYS = frozenset(
    {
        "ok",
        "id",
        "state",
        "count",
        "messages",
        "tasks",
        "error",
        # dispatch / plan / route fields
        "text",
        "status",
        "artifacts",
        "pr_urls",
        "engine",
        "session_mode",
        "argv",
        "spec",
        "model",
        "reason",
    }
)


def sanitize_response(response: dict[str, Any]) -> dict[str, Any]:
    """Strip non-allowlisted keys before returning to MCP client."""
    return {k: v for k, v in response.items() if k in ALLOWED_KEYS}
