from __future__ import annotations

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class Router(Protocol):
    """Port for dispatch and adapter operations."""

    async def dispatch(
        self, message: str, tier: str, payload: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """Dispatch a message and return the backend response."""

    async def health(self) -> dict[str, Any]:
        """Return backend health information."""

    def cancel(self, request_id: str) -> bool:
        """Cancel a previously created request."""

    async def close(self) -> None:
        """Close the adapter connection."""
