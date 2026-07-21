# Dependency policy

This policy governs third-party dependencies used by the substrate workspace.
It applies to runtime crates, build tools, release tooling, and checked-in
development automation.

## Approval rules

1. Prefer a maintained crate from crates.io with a compatible permissive
   license and an active release history.
2. Pin the dependency in the workspace `Cargo.toml`; commit `Cargo.lock`.
3. Add a short rationale to the PR description and update the relevant ADR or
   architecture document when the dependency changes a boundary.
4. Keep dependency features minimal. Do not enable default features solely to
   obtain an unrelated capability.
5. Do not add git, path, or unpublished registry dependencies to production
   crates without an explicit maintainer review and a removal issue.

## Security and licensing gates

Every dependency change must pass the CI `cargo deny check` and `cargo audit`
jobs. The repository's `deny.toml` is the source of truth for allowed licenses,
registries, duplicate bans, and advisory handling. A new advisory or license
violation blocks merge until it is upgraded, removed, or documented as an
approved exception.

Release artifacts are generated from the locked graph. Reviewers should check
the lockfile diff for unexpected transitive additions and confirm that no
credentials, vendored binaries, or generated build output entered the commit.

## Ownership and review

The author owns the dependency rationale and rollback plan. A maintainer owns
the final approval. Emergency security upgrades may be merged with a reduced
review window, but must retain the lockfile and CI evidence and be recorded in
the changelog.
