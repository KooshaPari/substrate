# User Journeys

Real user stories for substrate. Each row identifies the actor, what they want, the value delivered, and a runnable command or observable outcome. Stories derive the functional requirements in [`functional_requirements.md`](./functional_requirements.md) — when a story stops matching the CLI/HTTP behavior, the FR is the thing to update, not this file's narrative.

## Stories

| # | As a... | I want to... | so that... | Story |
|---|---------|--------------|------------|-------|
| 1 | operator | run `substrate plan "..."` and see the chosen engine without dispatching | I can verify the planner's decision (FR-001) before spawning a child process or burning tokens | `substrate plan "fix the typo in README" --cwd .` → JSON `{engine, session_mode, argv}` on stdout; exit 0; no child process spawned |
| 2 | operator | dispatch a prompt and receive a single JSON object on stdout (FR-003) | downstream wrappers (`jq`, `codex cloud`, thegent) can pipe substrate results without parsing prose | `substrate dispatch "echo hi" --fake --cwd /tmp` → pretty JSON containing `engine`, `output`, `success`; exit 0; stderr empty on success |
| 3 | integrator | bind the HTTP surface on a custom port and hit `GET /health` | I can wire substrate into a process-compose stack without code changes | `SUBSTRATE_HTTP_BIND=127.0.0.1:7777 cargo run -p driver-http` then `curl -fsS http://127.0.0.1:7777/health` → 200 JSON `{"status":"ok"}` |
| 4 | runtime author | subclass the `Engine` port and have my adapter picked up by the conformance suite (FR-002) | a new engine lands with the same dispatch semantics as forge/codex/claude | implement `engine_spec::Engine`, register in `engine-conformance` matrix, `cargo test -p engine-conformance` passes the existing assertions |
| 5 | triage agent | file a UX gap via the issue picker with a single label | reported friction lands in `docs/friction-log.md` without me leaving the GitHub UI | open an issue, pick "User friction", apply `friction:UX` automatically; row appended on triage (AGENTS.md → "Forbidden" still applies: no AI-authored issue bodies) |
| 6 | cloud-agent operator | invoke `substrate cloud-dispatch` to push a task to Codex Cloud and harvest the resulting PR as JSON | I can fan a long-running task into the cloud without polling a dashboard | `substrate cloud-dispatch codex --repo KooshaPari/substrate --branch chore/x --task "open a PR that bumps X"` → JSON `{pr_url, status, commit_sha}`; exit 0 on success, non-zero with structured error on failure |

## Persona map

| Persona | Goals | Story rows |
|---------|-------|------------|
| Operator | Visible behavior, predictable JSON, no surprises | 1, 2, 6 |
| Integrator | Pluggable ports, REST + CLI parity, env-driven config | 2, 3, 4 |
| Runtime author | Adapter conformance, single contract across engines | 4 |
| Triage / documentation agent | Friction intake lands in the right file | 5 |

## How to add a story

1. Add a row above; keep the prose terse (one line per "so that").
2. The Story column must be a runnable command or an HTTP-call assertion. If you can't run it, the story isn't ready.
3. Reference any new or existing `FR-NNN` from the Why.
4. If a story no longer matches shipped behavior, change the FR — do not edit the story to lie.
