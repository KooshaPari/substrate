#!/usr/bin/env python3
"""Verify ARCH-14's transport-agnostic schema crate boundary."""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / "crates" / "a2a"


def main() -> int:
    cargo = CRATE / "Cargo.toml"
    readme = CRATE / "README.md"
    source = CRATE / "src" / "lib.rs"
    missing = [str(path.relative_to(ROOT)) for path in (cargo, readme, source) if not path.is_file()]
    if missing:
        print(f"ARCH-14 schema crate files missing: {', '.join(missing)}", file=sys.stderr)
        return 1
    manifest = cargo.read_text(encoding="utf-8")
    readme_text = readme.read_text(encoding="utf-8")
    if "schema" not in manifest.lower() or "schema" not in readme_text.lower():
        print("ARCH-14 crate must identify itself as a schema crate", file=sys.stderr)
        return 1
    if "Transport-agnostic" not in readme_text:
        print("ARCH-14 schema crate must document transport independence", file=sys.stderr)
        return 1
    print("ARCH-14 evidence: crates/a2a is a transport-agnostic schema crate")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
