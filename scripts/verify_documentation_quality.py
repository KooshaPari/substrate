#!/usr/bin/env python3
"""Verify the dependency policy, glossary, and FAQ documentation contract."""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
REQUIRED = {
    "SEC-16": (ROOT / "docs/operations/dependency-policy.md", ("# Dependency policy", "## Approval rules", "## Security and licensing gates")),
    "DOC-15": (ROOT / "docs/GLOSSARY.md", ("# Glossary", "| Term | Meaning |", "SLO / SLI")),
    "DOC-16": (ROOT / "docs/FAQ.md", ("# Frequently asked questions", "## Which command should I run first?", "## How are vulnerabilities reported?")),
}


def main() -> int:
    failures: list[str] = []
    for criterion, (path, markers) in REQUIRED.items():
        if not path.is_file():
            failures.append(f"{criterion}: missing {path.relative_to(ROOT)}")
            continue
        content = path.read_text(encoding="utf-8")
        missing = [marker for marker in markers if marker not in content]
        if missing:
            failures.append(f"{criterion}: missing markers {', '.join(missing)}")
    if failures:
        print("documentation verification failed:", file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 1
    print("SEC-16, DOC-15, DOC-16 documentation evidence verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
