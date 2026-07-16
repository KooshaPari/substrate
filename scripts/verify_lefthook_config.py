#!/usr/bin/env python3
"""Verify the repository pre-commit hook protects Rust quality gates."""

from __future__ import annotations

import sys
from pathlib import Path


REQUIRED_SNIPPETS = (
    "pre-commit:",
    "cargo fmt --all -- --check",
    "cargo clippy --workspace --all-targets -- -D warnings",
)


def main() -> int:
    config = Path(__file__).resolve().parents[1] / "lefthook.yml"
    if not config.is_file():
        print(f"missing pre-commit configuration: {config}", file=sys.stderr)
        return 1

    content = config.read_text(encoding="utf-8")
    missing = [snippet for snippet in REQUIRED_SNIPPETS if snippet not in content]
    if missing:
        print(f"lefthook.yml missing required entries: {', '.join(missing)}", file=sys.stderr)
        return 1

    print("lefthook pre-commit configuration protects Rust format and clippy gates")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
