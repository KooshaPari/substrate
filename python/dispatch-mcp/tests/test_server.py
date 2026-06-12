"""Tests for dispatch_mcp.server.

Note: These tests mock _client (the module-level httpx.Client) to avoid
requiring a live OmniRoute server.
"""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest


class TestCallOmniroute:
    """Tests for _call_omniroute via dispatch_custom and dispatch_health."""

    def test_dispatch_custom_success(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_response = MagicMock()
            mock_response.json.return_value = {
                "ok": True,
                "tier": "worker",
                "message": "hello",
            }
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            from dispatch_mcp.server import dispatch_custom

            result = dispatch_custom("worker", "hello")
            mock_client.post.assert_called_once()
            call_args = mock_client.post.call_args
            assert call_args[0][0] == "http://localhost:8080/dispatch"
            assert call_args[1]["json"] == {"tier": "worker", "message": "hello"}
            assert result == {"ok": True, "tier": "worker", "message": "hello"}

    def test_dispatch_custom_rejects_oversized_response(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_response = MagicMock()
            # Response body larger than MAX_RESPONSE_LENGTH (1 MiB)
            mock_response.content = b"x" * (1024 * 1024 + 1)
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            from dispatch_mcp.server import dispatch_custom

            with pytest.raises(RuntimeError, match="exceeds maximum allowed size"):
                dispatch_custom("worker", "test")

    def test_dispatch_custom_connection_error(self) -> None:
        import httpx

        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_client.post.side_effect = httpx.ConnectError("Connection refused")

            from dispatch_mcp.server import dispatch_custom

            with pytest.raises(httpx.ConnectError):
                dispatch_custom("main", "test")

    def test_dispatch_custom_request_error(self) -> None:
        import httpx

        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_client.post.side_effect = httpx.WriteTimeout("write timeout")

            from dispatch_mcp.server import dispatch_custom

            with pytest.raises(httpx.RequestError):
                dispatch_custom("worker", "test")

    def test_dispatch_custom_timeout(self) -> None:
        import httpx

        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_client.post.side_effect = httpx.TimeoutException("timed out")

            from dispatch_mcp.server import dispatch_custom

            with pytest.raises(httpx.TimeoutException):
                dispatch_custom("worker", "test")

    def test_dispatch_custom_http_error(self) -> None:
        import httpx

        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_response = MagicMock()
            mock_response.raise_for_status.side_effect = httpx.HTTPStatusError(
                "404 Not Found",
                request=MagicMock(),
                response=MagicMock(status_code=404),
            )
            mock_client.post.return_value = mock_response

            from dispatch_mcp.server import dispatch_custom

            with pytest.raises(httpx.HTTPStatusError):
                dispatch_custom("worker", "test")

    def test_dispatch_custom_json_decode_error(self) -> None:
        import json

        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_response = MagicMock()
            inner = json.JSONDecodeError("invalid", "", 0)
            mock_response.json.side_effect = inner
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            from dispatch_mcp.server import dispatch_custom

            with pytest.raises(RuntimeError, match="invalid response"):
                dispatch_custom("worker", "test")

    def test_dispatch_health_success_sanitized(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_response = MagicMock()
            mock_response.json.return_value = {
                "status": "ok",
                "upstream_id": "secret-internal",
                "error": None,
            }
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            from dispatch_mcp.server import dispatch_health

            result = dispatch_health()
            assert "status" in result
            assert "upstream_id" not in result
            assert result == {"status": "ok", "error": None}

    def test_dispatch_health_http_error(self) -> None:
        import httpx

        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_response = MagicMock()
            mock_response.raise_for_status.side_effect = httpx.HTTPStatusError(
                "", request=MagicMock(), response=MagicMock(status_code=503)
            )
            mock_client.post.return_value = mock_response

            from dispatch_mcp.server import dispatch_health

            with pytest.raises(httpx.HTTPStatusError):
                dispatch_health()

    def test_invalid_omniroute_url_raises(self) -> None:
        with patch.dict("os.environ", {"OMNIROUTE_URL": "javascript:alert(1)"}):
            from dispatch_mcp.server import dispatch_health

            with pytest.raises(ValueError, match="must use http or https"):
                dispatch_health()

    def test_missing_omniroute_url_raises(self) -> None:
        with patch.dict("os.environ", {}, clear=True):
            from dispatch_mcp.server import dispatch_custom

            with pytest.raises(ValueError, match="OMNIROUTE_URL"):
                dispatch_custom("worker", "test")


class TestDispatchCustomTierValidation:
    """Tests for dispatch_custom tier validation."""

    def test_invalid_tier_raises(self) -> None:
        from dispatch_mcp.server import dispatch_custom

        with pytest.raises(ValueError, match="Invalid tier 'rogue'"):
            dispatch_custom("rogue", "test")

    def test_empty_tier_raises(self) -> None:
        from dispatch_mcp.server import dispatch_custom

        with pytest.raises(ValueError, match="Invalid tier ''"):
            dispatch_custom("", "test")

    def test_all_valid_tiers(self) -> None:
        from dispatch_mcp.server import VALID_TIERS, dispatch_custom

        for tier in VALID_TIERS:
            with (
                patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
                patch("dispatch_mcp.server._client") as mock_client,
            ):
                mock_response = MagicMock()
                mock_response.json.return_value = {"ok": True}
                mock_response.raise_for_status = MagicMock()
                mock_client.post.return_value = mock_response
                result = dispatch_custom(tier, "hello")
                assert result["ok"] is True


class TestNamedDispatchTools:
    """Tests for the named dispatch_$tier tool functions."""

    @pytest.mark.parametrize(
        "tool_func",
        [
            "dispatch_worker",
            "dispatch_main",
            "dispatch_codeman",
            "dispatch_freetier",
            "dispatch_kimi",
            "dispatch_kimi_thinking",
            "dispatch_minimax",
            "dispatch_opus",
            "dispatch_haiku",
            "dispatch_gemini",
        ],
    )
    def test_named_tool_exists_and_callable(self, tool_func: str) -> None:
        from dispatch_mcp import server

        func = getattr(server, tool_func)
        assert callable(func)

    def test_dispatch_worker_rejects_oversized_message(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            from dispatch_mcp.server import dispatch_worker

            oversized = "x" * (4096 + 1)
            with pytest.raises(ValueError, match="exceeds maximum length"):
                dispatch_worker(oversized)
            mock_client.post.assert_not_called()

    def test_dispatch_worker_accepts_empty_message(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            from dispatch_mcp.server import dispatch_worker

            mock_response = MagicMock()
            mock_response.json.return_value = {"ok": True}
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            result = dispatch_worker("")
            assert result["ok"] is True
            mock_client.post.assert_called_once()


class TestSanitizeResponse:
    """Tests for response sanitization."""

    def test_sanitize_response_strips_internal_keys(self) -> None:
        from dispatch_mcp.server import _sanitize_response

        response = {
            "ok": True,
            "tier": "worker",
            "message": "hello",
            "internal_host": "omniroute-1.internal",
            "stack_trace": "Traceback (most recent call last):\n  ...",
            "db_password": "hunter2",
        }
        result = _sanitize_response(response)
        assert result == {"ok": True, "tier": "worker", "message": "hello"}
        assert "internal_host" not in result
        assert "stack_trace" not in result
        assert "db_password" not in result

    def test_sanitize_response_allows_error_field(self) -> None:
        from dispatch_mcp.server import _sanitize_response

        response = {"ok": False, "error": "something went wrong"}
        assert _sanitize_response(response) == response

    def test_sanitize_response_allows_status_field(self) -> None:
        from dispatch_mcp.server import _sanitize_response

        response = {"status": "alive", "server": "dispatch-mcp"}
        result = _sanitize_response(response)
        assert result == {"status": "alive"}
        assert "server" not in result

    def test_sanitize_response_preserves_allowed_keys(self) -> None:
        from dispatch_mcp.server import _sanitize_response

        response = {
            "ok": True,
            "tier": "worker",
            "message": "hello",
            "status": "ok",
            "error": None,
        }
        result = _sanitize_response(response)
        assert result == response


class TestDispatchLiveness:
    """Tests for dispatch_liveness tool."""

    def test_dispatch_liveness_returns_status(self) -> None:
        from dispatch_mcp.server import dispatch_liveness

        result = dispatch_liveness()
        assert result == {"status": "alive", "server": "dispatch-mcp"}

    def test_dispatch_liveness_no_omniroute_required(self) -> None:
        from dispatch_mcp.server import dispatch_liveness

        with patch.dict("os.environ", {}, clear=True):
            # dispatch_liveness should NOT check OMNIROUTE_URL
            # If it did, this would raise ValueError
            result = dispatch_liveness()
        assert result["status"] == "alive"


class TestGeneratedDispatchTools:
    """Tests for generated dispatch_$tier tool functions."""

    def test_dispatch_worker_calls_omniroute(self) -> None:
        with (
            patch.dict("os.environ", {"OMNIROUTE_URL": "http://localhost:8080"}),
            patch("dispatch_mcp.server._client") as mock_client,
        ):
            mock_response = MagicMock()
            mock_response.json.return_value = {"ok": True, "tier": "worker"}
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            from dispatch_mcp.server import dispatch_worker

            result = dispatch_worker("hello from worker")
            mock_client.post.assert_called_once()
            assert result["ok"] is True
