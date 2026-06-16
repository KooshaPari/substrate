"""Thin HTTP client for the substrate driver-http REST API."""
from __future__ import annotations

import os
from typing import Any

import httpx

DEFAULT_HTTP_URL = "http://127.0.0.1:8080"


def http_base_url() -> str:
    return os.environ.get("SUBSTRATE_HTTP_URL", DEFAULT_HTTP_URL).rstrip("/")


def auth_headers() -> dict[str, str]:
    token = os.environ.get("SUBSTRATE_HTTP_AUTH_TOKEN")
    if token:
        return {"Authorization": f"Bearer {token}"}
    return {}


def post_json(path: str, body: dict[str, Any], *, client: httpx.Client | None = None) -> dict[str, Any]:
    """POST JSON to a substrate HTTP endpoint and return the parsed body."""
    url = f"{http_base_url()}{path}"
    headers = {"Content-Type": "application/json", **auth_headers()}
    if client is None:
        with httpx.Client(timeout=120.0) as owned:
            resp = owned.post(url, json=body, headers=headers)
    else:
        resp = client.post(url, json=body, headers=headers)
    return _parse_response(resp)


def get_json(path: str, *, client: httpx.Client | None = None) -> dict[str, Any]:
    """GET a substrate HTTP endpoint and return the parsed body."""
    url = f"{http_base_url()}{path}"
    headers = auth_headers()
    if client is None:
        with httpx.Client(timeout=30.0) as owned:
            resp = owned.get(url, headers=headers)
    else:
        resp = client.get(url, headers=headers)
    return _parse_response(resp)


def _parse_response(resp: httpx.Response) -> dict[str, Any]:
    try:
        data = resp.json()
    except ValueError:
        data = {"error": resp.text or f"HTTP {resp.status_code}"}
    if resp.is_success:
        if isinstance(data, dict):
            return data
        return {"ok": True, "data": data}
    if isinstance(data, dict) and "error" in data:
        return {"error": str(data["error"])}
    return {"error": resp.text or f"HTTP {resp.status_code}"}
