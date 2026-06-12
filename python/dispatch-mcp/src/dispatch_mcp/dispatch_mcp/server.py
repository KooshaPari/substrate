from __future__ import annotations

import json
import logging
import os
import signal
from collections.abc import Callable
from typing import Any
from urllib.parse import urlparse

import httpx
from fastmcp import FastMCP

mcp = FastMCP("dispatch-mcp")
_logger = logging.getLogger("dispatch_mcp")
_log_level = os.environ.get("LOG_LEVEL", "").upper()
if _log_level in ("DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"):
    _logger.setLevel(getattr(logging, _log_level, logging.WARNING))
# NOTE: Do not add DEBUG-level logging of tool arguments (message, tier, payload).
# Dispatch messages may contain sensitive context. If DEBUG is needed for
# troubleshooting, prefer logging route and timing only, never the payload content.
logger = _logger

# Shared httpx client for OmniRoute — reuses connections across dispatch calls.
# Thread-safe for FastMCP's single-threaded async event loop.
_client: httpx.Client = httpx.Client(
    timeout=10,
    follow_redirects=False,
    limits=httpx.Limits(keepalive_expiry=60),
)

MAX_MESSAGE_LENGTH = 4096  # bytes — prevents unbounded payload to OmniRoute
MAX_RESPONSE_LENGTH = 1024 * 1024  # bytes — prevents unbounded response from OmniRoute

# Allowlist of safe keys a dispatch tool may return to the MCP client.
# OmniRoute may include internal details (hostnames, stack traces, etc.) under
# other keys — those are stripped before passing the response up.
_ALLOWED_RESPONSE_KEYS = frozenset({"ok", "tier", "message", "status", "error"})


def _sanitize_response(response: dict[str, Any]) -> dict[str, Any]:
    """Strip non-allowlisted keys from OmniRoute responses before passing up."""
    return {k: v for k, v in response.items() if k in _ALLOWED_RESPONSE_KEYS}


# Allowlist of valid dispatch tiers — dispatch_custom must use one of these.
VALID_TIERS = frozenset(
    {
        "worker",
        "main",
        "codeman",
        "freetier",
        "kimi",
        "kimi_thinking",
        "minimax",
        "opus",
        "haiku",
        "gemini",
    }
)


def _call_omniroute(route: str, payload: dict[str, Any]) -> dict[str, Any]:
    base = os.environ.get("OMNIROUTE_URL")
    if not base:
        raise ValueError(
            "OMNIROUTE_URL environment variable is not set. "
            "Set it to the base URL of the dispatch backend before starting the server."
        )
    parsed = urlparse(base)
    if parsed.scheme not in ("http", "https"):
        raise ValueError(f"OMNIROUTE_URL must use http or https scheme, got: {parsed.scheme!r}")
    try:
        response = _client.post(f"{base.rstrip('/')}/{route.lstrip('/')}", json=payload)
        response.raise_for_status()
        body_size = len(response.content)
        if body_size > MAX_RESPONSE_LENGTH:
            raise RuntimeError(
                f"OmniRoute response body ({body_size} bytes) exceeds "
                f"maximum allowed size ({MAX_RESPONSE_LENGTH} bytes) for route '{route}'"
            )
        return _sanitize_response(response.json())
    except httpx.TimeoutException as e:
        logger.error("OmniRoute timeout for route %s: %s", route, e)
        raise
    except httpx.HTTPStatusError as e:
        logger.error(
            "OmniRoute HTTP error %s for route %s: %s",
            e.response.status_code,
            route,
            e,
        )
        raise
    except httpx.RequestError as e:
        logger.error("OmniRoute request error for route %s: %s", route, e)
        raise
    except json.JSONDecodeError as e:
        logger.error(
            "OmniRoute returned non-JSON response for route %s: %s",
            route,
            e,
        )
        raise RuntimeError(f"OmniRoute returned an invalid response for route '{route}'") from e


def _make_dispatch(tier: str) -> Callable[[str], dict[str, Any]]:
    @mcp.tool(name=f"dispatch_{tier}")
    def dispatch(message: str) -> dict[str, Any]:
        if len(message.encode()) > MAX_MESSAGE_LENGTH:
            raise ValueError(f"message exceeds maximum length of {MAX_MESSAGE_LENGTH} bytes")
        return _call_omniroute("dispatch", {"tier": tier, "message": message})

    return dispatch


dispatch_worker = _make_dispatch("worker")
dispatch_main = _make_dispatch("main")
dispatch_codeman = _make_dispatch("codeman")
dispatch_freetier = _make_dispatch("freetier")
dispatch_kimi = _make_dispatch("kimi")
dispatch_kimi_thinking = _make_dispatch("kimi_thinking")
dispatch_minimax = _make_dispatch("minimax")
dispatch_opus = _make_dispatch("opus")
dispatch_haiku = _make_dispatch("haiku")
dispatch_gemini = _make_dispatch("gemini")


@mcp.tool()
def dispatch_custom(tier: str, message: str) -> dict[str, Any]:
    if tier not in VALID_TIERS:
        raise ValueError(f"Invalid tier '{tier}'. Must be one of: {', '.join(sorted(VALID_TIERS))}")
    if len(message.encode()) > MAX_MESSAGE_LENGTH:
        raise ValueError(f"message exceeds maximum length of {MAX_MESSAGE_LENGTH} bytes")
    return _call_omniroute("dispatch", {"tier": tier, "message": message})


@mcp.tool()
def dispatch_health() -> dict[str, Any]:
    """Check OmniRoute backend health. Requires OMNIROUTE_URL."""
    return _call_omniroute("health", {})


@mcp.tool()
def dispatch_liveness() -> dict[str, Any]:
    """Return server liveness status. Does not require OmniRoute."""
    return {"status": "alive", "server": "dispatch-mcp"}


def main() -> None:
    """Start the MCP server. Registers SIGTERM/SIGINT handlers that log intent;
    the event loop (mcp.run) controls its own lifecycle and does not
    guarantee immediate interruption on signal receipt."""

    def _handle_signal(signum: int, _frame: object) -> None:
        sig_name = signal.Signals(signum).name
        logger.warning("Received %s, closing OmniRoute client", sig_name)
        _client.close()

    signal.signal(signal.SIGTERM, _handle_signal)
    signal.signal(signal.SIGINT, _handle_signal)
    mcp.run()
    _client.close()


if __name__ == "__main__":
    main()
