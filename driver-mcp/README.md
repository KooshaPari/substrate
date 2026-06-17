# driver-mcp

Python MCP servers for substrate inbound adapters.

> **Canonical SSOT:** Deployable MCP server packages live in
> [PhenoMCPServers](https://github.com/KooshaPari/PhenoMCPServers) (`servers/substrate/`).
> This tree is a **runtime convenience copy** for substrate development.
> Per [ADR-019](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/019-mcp-runtime-implementation-deps.md),
> long-term wiring imports from PhenoMCPServers — do not fork tool definitions here.

## Servers

| Module | Purpose |
|--------|---------|
| `lead_server.py` | Lead-facing team mailbox (send, inbox, task list) |
| `team_mailbox_server.py` | Worker-facing team mailbox |
| `dispatch_server.py` | Tier-based OmniRoute dispatch (ported from `dispatch-mcp`) |

## Dispatch MCP (OmniRoute tiers)

Absorbs `KooshaPari/dispatch-mcp`. Exposes per-tier tools (`dispatch_worker`, `dispatch_main`, …), `dispatch_custom`, `dispatch_health`, and `dispatch_liveness`.

### Tools

| Tool | Tier |
|------|------|
| `dispatch_worker` | `worker` |
| `dispatch_main` | `main` |
| `dispatch_codeman` | `codeman` |
| `dispatch_freetier` | `freetier` |
| `dispatch_kimi` | `kimi` |
| `dispatch_kimi_thinking` | `kimi_thinking` |
| `dispatch_minimax` | `minimax` |
| `dispatch_opus` | `opus` |
| `dispatch_haiku` | `haiku` |
| `dispatch_gemini` | `gemini` |

`dispatch_custom(tier, message)` accepts any tier from `VALID_TIERS`. Messages are capped at **4096 bytes** (UTF-8).

### Configuration

| Variable | Required | Description |
|----------|----------|-------------|
| `OMNIROUTE_URL` | Yes (dispatch) | OmniRoute base URL (`http://` or `https://`) |
| `LOG_LEVEL` | No | `DEBUG`, `INFO`, `WARNING`, `ERROR`, or `CRITICAL` |

### Health module

`dispatch_mcp.health` provides `liveness()`, `readiness(check_omniroute=False)`, and `metrics()` for probes and Prometheus text exposition.

### Run

```bash
cd driver-mcp
pip install -r requirements.txt
export OMNIROUTE_URL=http://localhost:20128
python dispatch_server.py
```

### Test

```bash
cd driver-mcp && pip install -r requirements.txt && pytest -q
```
