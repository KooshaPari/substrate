#!/usr/bin/env python3
"""Verify the canonical RFC process remains actionable and complete."""

from __future__ import annotations

import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RFC = ROOT / "docs" / "RFC.md"
REQUIRED_SECTIONS = (
    "## When to write an RFC",
    "## RFC lifecycle",
    "## RFC template",
    "## Decision records (ADRs)",
    "## Links and ownership",
)
REQUIRED_TEMPLATE_FIELDS = (
    "## Summary",
    "## Motivation and goals",
    "## Non-goals",
    "## Proposal",
    "## Alternatives considered",
    "## Compatibility and migration",
    "## Test and rollout plan",
    "## Open questions",
)


def main() -> int:
    if not RFC.is_file():
        print(f"missing DOC-17 evidence: {RFC}", file=sys.stderr)
        return 1

    content = RFC.read_text(encoding="utf-8")
    required = (*REQUIRED_SECTIONS, *REQUIRED_TEMPLATE_FIELDS)
    missing = [heading for heading in required if heading not in content]
    if missing:
        print(f"RFC process missing required headings: {', '.join(missing)}", file=sys.stderr)
        return 1

    if "docs/adr/" not in content or "CONTRIBUTING.md" not in content:
        print("RFC process must link the ADR and contributor workflows", file=sys.stderr)
        return 1

    print("DOC-17 evidence: canonical RFC process is actionable and linked to ADRs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
