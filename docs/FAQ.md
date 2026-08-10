# Frequently asked questions

## Which command should I run first?

Run `just build` to compile the workspace and `just test` for the full test
profile. `just lint` runs the strict formatting and clippy checks used by CI.

## Where does the gateway listen?

The HTTP gateway uses the configured bind address from the environment. Check
`docs/operations/runbook.md` for health probes and deployment defaults; never
copy production credentials into a local `.env` file.

## How is an engine selected?

The routing policy evaluates the task's requested tier and provider
capabilities, then applies the configured fallback order. The decision is
recorded in the structured result so operators can explain a route.

## How do I add a provider?

Implement the appropriate port in a dedicated adapter crate, add a focused
conformance test, and document configuration and failure behavior. Do not call
provider APIs from the gateway or core domain directly.

## What should I do when a task appears stuck?

Check the task state, lease owner, and gateway health endpoint first. Follow the
recovery and rollback procedures in `docs/operations/runbook.md` and
`docs/operations/rollback.md`; do not delete state as a first response.

## Where are metrics and service targets documented?

The gateway exposes JSON and Prometheus metrics endpoints. Definitions,
targets, error budgets, and alert guidance live in `docs/SLO.md`.

## How are vulnerabilities reported?

Use the private process in `SECURITY.md`. Do not publish credentials or a
security report in an issue or pull request.
