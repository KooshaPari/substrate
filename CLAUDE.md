# CLAUDE.md — Agent Context for substrate

## Project Overview

**substrate**: Substrate runtime

## Repository Structure

- See README.md for full architecture
- CI/CD: .github/workflows/
- Quality gates: deny.toml, .pre-commit-config.yaml

## Build & Test

- `cargo build` (Rust repos)
- `cargo test` (Rust repos)
- `cargo clippy -- -D warnings` (lint)
- `cargo fmt --check` (format check)

## Conventions

- Use Conventional Commits: `type(scope): description`
- All CI must pass before merge
- One approval required for merge
- Squash merge to main

## Agent Rules

- Read AGENTS.md for full gate definitions
- Never commit secrets or credentials
- Always run `cargo clippy` before committing
- Prefer editing existing files over creating new ones
- Check for existing patterns before adding new code
