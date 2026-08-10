# rust-analyzer workspace discovery

Substrate is a standard Cargo workspace rooted at the repository `Cargo.toml`.
Open the repository root in an editor; rust-analyzer discovers the complete
workspace through Cargo metadata. Do not commit a generated `rust-project.json`:
that format is intended for non-Cargo build systems and would duplicate,
then drift from, the Cargo package graph.

This contract is verified by:

```bash
python3 scripts/verify_rust_analyzer_workspace.py
```

The verifier requires `cargo metadata --no-deps --format-version 1` to resolve
this repository as the workspace root and report at least one workspace member.

## Upstream contract

rust-analyzer enables Cargo auto-reload by default and refreshes project
information with `cargo metadata` when `Cargo.toml` changes. Its configuration
also treats each Rust workspace as rooted at the directory containing its
`Cargo.toml`. See the [rust-analyzer configuration reference](https://rust-analyzer.github.io/book/configuration.html).
