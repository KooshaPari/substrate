# Quarantined Tests

This directory contains tests that have been quarantined due to flakiness.
See `TEST_QUARANTINE.md` at repo root for the full quarantine policy.

Tests in this directory:
- Are not run in CI by default
- Are run nightly to track quarantine age
- Must be fixed or removed within 30 days

To quarantine a test, move it here and add `#[ignore = "flaky: <reason>"]`.
