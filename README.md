<!-- AI-DD-META:START -->
<!-- This repository is planned, maintained, and managed by AI Agents only. -->
<!-- Slop issues are expected and intentionally present as part of an HITL-less -->
<!-- /minimized AI-DD metaproject of learning, refining, and building brute-force -->
<!-- training for both agents and the human operator. -->
![Downloads](https://img.shields.io/github/downloads/KooshaPari/thegent-dispatch/total?style=flat-square&label=downloads&color=blue)
![GitHub release](https://img.shields.io/github/v/release/KooshaPari/thegent-dispatch?style=flat-square&label=release)
![License](https://img.shields.io/github/license/KooshaPari/thegent-dispatch?style=flat-square)
![AI-Slop](https://img.shields.io/badge/AI--DD-Slop%20Expected-orange?style=flat-square)
![AI-Only-Maintained](https://img.shields.io/badge/Planned%20%26%20Maintained%20by-AI%20Agents%20Only-red?style=flat-square)
![HITL-less](https://img.shields.io/badge/HITL--less%20AI--DD-metaproject-yellow?style=flat-square)

> ⚠️ **AI-Agent-Only Repository**
>
> This repo is **planned, maintained, and managed exclusively by AI Agents**.
> Slop issues, rough edges, and AI artifacts are **expected and intentionally
> present** as part of an **HITL-less / minimized AI-DD** metaproject focused
> on learning, refining, and brute-force training both the agents and the
> human operator. Bug reports and contributions are still welcome, but please
> expect AI-generated code, comments, and documentation throughout.
<!-- AI-DD-META:END -->
> **Work state:** SCAFFOLD · **Progress:** `███▌░░░░░░ 35%`
> Unified dispatcher: provider-agnostic schema → native argv for 8 agent CLIs. argv-building + --dry-run + rck panel work; real `thegent bg` integration + MCP surface pending. · updated 2026-06-02

> **Pinned references (Phenotype-org)**
> - MSRV: see rust-toolchain.toml
> - cargo-deny config: see deny.toml
> - cargo-audit: rustsec/audit-check@v2 weekly
> - Branch protection: 1 reviewer required, no force-push
> - Authority: phenotype-org-governance/SUPERSEDED.md

# thegent-dispatch

Unified CLI dispatcher: takes a provider-agnostic schema and translates to the
native argv for Forge, Codex, Gemini, Copilot, Cursor, Droid, or routes through
`cheap-llm` (Minimax/Kimi) for cost-sensitive work.

Draft v0.1. Skeleton only — argv construction is real; integration with
`thegent bg` / `thegent-skills` orchestration layer pending.

## Usage

```bash
thegent-dispatch --provider forge --prompt "explain this function" --model claude-opus
thegent-dispatch --provider codex --prompt "refactor" --reasoning high --mode plan
thegent-dispatch --provider minimax --prompt "summarize: ..." # routes to cheap-llm CLI
thegent-dispatch --provider forge --prompt "long task" --session bg --owner helios
thegent-dispatch --provider copilot --prompt "x" --dry-run # preview argv
```

## Design

See `cheap-llm-mcp/claude/thegent-unified-design.md` for the absorption rationale.

## Status

- [x] Arg parsing (clap)
- [x] Per-provider argv builders (forge, codex, gemini, copilot, cursor, droid, minimax, claude)
- [x] Hard constraints (copilot model-lock rejection)
- [x] `--dry-run` for safe previews
- [x] Tests (6 unit tests on build_argv)
- [ ] Actual integration with `thegent bg` session wrapper
- [ ] MCP server version (expose dispatch as a tool)
- [ ] Model enumeration via provider (forge list model, etc.)

## Installation

Install the `thegent-dispatch` binary straight from Git with Cargo:

```bash
cargo install --git https://github.com/KooshaPari/thegent-dispatch
```

Or, with [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall):

```bash
cargo binstall --git https://github.com/KooshaPari/thegent-dispatch thegent-dispatch
```

## Build

For local development:

```bash
git clone https://github.com/KooshaPari/thegent-dispatch
cd thegent-dispatch
cargo build --release
cargo test
```

## License

MIT

## Documentation

This repository includes the following cross-cutting documents:

- [`AGENTS.md`](AGENTS.md) — operating instructions for AI agents and human contributors
- [`docs/`](docs/) — design notes, ADRs, and supporting documentation (see [`docs/index.md`](docs/index.md))

