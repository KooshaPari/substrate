# Functional Requirements

Authoritative FR catalog for substrate. Each entry is addressed by spec, code, and tests; gap closure is recorded in the FR's Status. New requirements are added here, not in code comments.

ID convention: `FR-NNN` (zero-padded, monotonic). Owner is the Rust workspace member that ships the FR. Acceptance is the smallest, agent-runnable check that proves it ships.

## Catalog

| ID | Title | Owner | Acceptance | Status |
|----|-------|-------|------------|--------|
| FR-001 | Deterministic dispatch plan | `substrate-core` | Given a `TaskSpec`, `DispatchPlanner::plan` returns the same `(engine, session_mode, argv)` for the same inputs across runs and platforms | accepted |
| FR-002 | Engine adapter conformance | `engine-conformance` | Every `Engine` impl passes the shared `engine-conformance` suite (forge / codex / claude / agentapi / codex-cloud) | accepted |
| FR-003 | CLI dispatch JSON contract | `driver-cli` | `substrate dispatch "..."` prints a single JSON object on stdout (parseable, no trailing tokens) and exits 0 on success | accepted |
| FR-004 | HTTP REST surface | `driver-http` | `GET /health` returns 200 JSON; `POST /admin/...` requires `ADMIN_TOKEN`; bind honored via `SUBSTRATE_HTTP_BIND` | accepted |
| FR-005 | Friction intake | `.github/ISSUE_TEMPLATE/user-friction.yml` | A new GitHub issue with the `friction:UX` label can be filed from the issue picker; reported rows are added to `docs/friction-log.md` | accepted |
| FR-006 | Agent entrypoint | `AGENTS.md` | An autonomous agent reading only `AGENTS.md` can build, test, lint, and create a worktree on a fresh clone without further prompts | accepted |
| FR-007 | User-story coverage | `USER_JOURNEYS.md` | Operator, integrator, and runtime-author personas each have >=1 documented story with an executable command and a measurable outcome | proposed |
| FR-008 | Visual identity (CLI) | `VISUAL_SPEC.md` | Every CLI subcommand renders a banner on `--help`; stderr/stdout split follows §2 of `VISUAL_SPEC.md` | proposed |

## Status legend

- **proposed** — listed but not yet shipped; tracked in a GitHub issue.
- **accepted** — meets acceptance; spec/code/tests aligned.
- **deprecated** — superseded by a newer FR; kept for history.

## How to add an FR

1. Pick the next `FR-NNN` (max + 1).
2. Add a row above with `Status: proposed`.
3. Link the FR in the PR's `## FR-IDs` section (`.github/PULL_REQUEST_TEMPLATE.md`).
4. When acceptance holds in CI, edit the row to `Status: accepted`.

This file is the single source of truth for "what substrate does." Do not duplicate the list in `SPEC.md` — `SPEC.md` links here.
