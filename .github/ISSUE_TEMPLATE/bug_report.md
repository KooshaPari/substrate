---
name: Bug Report
about: Report something that is not working in sharecli
title: 'bug: '
labels: ['bug', 'triage']
assignees: []
---

## Summary

<!-- One-sentence description of the bug. -->

## Environment

- sharecli version: `sharecli --version` → _paste output_
- Rust toolchain: `rustc --version && cargo --version` → _paste output_
- OS / distribution: (e.g. macOS 14.5, Ubuntu 24.04, Windows 11)
- Installation method: (e.g. `cargo install sharecli`, source build, GitHub release)
- Relevant config: `~/.config/sharecli/config.toml` excerpt (redact secrets!)

## Steps to Reproduce

```bash
# Minimal reproduction — paste the exact commands
sharecli <command> <args>
```

1.
2.
3.

## Expected Behavior

<!-- What you expected to happen. -->

## Actual Behavior

<!-- What actually happened. Paste the relevant log/error output. -->

```text
<paste log or error here>
```

## Possible Cause

<!-- If you have a hunch about the root cause, share it. Optional. -->

## FR-#### Reference

<!-- If this maps to a tracked functional requirement, note it here. Optional. -->
<!-- Example: FR-003 — limits set/clamp behavior -->

## Acceptance Criteria

- [ ] Reproduction is consistent and minimal
- [ ] Issue title follows `sharecli(<area>): <one-liner>` convention
- [ ] Logs / backtraces are attached (with secrets redacted)

## Severity

- [ ] Blocker — CLI is unusable
- [ ] High — major feature broken
- [ ] Medium — workaround exists
- [ ] Low — cosmetic / minor
