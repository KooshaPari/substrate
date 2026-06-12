# dispatch-mcp

MCP server for tier-based dispatch delegation via OmniRoute.

## Tools

### Per-tier dispatch tools

| Tool name | Tier |
|---|---|
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

Each accepts a single `message: str` argument and dispatches it to the configured OmniRoute backend under the corresponding tier.

### Custom dispatch

`dispatch_custom(tier: str, message: str)` — dispatch to any tier from `VALID_TIERS` above.

### Health

- `dispatch_health()` — probe the OmniRoute backend health endpoint. Requires `OMNIROUTE_URL` to be set.
- `dispatch_liveness()` — returns server liveness status without contacting OmniRoute.

## Configuration

| Variable | Required | Default | Description |
|---|---|---|---|
| `OMNIROUTE_URL` | Yes | — | Base URL of the OmniRoute dispatch backend (e.g. `http://localhost:8080`). Must use `http://` or `https://` scheme. |
| `LOG_LEVEL` | No | (root logger) | Logging verbosity. Accepted values: `DEBUG`, `INFO`, `WARNING`, `ERROR`, `CRITICAL`. Invalid values fall through to the root logger's level. |

### Constraints

- `message` must not exceed **4096 bytes** (UTF-8 encoded).
- `tier` must be one of the known tiers listed above.
- `OMNIROUTE_URL` must use `http://` or `https://` scheme. Other schemes (e.g. `file://`, `javascript:`) are rejected at startup with a `ValueError`.
- HTTP redirects are **not followed** — only direct requests to `OMNIROUTE_URL` are made.

## Build

Install the package in editable mode with the `dev` extras (pulls in
`pytest`, `pytest-cov`):

```bash
python -m pip install -e ".[dev]"
```

The runtime dependencies are `fastmcp>=3.2.4` and `httpx>=0.27.0`.
Requires Python 3.13+.

## Test

```bash
# Run the full test suite with coverage (uses pyproject.toml addopts)
pytest

# Or with an explicit report
pytest --cov-report=term-missing
```

The default `addopts` (see `[tool.pytest.ini_options]`) enables branch
coverage, missing-line reporting, and fails the run if coverage drops
below 80%.

### Lint & type check

```bash
# Format
ruff format .

# Lint
ruff check .

# Strict type check
mypy src/ --strict
```

The same checks run in CI (see `.github/workflows/ci.yml`).

## Run

```bash
# Set the backend URL
export OMNIROUTE_URL=http://localhost:20128

# Via entry point
dispatch-mcp

# Or directly
python -m dispatch_mcp.server
```
