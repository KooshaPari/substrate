#!/usr/bin/env python3
"""Verify evidence for the final release, tracing, and testing scorecard gaps."""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    formula = ROOT / "packaging/homebrew/substrate.rb"
    mutation = ROOT / ".github/workflows/mutation.yml"
    gateway = ROOT / "crates/gateway/src/main.rs"
    required = (formula, mutation, gateway)
    missing = [str(p.relative_to(ROOT)) for p in required if not p.is_file()]
    if missing:
        print(f"scorecard evidence missing: {', '.join(missing)}", file=sys.stderr)
        return 1
    if "TraceContextPropagator" not in gateway.read_text(encoding="utf-8"):
        print("OBS-07 W3C Trace Context propagator is not registered", file=sys.stderr)
        return 1
    if "cargo mutants" not in mutation.read_text(encoding="utf-8"):
        print("TEST-08 mutation workflow is not configured", file=sys.stderr)
        return 1
    if "sha256" not in formula.read_text(encoding="utf-8"):
        print("RE-07 Homebrew formula must pin a release checksum", file=sys.stderr)
        return 1
    print("TEST-08, OBS-07, RE-07 evidence verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
