# F3: Substrate Dispatch Integration into OmniRoute MCP

**Status:** Design complete, awaiting OmniRoute worktree integration  
**Date:** 2026-07-01  
**Scope:** Wire substrate dispatch tools into OmniRoute's MCP server

---

## Overview

Substrate dispatch (F1+F2 phases complete) provides tier-based model routing via HTTP API and CLI. **F3 goal:** expose substrate dispatch as MCP tools in OmniRoute, enabling agent sessions to dispatch work through substrate with model-tier routing (heavy/main/worker).

**Current state:**
- Substrate HTTP API ready: `POST /v1/dispatch`, `POST /v1/plan`, `GET /healthz`
- Substrate CLI working end-to-end with codex, forge backends
- OmniRoute MCP server has 87+ tools, modular tool registration
- Integration design complete (see below)

---

## Integration Design

### MCP Tools to Add (3 tools)

| Tool | Purpose | Inputs |
|------|---------|--------|
| `substrate_dispatch` | Execute prompt via tiered routing | prompt (required), tier?, engine?, cwd? |
| `substrate_plan` | Dry-run dispatch planning | prompt (required), engine?, cwd? |
| `substrate_health` | Health check | (none) |

### Implementation Files

1. **New file:** `OmniRoute/open-sse/mcp-server/tools/dispatchTools.ts`
   - Export `const dispatchTools: McpToolExtraLike[]`
   - Wraps substrate HTTP API client
   - Calls `POST /v1/dispatch`, `POST /v1/plan`, `GET /healthz`
   - Error handling + audit logging

2. **Modified:** `OmniRoute/open-sse/mcp-server/server.ts`
   - Import `dispatchTools`
   - Add to `TOTAL_MCP_TOOL_COUNT`
   - Add tool names to `RESERVED_MCP_NAMES` set
   - Register tools with `.forEach()` loop (pattern: pluginTools registration)

### Code References

**dispatchTools structure:**
```typescript
export const dispatchTools: McpToolExtraLike[] = [
  {
    name: "substrate_dispatch",
    description: "Dispatch prompt to substrate with tier-based routing",
    inputSchema: { type: "object", properties: {...}, required: ["prompt"] },
    handler: async (input, extra) => handleSubstrateDispatch(...),
  },
  // ... substrate_plan, substrate_health
];
```

**Server registration pattern:**
```typescript
import { dispatchTools } from "./tools/dispatchTools.ts";

// In initialization:
TOTAL_MCP_TOOL_COUNT += dispatchTools.length;
RESERVED_MCP_NAMES.add(...dispatchTools.map((t) => t.name));

// In tool registration section:
dispatchTools.forEach((toolDef) => {
  server.registerTool(toolDef.name, {...}, withScopeEnforcement(...));
});
```

---

## Prerequisites

### Environment
- `SUBSTRATE_HTTP_URL` env var set to substrate HTTP server (e.g., `http://localhost:8000`)
- Substrate HTTP driver running (`cargo run --bin substrate-http` or via systemd)

### Substrate HTTP API Contract

**Request:**
```json
POST /v1/dispatch
Content-Type: application/json

{
  "prompt": "string (required)",
  "tier": "heavy|main|worker (optional)",
  "engine": "forge|codex|claude|agentapi (optional)",
  "cwd": "/path (optional, default: cwd)"
}
```

**Response:**
```json
200 OK
{
  "text": "completion text",
  "artifacts": [{name, uri}, ...],
  "pr_urls": ["url", ...],
  "status": "submitted|running|completed|failed"
}
```

**Error:**
```json
{4xx|5xx}
API [STATUS]: error message
```

---

## Testing Strategy

### Unit Tests (Optional for F3)
- Mock substrate HTTP API responses
- Verify tool parameter parsing
- Check error handling paths

### Integration Tests (Phase 3)
1. Start substrate HTTP server
2. Start OmniRoute with `SUBSTRATE_HTTP_URL=http://localhost:8000`
3. Call MCP tools via OmniRoute client (Cursor, Claude Desktop, or programmatic)
4. Verify round-trip (dispatch → substrate → backend engine → result)

### Live Test (Validation)
```bash
# Terminal 1: Start substrate HTTP server
cargo run --bin substrate-http --release

# Terminal 2: Set env and start OmniRoute
export SUBSTRATE_HTTP_URL=http://localhost:8000
npm run dev

# Terminal 3: Call via Claude Desktop or programmatic MCP client
# Tool: substrate_dispatch
# Input: { prompt: "Print: F3-OK", tier: "main" }
# Expected: { text: "F3-OK\n", status: "completed", ... }
```

---

## Handoff to Phase 3 (Full Cutover)

After F3 integration verification, phase 3 will:
1. Add per-tier convenience tools (dispatch_worker, dispatch_main, dispatch_heavy)
2. Integrate with phenofleet + OmniRoute workflow orchestration
3. Add usage examples to README
4. Update AGENTS.md with deprecation notices (Agent(), bare codex exec)
5. Measure latency / cost vs baseline (codex exec)

---

## Known Limitations & Future Work

| Item | Severity | Phase |
|------|----------|-------|
| MCP tools capped at 4096 bytes per message | LOW | 3+ (increase via substrate config) |
| No streaming support (full output on completion) | MEDIUM | 4 (add SSE streaming via substrate) |
| Per-tier tools not aliased (use dispatch_custom) | LOW | 3 (add dispatch_worker, etc.) |
| Artifact/PR_URL extraction not exercised | LOW | 4 (validate with real LLM outputs) |

---

## Files & Locations

- **Source (substrate):** `/repos/substrate/docs/F3_OMNIROUTE_INTEGRATION.md`
- **Reference (dispatch mcp):** `/repos/substrate/driver-mcp/dispatch_mcp/server.py` (Python MCP reference)
- **Reference (omniroute):** `/repos/OmniRoute/open-sse/mcp-server/tools/*.ts` (tool patterns)
- **HTTP API spec:** `/repos/substrate/crates/driver-http/src/lib.rs` (Axum routes)

---

## Checklist for Phase 3

- [ ] OmniRoute on `main` or clean release branch (not stale PR)
- [ ] `dispatchTools.ts` added to OmniRoute/open-sse/mcp-server/tools/
- [ ] `server.ts` updated (import, TOTAL_MCP_TOOL_COUNT, RESERVED_MCP_NAMES, registration)
- [ ] Type check passes: `npm run typecheck:core`
- [ ] Lint passes: `npm run lint`
- [ ] Substrate HTTP server runnable locally
- [ ] Live test: substrate_dispatch tool invoked end-to-end
- [ ] PR merged to main with integration guide updated
- [ ] README.md updated with substrate dispatch examples
- [ ] AGENTS.md updated with deprecation notices

---

## Success Criteria (F3)

✅ Substrate dispatch tools callable from OmniRoute MCP  
✅ Tool calls routed to substrate HTTP API correctly  
✅ Errors handled gracefully (missing SUBSTRATE_HTTP_URL, API unreachable)  
✅ Response contract matched (text, artifacts, status)  
✅ Example usage documented  

---

**Next:** Phase 3 worker picks up checklist above and completes OmniRoute integration.
