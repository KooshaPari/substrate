# Substrate Dispatch Migration Plan
**2026-06-30 · Phenotype agent fleet dispatch infrastructure consolidation**

## Executive Summary

This plan proposes adopting **substrate** (KooshaPari/substrate @ ecee354) + **forge-dev** (KooshaPari/forgecode) as the unified dispatch backend for Phenotype agent workflows, replacing the current ad-hoc mix of native `Agent()` tool, `codex exec` direct invocation, and `forge -p` flags.

**Current state:** dispatch is scattered across 3 primitives with no centralized model-tier routing or resilience orchestration.

**Target state:** single dispatch abstraction (MCP tool + CLI + HTTP + Skill) with model-tier routing (heavy/main/worker), cloud-agent support (Cursor/Kilo), and build-contention scheduling hooks via [[project_sharecli]].

**Win:** consolidate ~200 LOC of dispatch boilerplate across 5+ repos into one reusable hexagonal spine; enable structured execution planning (dry-run), inter-agent resource scheduling, and performance tuning per tier.

---

## Current State (as of 2026-06-30)

### Substrate Snapshot

**Repo:** KooshaPari/substrate (HEAD @ ecee354, v2.0.0-ready)

**Maturity:** Release-ready · 150+ tests · clippy clean · 6 driver faces + 10 engine adapters + 6 port traits.

**Three inbound driver faces:**
1. **CLI** (`driver-cli` / `substrate` binary)
   - Subcommands: `plan`, `dispatch`, `cloud-dispatch`, `--dry-run`, `--fake`
   - Local engines: forge, codex, claude, agentapi
   - Cloud engines: cursor, kilo, codex-cloud
   - Status: dogfooded; binary released on `v*` tags

2. **HTTP** (`driver-http` / `substrate-http`)
   - REST API: `/v1/dispatch`, `/v1/plan`, `/v1/route`, `/v1/mailbox/*`, `/healthz`
   - Axum server; composition root wiring; not yet observed in live use

3. **MCP** (`driver-mcp` / FastMCP Python)
   - Tools: `substrate_dispatch`, `substrate_plan`, `substrate_route`, team mailbox (send/inbox/task list)
   - OMNIROUTE_URL config; dispatch tools cap messages at 4096 bytes UTF-8
   - Absorbs `KooshaPari/dispatch-mcp` (per-tier tool aliases: `dispatch_worker`, `dispatch_main`, …)
   - Status: available but not wired into Phenotype workflow runners yet

**Hexagonal core** (`substrate-core`):
- Port traits: `EnginePort`, `RoutingPort`, `TransportPort`, `StorePort`, `DispatchApi`, `SchedulePort`, `WorkflowPort`, `ClaimPort`, `SkillPort`, `MemoryPort`, `ProcessPort`, `WatcherPort`, `EventStorePort`
- Routing superset: round-robin / weighted / least-used / power-of-two-choices, per-target circuit breaker (Closed/Open/HalfOpen), weighted fallback chain
- `DispatchPlanner`: engine + session-mode selection (Background/Foreground/InProcess)
- Event sourcing: append-only event log, duplicate-seq rejection, `TaskLifecycleProjection` demo

**Orchestration superset** (2026-06):
- `SchedulePort`: cron/interval/daily/weekly via croner
- `WorkflowPort`: petgraph DAG topo order, ready-set, cycle detection
- `ClaimPort`: BEGIN IMMEDIATE atomic claim + strsim fuzzy dedup (SQLite + file-based lockfile)
- `SkillPort`: named invokable skills with JSON schema validation
- `MemoryPort`: bounded ring buffer + two-tier compose with SQLite persistent tier

**Known issue (feedback_substrate_dogfood.md):** The CodexEngine argv builder emits `--prompt <p>` but installed `codex exec` expects positional prompt. Fix needed before live `--tier` dispatch runs.

---

### Forgecode (forge-dev) Snapshot

**Repo:** KooshaPari/forgecode (HEAD @ 9326a72da, v2.13.14)

**Maturity:** Production-ready · 33-crate workspace · Phenotype-org fork of upstream tailcallhq/forgecode.

**Phenotype additions:**
- SQLite session store with WAL + zstd compression
- Conversation FTS/vector search
- Subagent breadcrumbs (conversation-id + fork pointers)
- deny.toml + cargo-deny CI

