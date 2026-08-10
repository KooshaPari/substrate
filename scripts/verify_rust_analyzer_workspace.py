#!/usr/bin/env python3
"""Verify rust-analyzer can discover the Cargo workspace without rust-project.json."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs" / "architecture" / "rust-analyzer.md"


def main() -> int:
    if not EVIDENCE.is_file():
        print(f"missing DX-05 evidence: {EVIDENCE}", file=sys.stderr)
        return 1

    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    if Path(metadata["workspace_root"]).resolve() != ROOT.resolve():
        print("cargo metadata did not resolve this repository as its workspace root", file=sys.stderr)
        return 1
    if not metadata["workspace_members"]:
        print("cargo metadata reported no workspace members", file=sys.stderr)
        return 1

    print("rust-analyzer DX-05 evidence: Cargo workspace discovery is reproducible")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
