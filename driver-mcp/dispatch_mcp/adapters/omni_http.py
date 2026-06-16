from __future__ import annotations

from typing import Any

import httpx


class OmniHttpAdapter:
    """HTTP adapter for OmniRoute dispatch endpoints."""

    _ALLOWED_RESPONSE_KEYS = frozenset({"ok", "tier", "message", "status", "error"})

    def __init__(self, base_url: str, client: httpx.AsyncClient | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self._client = client or httpx.AsyncClient(timeout=10.0)

    async def dispatch(
        self,
        message: str,
        tier: str,
        payload: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        response = await self._client.post(
            f"{self.base_url}/dispatch",
            json={"message": message, "tier": tier, "payload": payload or {}},
        )
        response.raise_for_status()
        data: dict[str, Any] = response.json()
        return data

    async def health(self) -> dict[str, Any]:
        response = await self._client.get(f"{self.base_url}/health")
        response.raise_for_status()
        data: dict[str, Any] = response.json()
        return data

    def cancel(self, request_id: str) -> bool:
        raise NotImplementedError("cancel() requires async implementation")

    async def close(self) -> None:
        """Close the adapter connection."""
        await self._client.aclose()

    def _sanitize_response(self, response: dict[str, Any]) -> dict[str, Any]:
        """Filter response to only include allowed keys, stripping internal/sensitive fields."""
        return {k: v for k, v in response.items() if k in self._ALLOWED_RESPONSE_KEYS}