**CLI interface:**
- `forge exec <prompt>`: invoke as executor
- Builtin conversation management, provider routing, session persistence
- Model aliases and fallback strategies
- Integrates with OpenAI, Anthropic, Google, AWS Bedrock, and others

**Auth model:** credentials stored locally (~/.forge/.credentials.json), never embedded.

---

### Current Dispatch Patterns (Phenotype workflows)

Three ad-hoc primitives:

1. **Agent() tool** (Claude Code native)
   - Used in: parent-session coordination, quick subagent spawns
   - Limitation: no structured output contract, no dry-run, no tier routing
   - Cost: Opus-tier LLM (expensive for workers)

2. **codex exec** (direct invocation via Bash)
   - Syntax: `codex exec --skip-git-repo-check --enable exec_permission_approvals --dangerously-bypass-approvals-and-sandbox <<'PROMPT' ...`
   - Concurrency: 20-30 workers viable; gpt-5.5 default
   - Limitation: no standard arg parsing for model tier, dry-run, or routing strategy
   - Status: proven pattern (feedback_codex_dispatch_pattern.md); working but brittle

3. **forge -p** (flag-based dispatch)
   - Limited coverage; mostly forgotten in favor of codex exec
   - No tier routing or circuit breaking

**Cross-cutting gaps:**
- No centralized routing decision logic
- No dry-run / plan introspection
- No inter-task resource scheduling (build contention)
- No structured task lifecycle or persistence
- Tier routing (`heavy/main/worker`) is manual or absent

---

## Target State (Proposed)

### Unified Substrate-Based Dispatch

**Single entry point:** `substrate dispatch` (via CLI, HTTP, MCP tool)

**Model-tier routing (3-tier):**
- **heavy:** gpt-5.5, reasoning_effort=low (synthesis, architecture review)
- **main:** gpt-5.4-mini, low (typical multi-step tasks)
- **worker:** gpt-5.3-codex-spark, medium (high-concurrency leaf tasks)

**Three inbound adapters (pick one per context):**

1. **MCP tool (for Phenotype workflows)**
   ```
   @mcp
   tool substrate_dispatch(tier, cwd, task)
     → DispatchPlan (engine, session_mode, argv) + StructuredResult JSON
   ```
   - Wired into OmniRoute MCP server (or standalone server)
   - Used by: Claude agents, AgilePlus plan execution, phenotype-router skill composition
   - Advantage: structured output, dry-run support, tiers as first-class citizen

2. **CLI (for local shells, CI, ad-hoc dispatch)**
   ```bash
   substrate dispatch --tier main --cwd . --mode background "implement feature X"
   substrate plan --engine forge --cwd . "echo hi"   # dry-run introspection
   ```
   - Dogfooded binary in ~/.local/bin/substrate
   - Advantage: simple, no server needed, scripting-friendly

3. **HTTP API (for polyglot / remote dispatch)**
   ```
   POST /v1/dispatch { tier, cwd, task, mode?, dry_run? }
     → { plan, result?, error? }
   ```
   - Optional; useful if dispatch needs to cross network boundaries
   - Status: implemented, untested in production

**Engines (adapt from substrate):**
- **forge** (primary): forge-dev fork; rich conversation mgmt, multi-provider, proven
- **codex:** substrate-native adapter; uses installed codex binary
- **claude:** substrate-native adapter; uses installed claude binary
- **agentapi:** HTTP bridge to agentapi-plusplus (for local/cloud flexibility)
- **cursor, kilo, codex-cloud:** cloud dispatch adapters (future; substrate already has stubs)

**Orchestration layers (dogfooded into workflows):**
- **Routing:** phenotype-router decision logic plugged into substrate `RoutingPort`
- **Build contention:** sharecli semaphore hooks in `ProcessPort` (serialize cargo/ruff/git-lock)
- **Persistence:** store-sqlite for task lifecycle, event log, dedup claims
- **Scheduling:** substrate-schedule for periodic/cron dispatch (future)

---

## Gap Analysis: Substrate → Drop-In Replacement

### What Substrate Has ✅

