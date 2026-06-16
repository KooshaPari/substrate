from __future__ import annotations

from dataclasses import dataclass


@dataclass(slots=True)
class JobResult:
    """Serialized result returned by dispatch MCP tools."""

    ok: bool | None = None
    tier: str | None = None
    message: str | None = None
    status: str | None = None
    error: str | None = None

    def to_dict(self) -> dict[str, str | bool | None]:
        """Serialize to the public MCP tool response shape."""
        return {
            key: value
            for key, value in {
                "ok": self.ok,
                "tier": self.tier,
                "message": self.message,
                "status": self.status,
                "error": self.error,
            }.items()
            if value is not None
        }
