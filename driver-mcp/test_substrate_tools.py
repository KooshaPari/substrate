"""pytest: substrate MCP tools call driver-http with correct shape."""
from __future__ import annotations

import os
import sys
from unittest.mock import MagicMock, patch

# Configure in-memory DB before importing servers that open SQLite.
os.environ.setdefault("SUBSTRATE_DB", ":memory:")
sys.path.insert(0, os.path.dirname(__file__))

import httpx
import pytest

import _http
import _sanitize
import substrate_server


@pytest.fixture(autouse=True)
def _env(monkeypatch):
    monkeypatch.setenv("SUBSTRATE_HTTP_URL", "http://127.0.0.1:9999")
    monkeypatch.delenv("SUBSTRATE_HTTP_AUTH_TOKEN", raising=False)
    monkeypatch.setenv("SUBSTRATE_DB", ":memory:")


def test_substrate_dispatch_calls_dispatch_endpoint():
    with patch.object(
        _http,
        "post_json",
        return_value={
            "text": "done",
            "status": "completed",
            "artifacts": [],
            "pr_urls": [],
            "internal_trace": "secret",
        },
    ) as mock_post:
        result = substrate_server.substrate_dispatch("echo hi", engine="forge", cwd="/tmp")

    mock_post.assert_called_once_with(
        "/v1/dispatch",
        {"prompt": "echo hi", "engine": "forge", "cwd": "/tmp"},
    )
    assert "internal_trace" not in result
    assert result["text"] == "done"
    assert result["status"] == "completed"


def test_substrate_plan_calls_plan_endpoint():
    with patch.object(
        _http,
        "post_json",
        return_value={
            "engine": "forge",
            "session_mode": "foreground",
            "argv": ["forge", "run"],
            "spec": {"prompt": "hi", "cwd": "/tmp"},
            "debug_host": "hidden",
        },
    ) as mock_post:
        result = substrate_server.substrate_plan("hi", cwd="/tmp")

    mock_post.assert_called_once_with("/v1/plan", {"prompt": "hi", "cwd": "/tmp"})
    assert "debug_host" not in result
    assert result["engine"] == "forge"
    assert result["argv"] == ["forge", "run"]


def test_substrate_route_calls_route_endpoint():
    task = {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "prompt": "route me",
        "cwd": "/repo",
        "state": "submitted",
        "parent_task_id": None,
        "requirement_id": None,
        "epic_id": None,
    }
    with patch.object(
        _http,
        "post_json",
        return_value={"engine": "forge", "model": "kimi", "reason": "default", "shard": "internal"},
    ) as mock_post:
        result = substrate_server.substrate_route(task)

    mock_post.assert_called_once_with("/v1/route", {"task": task})
    assert "shard" not in result
    assert result["engine"] == "forge"
    assert result["model"] == "kimi"


def test_sanitize_strips_internal_fields():
    raw = {
        "ok": True,
        "text": "hi",
        "hostname": "internal.server",
        "stack_trace": "oops",
        "engine": "forge",
    }
    result = _sanitize.sanitize_response(raw)
    assert "hostname" not in result
    assert "stack_trace" not in result
    assert result["text"] == "hi"
    assert result["engine"] == "forge"


def test_empty_prompt_rejected_without_http():
    result = substrate_server.substrate_dispatch("   ")
    assert result == {"error": "prompt must not be empty"}


def test_empty_prompt_plan_rejected():
    result = substrate_server.substrate_plan("")
    assert result == {"error": "prompt must not be empty"}


def test_route_bad_task_rejected():
    assert substrate_server.substrate_route({"cwd": "/tmp"}) == {"error": "task.prompt must not be empty"}
    assert substrate_server.substrate_route("not-a-dict") == {"error": "task must be an object"}


def test_http_error_surfaces_as_error_key():
    with patch.object(_http, "post_json", return_value={"error": "cwd must not be empty"}):
        result = substrate_server.substrate_plan("hi", cwd="/tmp")
    assert result == {"error": "cwd must not be empty"}


def test_post_json_uses_injected_client():
    client = MagicMock(spec=httpx.Client)
    client.post.return_value = httpx.Response(
        200,
        json={"engine": "forge", "session_mode": "foreground", "argv": []},
        request=httpx.Request("POST", "http://127.0.0.1:9999/v1/plan"),
    )
    result = _http.post_json("/v1/plan", {"prompt": "x", "cwd": "/tmp"}, client=client)
    client.post.assert_called_once()
    assert client.post.call_args[0][0] == "http://127.0.0.1:9999/v1/plan"
    assert result["engine"] == "forge"


def test_auth_header_when_token_set(monkeypatch):
    monkeypatch.setenv("SUBSTRATE_HTTP_AUTH_TOKEN", "secret-token")
    client = MagicMock(spec=httpx.Client)
    client.post.return_value = httpx.Response(
        200,
        json={"engine": "forge", "session_mode": "foreground", "argv": []},
        request=httpx.Request("POST", "http://127.0.0.1:9999/v1/plan"),
    )
    _http.post_json("/v1/plan", {"prompt": "x", "cwd": "/tmp"}, client=client)
    headers = client.post.call_args[1]["headers"]
    assert headers["Authorization"] == "Bearer secret-token"
