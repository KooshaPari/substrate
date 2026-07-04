# AGENTS.md — substrate

Agent entrypoint for autonomous work in this repo. Read this before editing.

## Working directory

Repo root is a Cargo workspace. Feature work goes in a git worktree, never on `main`:

```
git worktree add -b <type>/<topic> .claude/worktrees/<topic> origin/main
```

`<type>` ∈ `feat|fix|chore|ci|docs`. Worktrees live under `.claude/worktrees/` only.

## Build / test / lint / run

```bash
cargo build --workspace                              # build all crates
cargo test  --workspace                              # run the suite (733+ tests)
cargo clippy --workspace --all-targets -- -D warnings # lint (warnings are errors)
cargo fmt --all -- --check                           # format check
cargo build -p driver-http                           # the HTTP REST surface crate
cargo build --release -p driver-cli                  # -> target/release/substrate (the CLI)
```

Python MCP surface: `pip install -r driver-mcp/requirements.txt && pytest driver-mcp/`.

Fast inner loop: scope to one crate — `cargo test -p <crate>` / `cargo check -p <crate>`.

## Key files

| Path | What |
|------|------|
| `crates/substrate` | SDK facade — single dependency for downstream repos |
| `crates/substrate-core` | domain + ports (DispatchPlanner) |
| `crates/driver-cli` | the `substrate` CLI binary (`[[bin]] name = "substrate"`) |
| `crates/driver-http` | HTTP REST surface (bind via `SUBSTRATE_HTTP_BIND`) |
| `crates/engine-*` | per-engine adapters (forge/codex/claude/agentapi/...) |
| `crates/substrate-tui` | ratatui dashboard |
| `SPEC.md` | one-page spec | `llms.txt` | LLM doc index |

## Forbidden

- No direct commits to `main` (protected — PR only).
- No `git reset --hard`, `git stash`, `git clean` in worktrees.
- No `--no-verify` / hook bypass without operator approval.
- No AI attribution in commit/PR metadata.
- Do not work a branch/worktree another actor is on.

## Gotchas

- Run `cargo check -p <crate>` before editing to warm the cache.
- `clippy` is `-D warnings` — fix, don't `#[allow]` (allow needs a tracking-issue comment).
- MSRV is pinned in `rust-toolchain.toml`; match it.
- `substrate dispatch` routes to engine binaries via `FORGE_BIN`/`CODEX_BIN` env — it does NOT need OmniRoute.
