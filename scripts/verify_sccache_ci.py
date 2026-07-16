#!/usr/bin/env python3
"""Verify the Rust CI job configures sccache."""

from pathlib import Path
import sys


def main() -> int:
    workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
    required = ("mozilla-actions/sccache-action", "RUSTC_WRAPPER: sccache")
    missing = [item for item in required if item not in workflow]
    if missing:
        print(f"CI sccache configuration missing: {', '.join(missing)}", file=sys.stderr)
        return 1
    print("DX-03 sccache configuration present in Rust CI")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