| Capability | Status | Notes |
|------------|--------|-------|
| Multiple engines (forge, codex, claude, agentapi) | ✅ | Adapters complete; tested offline |
| Cloud dispatch (cursor, kilo, codex-cloud) | ✅ | Stubs present; wiring pattern clear |
| Structured task/result contracts | ✅ | `TaskSpec`, `StructuredResult`, `TaskState` FSM |
| Dry-run / plan introspection | ✅ | `DispatchPlanner::plan()` + `--dry-run` CLI flag |
| Routing superset (circuit breaker, weighted fallback) | ✅ | `routing_port` + `RoutingDecision` |
| Event sourcing (task lifecycle) | ✅ | `EventStorePort` + `SqliteEventStore` |
| Skill / memory composition | ✅ | `SkillPort`, `MemoryPort` with two-tier ring buffer |
| Per-target retry / backoff | ✅ | Circuit breaker Closed/Open/HalfOpen + fallback chain |
| Process management (spawn, monitor, kill) | ✅ | `ProcessPort` with `command-group`, timeout, status polling |
| CLI binary (dogfooded) | ✅ | `target/release/substrate`; tested daily by maintainer |

### What Needs Wiring or Tuning ⚠️

| Gap | Severity | Solution |
|-----|----------|----------|
| **CodexEngine argv bug** (--prompt → positional) | HIGH | Fix substrate PR; rebuild; re-test --tier runs |
| **MCP dispatch tool tier routing** | HIGH | Expose all 3 tiers as separate tools + `dispatch_custom(tier, msg)` (done in driver-mcp) |
| **Build contention scheduling** | MEDIUM | Hook `ProcessPort` adapter to sharecli semaphore; serialize cargo/ruff/mypy/git-lock |
| **Phenotype-router integration** | MEDIUM | Implement `RoutingPort` adapter wrapping phenotype-router's decision layer; test with real combo rules |
| **HTTP API auth + TLS** | MEDIUM | Axum server; add JWT/API-key middleware + optional TLS; not urgent if CLI/MCP sufficient |
| **Performance perf-measurement** | LOW | Add timing hooks to `TracePort`; emit to OmniRoute telemetry for forge vs codex comparison |
| **Multi-tier load testing** | LOW | Benchmark concurrency limits per tier; profile memory/CPU per model; establish SLO dashboard |

### What Substrate Does NOT Have (and does not need)

| Feature | Why not needed | Fallback |
|---------|---|---|
| Embedded auth provider list | Phenotype owns auth via OmniRoute | Use config/env injection |
| LLM-specific prompt templates | Domain-specific; out of scope | Caller provides `TaskSpec` with full prompt |
| Built-in observability dashboard | Phenotype-router + OmniRoute have dashboards | Emit events via `TracePort` to existing systems |
| Auto-scaling pool | Local machine; manual dispatch | Controlled by caller (phenodag, phenofleet) |

---

## Phased Migration DAG

### Phase 1: Setup & Verification (Week 1)
**Goal:** Substrate ready for pilot; forge-dev proven in this session.

**Tasks:**
1. **Fix CodexEngine argv bug** (substrate PR or local patch)
   - File: `substrate/crates/engine-codex/src/lib.rs`
   - Change: argv builder; drop `--prompt` flag; pass prompt positionally
   - Test: `substrate dispatch --tier main --fake "echo hi"` → success
   
2. **Verify forge-dev CLI works end-to-end**
   - Clone forgecode; `cargo build --release -p forge_main`
   - Test: `forge exec --skip-git-repo-check "hello world"`
   - Ensure integration tests pass: `cargo nextest run`

3. **Verify substrate MCP tools load**
   - `cd substrate/driver-mcp && pip install -r requirements.txt`
   - Test: `export OMNIROUTE_URL=http://localhost:20128 && python dispatch_server.py`
   - Verify tools advertise correctly via `curl http://localhost:3001/tools`

4. **Document current dispatch patterns in Phenotype**
   - Audit: how many repos use Agent() vs codex exec vs forge -p?
   - Record in ADR (decision + rationale)

**Blockers:** None expected; all pieces are stable.

---

### Phase 2: Pilot Integration (Week 1-2)
**Goal:** Route THIS session's dispatches through substrate; measure latency, error rates, outputs.

**Tasks:**
1. **Wire substrate MCP into OmniRoute** (or standalone server)
   - Absorb driver-mcp tools into OmniRoute's MCP server (or bind a separate FastMCP instance)
   - Expose as `@mcp substrate_dispatch(tier, cwd, task)` tool
   - Config: OMNIROUTE_URL, model tier defaults

