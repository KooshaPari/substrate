"""Offline tests for tier-based OmniRoute dispatch MCP."""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest
import respx

from dispatch_mcp import health, server
from dispatch_mcp.adapters.omni_http import OmniHttpAdapter
from dispatch_mcp.core.types import JobResult


def run_async(result: object) -> object:
    return asyncio.run(result)


class _FakeRouter:
    async def dispatch(self, message: str, tier: str, payload: dict | None = None) -> dict:  # noqa: ARG002
        return {"ok": True, "tier": tier, "message": message}


class TestJobResult:
    def test_to_dict_omits_none_values(self) -> None:
        result = JobResult(ok=True, message="ok")
        assert result.to_dict() == {"ok": True, "message": "ok"}


class TestDispatchCustom:
    def test_dispatch_custom_success(self) -> None:
        mock_router = AsyncMock()
        mock_router.dispatch = AsyncMock(
            return_value={"ok": True, "tier": "worker", "message": "hello"}
        )

        with patch.object(server._RouterHolder, "get", return_value=mock_router):
            result = run_async(server.dispatch_custom("worker", "hello"))
            mock_router.dispatch.assert_awaited_once_with(tier="worker", message="hello")
            assert result == {"ok": True, "tier": "worker", "message": "hello"}

    def test_dispatch_custom_propagates_router_error(self) -> None:
        mock_router = AsyncMock()
        mock_router.dispatch = AsyncMock(side_effect=RuntimeError("boom"))

        with patch.object(server._RouterHolder, "get", return_value=mock_router):
            with pytest.raises(RuntimeError, match="boom"):
                run_async(server.dispatch_custom("main", "test"))

    def test_invalid_tier_raises(self) -> None:
        with pytest.raises(ValueError, match="Invalid tier 'rogue'"):
            run_async(server.dispatch_custom("rogue", "test"))

    def test_missing_omniroute_url_bubbles_from_router(self) -> None:
        mock_router = AsyncMock()
        mock_router.dispatch = AsyncMock(side_effect=ValueError("OMNIROUTE_URL missing"))

        with patch.object(server._RouterHolder, "get", return_value=mock_router):
            with pytest.raises(ValueError, match="OMNIROUTE_URL"):
                run_async(server.dispatch_custom("worker", "test"))


class TestDispatchHealth:
    def test_dispatch_health_success(self) -> None:
        mock_router = AsyncMock()
        mock_router.health = AsyncMock(return_value={"status": "ok"})

        with patch.object(server._RouterHolder, "get", return_value=mock_router):
            result = run_async(server.dispatch_health())
            mock_router.health.assert_awaited_once_with()
            assert result == {"status": "ok"}


class TestNamedDispatchTools:
    @pytest.mark.parametrize(
        ("tool_func", "tier"),
        [
            ("dispatch_worker", "worker"),
            ("dispatch_main", "main"),
            ("dispatch_codeman", "codeman"),
            ("dispatch_freetier", "freetier"),
            ("dispatch_kimi", "kimi"),
            ("dispatch_kimi_thinking", "kimi_thinking"),
            ("dispatch_minimax", "minimax"),
            ("dispatch_opus", "opus"),
            ("dispatch_haiku", "haiku"),
            ("dispatch_gemini", "gemini"),
        ],
    )
    def test_named_tool_exists_and_callable(self, tool_func: str, tier: str) -> None:
        mock_router = AsyncMock()
        mock_router.dispatch = AsyncMock(
            return_value={"ok": True, "tier": tier, "message": "hello"}
        )

        with patch.object(server._RouterHolder, "get", return_value=mock_router):
            func = getattr(server, tool_func)
            assert callable(func)
            result = run_async(func("hello"))
            mock_router.dispatch.assert_awaited_once_with(message="hello", tier=tier)
            assert result == {"ok": True, "tier": tier, "message": "hello"}

    def test_dispatch_worker_rejects_oversized_message(self) -> None:
        mock_router = AsyncMock()

        with patch.object(server._RouterHolder, "get", return_value=mock_router):
            oversized = "x" * (4096 + 1)
            with pytest.raises(ValueError, match="exceeds maximum length"):
                run_async(server.dispatch_worker(oversized))
            mock_router.dispatch.assert_not_called()


