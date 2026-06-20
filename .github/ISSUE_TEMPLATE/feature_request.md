---
name: Feature Request
about: Suggest a new command, flag, or behavior for sharecli
title: 'feat: '
labels: ['enhancement', 'triage']
assignees: []
---

## Summary

<!-- One-sentence description of the proposed feature. -->

## Motivation / Problem

<!-- What pain point does this solve? Who is affected and how often? -->

## Proposed Solution

<!-- Describe the desired behavior, command shape, and flags. -->

### Example

```bash
sharecli <new-command> --<flag> <value>
```

### Sketch

```rust
// Optional: rough shape of the API or CLI surface
```

## Alternatives Considered

<!-- What other approaches did you consider, and why is this one better? -->

## Impact

- [ ] Adds a new top-level command
- [ ] Adds flags to an existing command
- [ ] Changes default behavior
- [ ] Affects public configuration (`~/.config/sharecli/config.toml`)
- [ ] Affects `process-compose.yml` generation

## FR-#### Reference

<!-- If you opened this for a known FR, link it. Optional. -->
<!-- Example: FR-007 — runtime pool metrics -->

## Acceptance Criteria

- [ ] Updated tests cover the new behavior
- [ ] `CHANGELOG.md` and `README.md` updated
- [ ] No regression in existing commands (`just test` green)
- [ ] Backward compatible (or migration note added)

## Priority

- [ ] P0 — blocking real work
- [ ] P1 — needed for the next minor release
- [ ] P2 — nice to have
- [ ] P3 — research / spike
