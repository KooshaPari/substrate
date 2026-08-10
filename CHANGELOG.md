# Changelog

All notable changes to substrate are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versioning is [SemVer](https://semver.org/).

## [0.2.0] - 2026-07-04

### Added
- Prometheus latency histogram (exponential buckets 10ms–5.12s)
- Sliding-window request rate tracker (10s window)
- Extended /health endpoint with per-provider SLA + circuit breaker state
- SLA violation tier checker (P50/P95/P99)
- TUI boot animation with BootPhase state machine
- Gateway startup banner
- OCI Containerfile + process-compose.yaml

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
