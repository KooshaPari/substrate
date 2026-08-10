# Pull Request

Thanks for contributing to substrate. Please fill out the sections below so
reviewers can land your change quickly.

## Summary

<!-- 1-3 sentences. What does this PR do and why? -->

## Linked issues / ADRs

- Fixes #
- Relates to #
- ADR: docs/adr/XXXX-*.md (if architecture-relevant)

## Type of change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds capability)
- [ ] Breaking change (fix or feature that changes public API, wire format, or on-disk layout)
- [ ] Refactor / cleanup (no behavior change)
- [ ] Documentation / ADR only
- [ ] CI / build / tooling

## FR-IDs / Checklists

<!-- FR-NNN this satisfies, or "n/a — <reason>". -->

### Required

- [ ] `cargo fmt --all -- --check` passes locally
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes locally
  (or, for `psub-orchestrator` which has known pre-existing warnings,
  `cargo clippy -p <affected-crate> --all-targets -- -D warnings`)
- [ ] `cargo test --workspace` passes locally
- [ ] If you changed the public API or wire format:
  - [ ] Added an entry to `CHANGELOG.md` under **Unreleased**
  - [ ] Updated `ARCHITECTURE.md` and / or `docs/adr/`
  - [ ] Wrote a migration note in `docs/operations/` if downstream must act
- [ ] If you added a new dependency: ran `cargo deny check` locally

### Quality

- [ ] New code has unit tests (`#[cfg(test)] mod tests`)
- [ ] New public API surfaces have at least one integration test in `tests/`
- [ ] New parser / codec / state-machine code has a fuzz target in `fuzz/`
- [ ] Hot-path code is wrapped in `#[tracing::instrument]` and emits useful
  structured fields (request id, model, provider, outcome, latency_ms)
- [ ] No `unwrap()` / `expect()` on user-supplied input
- [ ] No `println!` / `dbg!` left in the diff

### Security (touch any of these? then all four are required)

- [ ] Secrets / credentials handling reviewed
- [ ] Authn / authz path reviewed
- [ ] Input validation added or confirmed (typed
      `serde::Deserialize` + length limits)
- [ ] Error paths return sanitized messages
      (no `err` chain printed to HTTP clients; use
      `tracing::warn!(error = ?err, ...)` server-side)

## How tested

<!-- Commands run + result. e.g. `cargo test --workspace` (pass), new tests added. -->

## Risk

<!-- Blast radius: crates touched, public-API/behavior changes, migration needs. -->

## Rollback

<!-- How to revert safely if this misbehaves. -->
