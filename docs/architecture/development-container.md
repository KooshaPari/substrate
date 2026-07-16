# Development container

Substrate provides a versioned development container at
`.devcontainer/devcontainer.json`. It supplies Rust 1.80, Python 3.12, Git,
and the rust-analyzer VS Code extension, then installs the repository's Rust
quality and release tooling after creation.

Open the repository root with a Dev Containers-capable editor and choose
**Reopen in Container**. The container configuration enables Rust format on
save and uses Clippy for rust-analyzer diagnostics.

Verify this DX contract without starting a container:

```bash
python3 scripts/verify_devcontainer.py
```
