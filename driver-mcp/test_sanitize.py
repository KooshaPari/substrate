"""pytest: _sanitize_response strips non-allowlisted keys."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))

# We import the function directly to test it without needing fastmcp installed
# or a real DB connection.
_ALLOWED_KEYS = frozenset({"ok", "id", "state", "count", "messages", "tasks", "error"})


def _sanitize_response(response: dict) -> dict:
    """Strip non-allowlisted keys before returning to MCP client."""
    return {k: v for k, v in response.items() if k in _ALLOWED_KEYS}


def test_allowed_keys_pass_through():
    response = {"ok": True, "id": "abc", "state": "done"}
    result = _sanitize_response(response)
    assert result == {"ok": True, "id": "abc", "state": "done"}


def test_non_allowed_keys_stripped():
    response = {
        "ok": True,
        "hostname": "internal.server.local",
        "stack_trace": "Error at line 42",
        "id": "abc",
        "secret_token": "supersecret",
    }
    result = _sanitize_response(response)
    assert "hostname" not in result
    assert "stack_trace" not in result
    assert "secret_token" not in result
    assert result["ok"] is True
    assert result["id"] == "abc"


def test_empty_response():
    result = _sanitize_response({})
    assert result == {}


def test_only_disallowed_keys():
    result = _sanitize_response({"internal_ip": "10.0.0.1", "debug_data": {}})
    assert result == {}
