#!/usr/bin/env python3
"""Verify the versioned development container provides the DX-09 contract."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / ".devcontainer" / "devcontainer.json"
EVIDENCE = ROOT / "docs" / "architecture" / "development-container.md"


def main() -> int:
    if not EVIDENCE.is_file():
        print(f"missing DX-09 evidence: {EVIDENCE}", file=sys.stderr)
        return 1
    config = json.loads(CONFIG.read_text(encoding="utf-8"))
    features = config.get("features", {})
    if not config.get("image") or not any("rust" in feature for feature in features):
        print("devcontainer must declare an image and Rust feature", file=sys.stderr)
        return 1
    if "rust-lang.rust-analyzer" not in config.get("customizations", {}).get("vscode", {}).get("extensions", []):
        print("devcontainer must install rust-analyzer", file=sys.stderr)
        return 1
    print("DX-09 evidence: versioned Rust development container is configured")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