class TestDispatchLiveness:
    def test_dispatch_liveness_returns_status(self) -> None:
        result = run_async(server.dispatch_liveness())
        assert result == {"status": "alive", "message": "dispatch-mcp"}


class TestOmniHttpAdapter:
    @respx.mock
    def test_dispatch_returns_dict(self) -> None:
        route = respx.post("https://omni.example/dispatch").mock(
            return_value=httpx.Response(200, json={"ok": True, "status": "queued"})
        )
        adapter = OmniHttpAdapter("https://omni.example")

        result = run_async(adapter.dispatch("hello", "worker", {"priority": "high"}))

        assert route.called
        assert isinstance(result, dict)
        assert result == {"ok": True, "status": "queued"}

    @respx.mock
    def test_health_handles_200(self) -> None:
        route = respx.get("https://omni.example/health").mock(
            return_value=httpx.Response(200, json={"status": "ok"})
        )
        adapter = OmniHttpAdapter("https://omni.example")

        result = run_async(adapter.health())

        assert route.called
        assert result["status"] == "ok"

    def test_cancel_raises_not_implemented(self) -> None:
        adapter = OmniHttpAdapter("https://omni.example")

        with pytest.raises(NotImplementedError, match="cancel"):
            adapter.cancel("req-123")

    def test_sanitize_response_strips_internal_keys(self) -> None:
        response = {
            "ok": True,
            "tier": "worker",
            "message": "hello",
            "internal_host": "omniroute-1.internal",
            "stack_trace": "Traceback (most recent call last):\n  ...",
            "db_password": "hunter2",
        }
        result = OmniHttpAdapter("http://localhost:20128")._sanitize_response(response)
        assert result == {"ok": True, "tier": "worker", "message": "hello"}


class TestHealthModule:
    def test_liveness_returns_alive(self) -> None:
        result = health.liveness()
        assert result["status"] == "alive"
        assert result["server"] == "dispatch-mcp"

    def test_liveness_reports_uptime(self) -> None:
        result = health.liveness()
        assert "uptime_seconds" in result
        assert isinstance(result["uptime_seconds"], (int, float))
        assert result["uptime_seconds"] >= 0.0

    def test_ready_when_omniroute_url_set(self) -> None:
        with patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}):
            result = health.readiness()
        assert result["status"] == "ready"
        assert result["checks"]["omniroute_url"]["ok"] is True
        assert result["checks"]["omniroute_url"]["scheme"] == "http"

    def test_not_ready_when_omniroute_url_missing(self) -> None:
        with patch.dict("os.environ", {}, clear=True):
            result = health.readiness()
        assert result["status"] == "not_ready"
        assert result["checks"]["omniroute_url"]["ok"] is False
        assert "not set" in result["checks"]["omniroute_url"]["detail"]

    def test_readiness_does_not_contact_omniroute_by_default(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.health.httpx.get") as mock_get,
        ):
            health.readiness()
            mock_get.assert_not_called()

    def test_readiness_reports_reachable_when_upstream_ok(self) -> None:
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.raise_for_status = MagicMock()
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.health.httpx.get", return_value=mock_response) as mock_get,
        ):
            result = health.readiness(check_omniroute=True)
        assert result["status"] == "ready"
        assert result["checks"]["omniroute_reachable"]["ok"] is True
        mock_get.assert_called_once()

    def test_readiness_reports_unreachable_when_upstream_fails(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch(
                "dispatch_mcp.health.httpx.get",
                side_effect=httpx.ConnectError("refused"),
            ),
        ):
            result = health.readiness(check_omniroute=True)
        assert result["status"] == "not_ready"
        assert result["checks"]["omniroute_reachable"]["ok"] is False

    def test_metrics_returns_prometheus_text(self) -> None:
        payload = health.metrics()
        assert "# HELP dispatch_mcp_up" in payload
        assert "# TYPE dispatch_mcp_up gauge" in payload
        assert "dispatch_mcp_up 1" in payload
        assert "dispatch_mcp_uptime_seconds" in payload
        assert "dispatch_mcp_dispatches_total" in payload
        assert "dispatch_mcp_dispatch_errors_total" in payload

    def test_metrics_ends_with_newline(self) -> None:
        assert health.metrics().endswith("\n")
