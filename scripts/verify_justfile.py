#!/usr/bin/env python3
"""Verify the DX-10 Justfile exposes core contributor workflows."""

from pathlib import Path
import sys


REQUIRED = ("build:", "test:", "lint:", "check:")


def main() -> int:
    justfile = Path(__file__).resolve().parents[1] / "Justfile"
    if not justfile.is_file():
        print("missing Justfile", file=sys.stderr)
        return 1
    content = justfile.read_text(encoding="utf-8")
    missing = [entry for entry in REQUIRED if entry not in content]
    if missing:
        print(f"Justfile missing recipes: {', '.join(missing)}", file=sys.stderr)
        return 1
    print("DX-10 Justfile exposes core contributor workflows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
