# Contributing to kooshapari/substrate

> **Status:** Phase 0 scaffold (re-mediated 2026-07-08). This document is
> authoritative for the engineering workflow. For non-normative overview see
> [`README.md`](./README.md) and [`ARCHITECTURE.md`](./ARCHITECTURE.md).

---

## 1. Ground rules

1. **Read [`ARCHITECTURE.md`](./ARCHITECTURE.md) first.** It maps every crate
   to a layer and explains the request pipeline. Five minutes spent there
   saves hours of spelunking.
2. **All crates are `edition = "2021"`, `rust-version = "1.80"`.**
   Toolchain is pinned in `rust-toolchain.toml` — install via
   `rustup toolchain install $(cat rust-toolchain.toml | grep channel | cut -d'"' -f2)`.
3. **`#![forbid(unsafe_code)]` is on every gateway / core / driver / store
   / engine crate.** Don't add `unsafe` to a file that already has the
   forbid. Ask first.
4. **No upward dependencies.** Adapters depend on `substrate-core`; core
   never depends on an adapter. `cargo tree --invert substrate-core` is the
   fast test.
5. **Public APIs must be documented.** Add `///` on every `pub` item.
   `#![warn(missing_docs)]` is set in `substrate-trace` and `substrate-core`;
   make it `#![deny(missing_docs)]` for any new crate.

---

## 2. Local development

### Prerequisites

- Rust 1.80+ (use `rust-toolchain.toml`)
- `cargo install cargo-deny cargo-llvm-cov cargo-mutants cargo-fuzz`
- `direnv` (optional but recommended; `.envrc` is not yet wired — see issue)

### Bootstrap

```bash
git clone https://github.com/KooshaPari/substrate
cd substrate
rustup component add clippy rustfmt
cargo build --workspace
cargo test --workspace
```

### Daily loop

```bash
# format-check + lint + build (read these before pushing)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p <crate>
```

### Cargo subcommands you must know

| Tool | When you run it | What it does |
|---|---|---|
| `cargo build --workspace` | every PR | builds every crate |
| `cargo test --workspace` | every PR | runs unit + integration tests |
| `cargo fmt --all -- --check` | every PR | enforces formatting |
| `cargo clippy --workspace --all-targets -- -D warnings` | every PR | strict lints |
| `cargo deny check` | every PR (CI) | licenses + bans + advisories + sources |
| `cargo llvm-cov report` | local | line coverage report |
| `cargo fuzz run <target>` | local | fuzz `crates/fuzz` targets |
| `cargo doc --workspace --no-deps` | local | rustdoc build |

---

## 3. Pull request workflow