2. **Refactor phenofleet to use substrate dispatch**
   - Current: raw `codex exec` calls in Bash loops
   - Target: wrapper function that calls `substrate dispatch --tier worker`
   - Benefit: structured logging, dry-run preview, fallback routing

3. **Test pilot dispatch in THIS session**
   - Issue 3 dispatches per tier (heavy, main, worker) with substrate
   - Compare latency, cost, output quality vs baseline codex exec
   - Record: tier vs observed model behavior (e.g., does heavy actually use reasoning?)

4. **Measure concurrency ceiling per tier**
   - Spin up 20 worker-tier tasks in parallel via substrate
   - Monitor: CPU, memory, API rate limit errors
   - Record: max concurrent tasks before throttle

**Blockers:** 
- CodexEngine argv bug (blocks codex engine; forge unaffected)
- sharecli integration (nice-to-have; pilot can run without it)

---

### Phase 3: Cutover (Week 2-3)
**Goal:** Replace Agent() + codex exec in all workflows; retire old primitives.

**Tasks:**
1. **Update Phenotype AGENTS.md / CLAUDE.md**
   - Deprecation notice for Agent() (Opus too expensive; use substrate heavy tier instead)
   - Deprecation notice for bare `codex exec` (use `substrate dispatch --tier X`)
   - Recommended: CLI for local shells, MCP tool for agent workflows

2. **Migrate repo dispatches**
   - thegent-dispatch, Eidolon, sharecli: replace Agent() calls with substrate MCP
   - phenotype-registry, phenotype-registry-phenoforge-final: replace codex exec with substrate CLI
   - OmniRoute skill composition: use substrate_dispatch MCP tool for multi-step tasks

3. **Retire old dispatch wrappers**
   - Delete any in-repo codex exec boilerplate (e.g., dispatch-mcp absorbed into substrate)
   - Remove Agent() import from agent templates

4. **Update CI/CD**
   - GitHub Actions workflows: use `substrate dispatch --tier main` for long-running jobs
   - Locally: CI cache via substrate event log (dry-run for parity checks)

**Blockers:** 
- Phenotype-router integration (blocks Combo routing strategy); can defer to phase 4

---

### Phase 4: Performance Optimization (Week 3-4)
**Goal:** Tune for packing density; reduce contention; measure total-cost improvement.

**Tasks:**
1. **Integrate sharecli build semaphore**
   - Hook substrate `ProcessPort` adapter to sharecli syscall-level scheduling
   - Benefit: 20+ concurrent agents; only 1-2 cargo builds at a time
   - Measurement: elapsed time for 50-task parallel sweep with vs without

2. **Integrate phenotype-router decision logic**
   - Implement substrate `RoutingPort` adapter wrapping phenotype-router's combo rules
   - Test: auto-combo strategy selection via substrate (not hardcoded tier)
   - Benefit: per-task model selection based on cost, latency, provider health

3. **Establish tier-per-workload mapping**
   - Document: which task types use which tier (heavy for arch review, main for typical, worker for map-reduce)
   - SLO dashboard: tier latency/cost/quality percentiles
   - Cost forecast: X workers * Y hours/day * Z cost/worker = expected monthly spend

4. **Enable event sourcing for audit trail**
   - SQLite event log: every dispatch decision, model choice, fallback trigger
   - Query: "why did task X use model Y?" (trace chain)
   - Export: telemetry to OmniRoute observability for cost/latency correlation

**Blockers:** 
- sharecli maturity (currently R&D; needs hardening)
- Phenotype-router refactor (nice-to-have for phase 4; can ship phase 3 without it)

---

## Resource Allocation

### Effort Estimate (agent-driven)
| Phase | Work | Tier | Est. Duration |
|-------|------|------|---|
| 1 | Fix + verify + audit | codex/forge | 1-2 hours |
| 2 | Pilot + measurement | codex/main | 3-4 hours |
| 3 | Cutover (5 repos) | codex/main | 2-3 hours |
| 4 | Optimization + tuning | forge/main | 4-6 hours |
| **TOTAL** | | | **~12-16 hours** |

