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

## Build

```bash
cargo build --release
cargo test
```

## License

MIT
