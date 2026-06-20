# Pull Request — sharecli

> Thanks for contributing! Please fill in the sections below. Sections marked
> **required** must be completed before the PR can be reviewed.

---

## Summary

<!-- Required: 1–3 sentences describing what this PR does and why. -->

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing behavior to change)
- [ ] Documentation update
- [ ] CI / tooling / governance
- [ ] Refactor (no behavior change)

## Linked Issues

<!-- Required: link the issue(s) this PR closes or addresses. -->
<!-- Example: Closes #42, fixes #43 -->

## FR-#### Reference

<!-- Optional: if this implements a tracked functional requirement, list it. -->
<!-- Example: FR-001 — process listing -->

## Implementation Notes

<!-- Required for non-trivial changes. Anything reviewers should know: -->
<!--   • design decisions / alternatives considered -->
<!--   • performance implications -->
<!--   • backwards-compatibility impact -->
<!--   • known limitations / TODOs -->

## Testing

<!-- Required: describe how this was tested and attach evidence. -->

- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual reproduction verified
- [ ] All tests pass locally: `just test`
- [ ] Lints clean locally: `just lint`
- [ ] Formatting clean locally: `just fmt-check`
- [ ] Audit / deny clean locally: `just audit && just deny`

### Test Output

```text
<paste `just test` output here>
```

## Documentation

- [ ] `CHANGELOG.md` updated (add an `[Unreleased]` entry)
- [ ] `README.md` updated (if user-facing)
- [ ] `docs/` updated (if design / architecture changed)
- [ ] Doc-comments added on public APIs

## Risk & Rollout

<!-- Required for non-doc PRs. -->

- **Risk level**: low / medium / high
- **Rollout plan**: automatic on merge / feature-flag / coordinated
- **Rollback plan**: revert commit / disable flag / release patch

## Checklist

- [ ] I have read [`CONTRIBUTING.md`](../CONTRIBUTING.md)
- [ ] I have read [`AGENTS.md`](../AGENTS.md) and followed the Test-First Mandate
- [ ] My code follows the project's style (`rustfmt.toml`, `clippy.toml`)
- [ ] I have performed a self-review of my own code
- [ ] I have commented non-obvious code paths
- [ ] I have updated the changelog and any affected docs
- [ ] No new warnings introduced (`cargo build`, `cargo clippy`)
- [ ] No new `cargo audit` advisories
- [ ] `cargo deny check` is green
- [ ] Commit messages follow Conventional Commits

## Reviewer Notes

<!-- Anything specific you want the reviewer to focus on. -->