### Repositories Impacted
| Repo | Change | Timing |
|------|--------|--------|
| KooshaPari/substrate | Fix CodexEngine argv; wire MCP into ecosystem | Phase 1-2 |
| KooshaPari/forgecode | Ensure dogfooding; document CLI contract | Phase 1 |
| KooshaPari/phenofleet | Replace codex exec with substrate CLI | Phase 3 |
| KooshaPari/OmniRoute | Absorb substrate MCP; wire skill composition | Phase 2-3 |
| KooshaPari/thegent-dispatch | Replace Agent() with substrate heavy tier | Phase 3 |
| KooshaPari/Eidolon | Replace Agent() with substrate main tier | Phase 3 |
| KooshaPari/sharecli | Hook ProcessPort for build contention | Phase 4 (optional) |

---

## Success Criteria

### Phase 1
- ✅ CodexEngine argv fix merged and tested
- ✅ Substrate MCP tools advertise correctly
- ✅ forge-dev CLI invocation succeeds 10/10 times
- ✅ Audit document lists all current dispatch patterns

### Phase 2
- ✅ Substrate dispatch latency ≤ 2x codex exec baseline (accounting for planner overhead)
- ✅ Cost per task ≤ baseline (tier matching is accurate)
- ✅ Max concurrency: 20+ worker-tier tasks without rate-limit errors
- ✅ Structured output contract satisfied (result JSON parseable by all callers)

### Phase 3
- ✅ Zero Agent() calls in agent template code
- ✅ All phenofleet dispatches routed through substrate CLI
- ✅ OmniRoute MCP server exposes substrate_dispatch tool
- ✅ Deprecation notices in AGENTS.md / CLAUDE.md

### Phase 4
- ✅ sharecli semaphore integration working (cargo serialize; agents fan out)
- ✅ phenotype-router combo rules queryable via substrate routing layer
- ✅ Event log: 100% dispatch decisions recorded
- ✅ Cost forecast: < baseline (better packing density)

---

## Cross-Links & Assumptions

### Blocked By / Enables
- **Blocks:** [[project_sharecli]] (waiting for dispatch consolidation; sharecli integrates into ProcessPort)
- **Enables:** [[feedback_no_memory_scare]] (substrate + sharecli = no agent-count throttling; only build-contention queuing)
- **Related:** [[feedback_codex_dispatch_pattern]] (codex exec pattern; substrate formalizes it)
- **Related:** [[project_phenofleet]] (dispatch is the leaf; phenofleet orchestrates the DAG)

### Assumptions
1. **Substrate code is stable** (v2.0.0-ready, 150+ tests; risk: low)
2. **forge-dev inherits upstream reliability** (tailcallhq/forgecode proven; risk: low)
3. **OmniRoute can absorb one more MCP server** (or run standalone; risk: low)
4. **Model tier mapping (heavy/main/worker) is correct** (GPT tiers move fast; risk: medium → monitor monthly)
5. **Build contention is the real bottleneck** (not agent count; risk: unproven → phase 2 testing will validate)

---

## Rollback Plan

If substrate dispatch causes widespread failures:

1. **Immediate:** keep native dispatch primitives (Agent(), codex exec) available in parallel (feature flag)
2. **Revert to phase 0:** document lessons learned; defer migration to next quarter
3. **Partial rollback:** if only one engine fails (e.g., codex), disable it; use forge/claude instead

---

## References & Reading Order

**Required reading before implementation:**

1. substrate README & architecture (especially hexagonal crates table)
2. forgecode CLAUDE.md (fork notes, stack, crate map)
3. feedback_substrate_dogfood.md (argv bug details + fix location)
4. feedback_codex_dispatch_pattern.md (proven pattern that substrate formalizes)
5. feedback_no_memory_scare.md (why agent count ≠ bottleneck; build contention is)
6. project_sharecli.md (how ProcessPort hooks into sharecli)
7. project_phenofleet.md (how phenofleet will consume substrate)

---

## Next Steps (Handoff)

### For the implementer:
1. Read the 7 references above (30 min)
2. Fix CodexEngine argv bug (20 min)
3. Run phase 1 verification tasks (1 hour)
4. Open PR to substrate with argv fix + test (5 min)
5. Run phase 2 pilot dispatches in session (2 hours)
6. Record measurements; report back to team lead

### For the team lead:
1. Review this plan; approve phase 1-2 (no external blockers expected)
2. Assign phase 3-4 to codex workers once phase 2 data is in
3. Monitor cost + latency metrics weekly; adjust tier mapping if needed
4. Plan quarterly review of dispatch routing efficiency

---

**End of plan. This document is the SSOT for the dispatch migration. Update it as phases complete and new gaps emerge.**
