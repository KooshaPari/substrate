# Changelog

All notable changes to substrate are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versioning is [SemVer](https://semver.org/).

## [Unreleased]

### Added

- `AGENTS.md` — agent entrypoint (build/test/lint/worktree/forbidden-ops).
- `rust-toolchain.toml` — MSRV pin (1.80) for reproducible builds.
- `.github/PULL_REQUEST_TEMPLATE.md` and `user-friction` issue template.
- `substrate-tui` metrics panel (m-key toggle) (#85).
- `driver-cli serve` subcommand with lock-based single-instance guard (#78).

### Notes

- `substrate dispatch` routes directly to engine binaries (forge/codex/claude/agentapi)
  via `FORGE_BIN`/`CODEX_BIN`; it does not require the OmniRoute router.

<!-- Prior history predates this changelog; reconstruct from git tags as versions are cut. -->
