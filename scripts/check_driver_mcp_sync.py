#!/usr/bin/env python3
"""Compare substrate driver-mcp with PhenoMCPServers servers/substrate (ADR-019)."""
from __future__ import annotations

import hashlib
import sys
from pathlib import Path

SUBSTRATE_ROOT = Path(__file__).resolve().parents[1]
DRIVER = SUBSTRATE_ROOT / "driver-mcp"
# sibling checkout used in pheno-mcp-work layout
PHENO = SUBSTRATE_ROOT.parent / "PhenoMCPServers" / "servers" / "substrate"

COMPARE = [
    "substrate_server.py",
    "dispatch_server.py",
    "lead_server.py",
    "team_mailbox_server.py",
    "_http.py",
    "_db.py",
    "_sanitize.py",
    "requirements.txt",
]


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    if not PHENO.is_dir():
        print(f"SKIP: PhenoMCPServers not found at {PHENO}")
        return 0

    drift: list[str] = []
    for name in COMPARE:
        left, right = DRIVER / name, PHENO / name
        if not left.exists():
            drift.append(f"missing in driver-mcp: {name}")
            continue
        if not right.exists():
            drift.append(f"missing in PhenoMCPServers: {name}")
            continue
        if sha(left) != sha(right):
            drift.append(f"hash mismatch: {name}")

    dispatch_pkg = "dispatch_mcp"
    for root, rel in [(DRIVER, DRIVER), (PHENO, PHENO)]:
        pass
    for path in sorted(DRIVER.glob("dispatch_mcp/**/*.py")):
        rel = path.relative_to(DRIVER)
        other = PHENO / rel
        if not other.exists() or sha(path) != sha(other):
            drift.append(f"hash mismatch: {rel.as_posix()}")

    if drift:
        for d in drift:
            print(f"DRIFT: {d}")
        return 1

    print(f"OK driver-mcp in sync with {PHENO}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