1. **Branch off `main`.** Do not branch off a worktree.
2. **One logical change per PR.** If you find two things to fix, open two PRs.
3. **Run the local checks** above. CI must not catch anything new.
4. **Commit message format:**
   ```
   <type>(<crate-or-scope>): <subject>

   <body>

   <footer>
   ```
   Allowed types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`,
   `build`, `ci`. Subject ≤ 72 chars, body wrapped at 100.
5. **Open a PR** against `main`. Fill in the template (added in P2.4).
6. **Address review feedback one comment at a time.** Resolve threads with
   `Resolve conversation`. Do not push fixes in the same commit as the
   original work — push a follow-up commit so the diff history tells a
   story.
7. **Squash-merge** if the PR contains 10+ commits; otherwise preserve the
   commit history.

### Review SLA

- **First review** — within 2 business days
- **Subsequent review** — within 1 business day
- **Approval requires 1 reviewer** from `.github/CODEOWNERS`

---

## 4. Adding a new crate

1. Create `crates/<your-crate>/{Cargo.toml,src/lib.rs}`.
2. Add `crates/<your-crate>` to `[workspace] members` in `/Cargo.toml`.
3. Pick the right **layer** (see `ARCHITECTURE.md` §2):
   - L0 (core) — only `substrate-core` lives here
   - L1 (use-cases) — orchestration, business logic
   - L2 (ports/drivers) — adapters implementing core traits
   - L3 (inbound) — HTTP/CLI/MCP frontends
   - L4 (meta) — re-export crate only
   - T (test) — fixtures, conformance
4. Use workspace-level deps (`substrate-core = { workspace = true }`).
5. Add `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]` to `src/lib.rs`.

---

## 5. Adding a new engine adapter

1. Pick a layer-2 slot (L2 ports/drivers).
2. Add `crates/engine-<name>/{Cargo.toml,src/lib.rs}`.
3. Implement the `Engine` trait from `engine-spec`. Use
   `engine-conformance` as the conformance template.
4. Add unit + conformance tests in `tests/conformance.rs`.
5. Wire the new engine in `psub-gateway::config_watcher`.
6. Update `ARCHITECTURE.md` §6.

---

## 6. Working with the migration plan

`DISPATCH_MIGRATION_PLAN_2026_06_30.md` documents the migration of dispatch
logic out of the legacy engine-coupled path into `substrate-app`. The plan
is authoritative for any changes that touch dispatch.

---

## 7. Coding conventions

- **No `.unwrap()` in non-test code.** Use `?`, `.context()`,
  `.expect("reason that matters")`.
- **Prefer `&str` over `String`** in function arguments; take `String` only
  when ownership transfer is required.
- **Imports order**: std → third-party → workspace → local (`super`,
  `crate`). Insert blank line between each group.
- **Error context**: every `?` in production code should chain
  `.with_context(|| ...)` (thiserror) or `.context(...)` (anyhow). This is
  a CI-enforced clippy lint.
- **Public types derive `Debug`.** Add `Clone` only when cheap.
- **No `println!` in library code.** Use `tracing::info!` etc.

---

## 8. Testing

### Unit tests

Place `#[cfg(test)] mod tests` at the bottom of the file under test.

### Integration tests

Place in `crates/<crate>/tests/` as `*.rs`. Use `tempfile = "3.14"` for
scratch state.

### Conformance tests

`crates/arch-test/` and `crates/engine-conformance/` enforce cross-crate
contracts. If you change a trait signature, expect failures here first.

### Property-based tests

`proptest` is allowed for any parser/codec. For pure-encoding codecs (BIP-173,
x509, key_tag) it is **required**.

### Fuzzing

`crates/fuzz/` (added in P1.5). Run before submitting changes that touch a
parser.

---

## 9. Security

- **Do not commit API keys, tokens, or credentials.** `.gitleaks.toml` runs
  in CI; leaks fail the build.
- **Secret-injection bugs (jwk, OAuth tokens)** must be reported via the
  process in [`SECURITY.md`](./SECURITY.md), not via public issues.
- **crate dependencies** are reviewed via `cargo deny` in CI. Bans are
  enforced by `deny.toml` [P0.1].

---

## 10. Release

| Action | Where | Trigger |
|---|---|---|
| Tag `vX.Y.Z` | the repo | maintainer |
| SLSA provenance | `.github/workflows/release-binary.yml` | tag push |
| CycloneDX SBOM | `.github/workflows/sbom.yml` (P2.1) | tag push |
| Container image | Containerfile | tag push |
| crates.io publish | `.github/workflows/release-crates.yml` | tag push |
| GitHub Release | manual + rel-bot | tag push |

See [`CHANGELOG.md`](./CHANGELOG.md) for the canonical release history.

---

## 11. Community

- **Issues:** GitHub issues, by component label
- **Discussions:** GitHub Discussions (not yet enabled — see roadmap)
- **Code of conduct:** standard "be kind, be technical" — there is no
  separate `CODE_OF_CONDUCT.md` yet

---

## 12. Where to get help

| You are trying to... | Open an issue labeled | Or ask in |
|---|---|---|
| understand the layout | `question` | — |
| propose a change | `rfc` (template TBD) | see ADR template below |
| report a bug | `bug` | follow the bug template |
| request a feature | `enhancement` | follow the feature template |
| report a security issue | — | email security@ (see SECURITY.md) |
