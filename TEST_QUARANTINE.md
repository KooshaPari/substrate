# Test Quarantine Policy

This document defines how substrate manages flaky tests in CI.

## Quarantine Process

1. **Detection**: A test is marked flaky when it fails in CI but passes locally
   or passes on retry. The flaky-test workflow automatically re-runs failed tests
   once.

2. **Quarantine**: If a test fails 3+ times across 50 PR runs, it is quarantined:
   - Add `#[ignore = "flaky: <reason>"]` attribute with the failure pattern
   - Add a tracking comment linking to the investigation issue
   - Move to `tests/quarantined/` namespace if it's a top-level test

3. **Re-enable**: A quarantined test must be either fixed or removed within 30 days.
   - If fixed: remove the `#[ignore]` attribute and verify the fix holds
   - If removed: delete the test entirely with a clear commit message

4. **Tracking**: All quarantined tests are listed in `tests/QUARANTINE.md` with:
   - Test name and location
   - Date quarantined
   - Failure pattern observed
   - Linked investigation issue
   - Owner responsible for resolution

## CI Integration

The `.github/workflows/ci.yml` workflow:
- Runs all tests, including quarantined ones, but quarantined tests don't gate merge
- Posts a summary comment on PRs with the list of quarantined tests that ran
- Tracks quarantine age and flags tests approaching the 30-day limit

## Owner Responsibility

Each quarantined test has an assigned owner who must:
- Investigate root cause within 7 days
- File an issue for tracking
- Update the `QUARANTINE.md` file with progress notes
- Either fix or remove the test before the 30-day deadline
