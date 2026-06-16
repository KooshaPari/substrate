"""Health, readiness, and metrics endpoints for dispatch MCP."""

from __future__ import annotations

import os
import time
from typing import Any
from urllib.parse import urlparse

import httpx

_START_TIME = time.monotonic()


def liveness() -> dict[str, Any]:
    """Return process liveness status."""
    return {
        "status": "alive",
        "server": "dispatch-mcp",
        "uptime_seconds": round(time.monotonic() - _START_TIME, 3),
    }


def readiness(*, check_omniroute: bool = False, timeout: float = 2.0) -> dict[str, Any]:
    """Return readiness status, including a dependency check."""
    checks: dict[str, dict[str, Any]] = {}
    overall_ok = True

    base = os.environ.get("OMNIROUTE_URL", "")
    if not base:
        checks["omniroute_url"] = {
            "ok": False,
            "detail": "OMNIROUTE_URL environment variable is not set",
        }
        overall_ok = False
    else:
        parsed = urlparse(base)
        if parsed.scheme not in ("http", "https"):
            checks["omniroute_url"] = {
                "ok": False,
                "detail": f"OMNIROUTE_URL must use http or https scheme, got: {parsed.scheme!r}",
            }
            overall_ok = False
        else:
            checks["omniroute_url"] = {"ok": True, "scheme": parsed.scheme, "host": parsed.hostname}

    if check_omniroute and overall_ok:
        try:
            response = httpx.get(
                f"{base.rstrip('/')}/health",
                timeout=timeout,
                follow_redirects=False,
            )
            response.raise_for_status()
            checks["omniroute_reachable"] = {"ok": True, "status_code": response.status_code}
        except (httpx.HTTPError, httpx.RequestError) as exc:
            checks["omniroute_reachable"] = {"ok": False, "detail": str(exc)}
            overall_ok = False

    return {
        "status": "ready" if overall_ok else "not_ready",
        "server": "dispatch-mcp",
        "checks": checks,
    }


def metrics() -> str:
    """Return metrics in Prometheus text exposition format."""
    lines = [
        "# HELP dispatch_mcp_up 1 if the dispatch-mcp process is up, 0 otherwise.",
        "# TYPE dispatch_mcp_up gauge",
        "dispatch_mcp_up 1",
        "# HELP dispatch_mcp_uptime_seconds Seconds since the dispatch-mcp process started.",
        "# TYPE dispatch_mcp_uptime_seconds gauge",
        f"dispatch_mcp_uptime_seconds {round(time.monotonic() - _START_TIME, 3)}",
        "# HELP dispatch_mcp_dispatches_total Number of dispatch calls handled since process start.",
        "# TYPE dispatch_mcp_dispatches_total counter",
        "dispatch_mcp_dispatches_total 0",
        "# HELP dispatch_mcp_dispatch_errors_total Number of dispatch calls that failed since process start.",
        "# TYPE dispatch_mcp_dispatch_errors_total counter",
        "dispatch_mcp_dispatch_errors_total 0",
    ]
    return "\n".join(lines) + "\n"
