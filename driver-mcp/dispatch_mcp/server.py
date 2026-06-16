from __future__ import annotations

import asyncio
import logging
import os
import signal
from collections.abc import Awaitable, Callable
from typing import cast

from fastmcp import FastMCP

from dispatch_mcp.adapters.omni_http import OmniHttpAdapter
from dispatch_mcp.core.port import Router
from dispatch_mcp.core.types import JobResult

mcp = FastMCP("dispatch-mcp")
_logger = logging.getLogger("dispatch_mcp")
_log_level = os.environ.get("LOG_LEVEL", "").upper()
if _log_level in ("DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"):
    _logger.setLevel(getattr(logging, _log_level, logging.WARNING))
logger = _logger

MAX_MESSAGE_LENGTH = 4096
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

_omniroute_url: str | None = os.environ.get("OMNIROUTE_URL")


class _RouterHolder:
    """Holder for lazy-loaded router instance."""

    _instance: Router | None = None

    @classmethod
    def get(cls) -> Router:
        """Get or create router instance (lazy-loaded to avoid import-time failure)."""
        if cls._instance is None:
            if not _omniroute_url:
                raise ValueError("OMNIROUTE_URL environment variable is required")
            cls._instance = OmniHttpAdapter(_omniroute_url)
        return cls._instance

    @classmethod
    def set(cls, router: Router) -> None:
        """Set router instance (for testing)."""
        cls._instance = router


def _get_router() -> Router:
    """Lazy-load router only when needed (avoids import-time failure during tests)."""
    return _RouterHolder.get()


def _make_dispatch(tier: str) -> Callable[[str], Awaitable[dict[str, str | bool | None]]]:
    @mcp.tool(name=f"dispatch_{tier}")
    async def dispatch(message: str) -> dict[str, str | bool | None]:
        if len(message.encode()) > MAX_MESSAGE_LENGTH:
            raise ValueError(f"message exceeds maximum length of {MAX_MESSAGE_LENGTH} bytes")
        router = _get_router()
        return cast(dict[str, str | bool | None], await router.dispatch(message=message, tier=tier))

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
async def dispatch_custom(tier: str, message: str) -> dict[str, str | bool | None]:
    if tier not in VALID_TIERS:
        raise ValueError(f"Invalid tier '{tier}'. Must be one of: {', '.join(sorted(VALID_TIERS))}")
    if len(message.encode()) > MAX_MESSAGE_LENGTH:
        raise ValueError(f"message exceeds maximum length of {MAX_MESSAGE_LENGTH} bytes")
    router = _get_router()
    return cast(dict[str, str | bool | None], await router.dispatch(tier=tier, message=message))


@mcp.tool()
async def dispatch_health() -> dict[str, str | bool | None]:
    """Check OmniRoute backend health. Requires OMNIROUTE_URL."""
    router = _get_router()
    return cast(dict[str, str | bool | None], await router.health())


@mcp.tool()
async def dispatch_liveness() -> dict[str, str | bool | None]:
    """Return server liveness status. Does not require OmniRoute."""
    return JobResult(status="alive", message="dispatch-mcp").to_dict()


def main() -> None:
    """Start the MCP server."""

    def _handle_signal(signum: int, frame: object) -> None:  # noqa: ARG001
        sig_name = signal.Signals(signum).name
        logger.warning("Received %s, closing OmniRoute client", sig_name)
        router = _get_router()
        asyncio.run(router.close())

    signal.signal(signal.SIGTERM, _handle_signal)
    signal.signal(signal.SIGINT, _handle_signal)
    mcp.run()
    router = _get_router()
    asyncio.run(router.close())


if __name__ == "__main__":
    main()
