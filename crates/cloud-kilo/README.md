# cloud-kilo

[`CloudDispatchPort`] adapter for Kilo.

## Discovery note

Kilo exposes an LLM gateway at `https://api.kilo.ai/api/gateway/v1` (chat completions only). There is **no** public REST endpoint at `api.kilo.ai` for programmatic Cloud Agent session creation — cloud agents are started via the web UI or [webhook triggers](https://kilo.ai/docs/code-with-ai/platforms/cloud-agent#triggers).

This crate therefore implements **model-backed dispatch**:

1. Call the Kilo gateway (`minimax/minimax-m3`) with the task prompt.
2. Clone the target repo locally, create a branch, write a dispatch artifact from the model response.
3. Commit, push, and open a PR via `git` + `gh` when credentials are available.

Auth: `KILO_API_KEY` (Bearer JWT).

## Env

| Variable | Purpose |
|----------|---------|
| `KILO_API_KEY` | Gateway Bearer JWT (required) |
| `KILO_GATEWAY_URL` | Override gateway base (default `https://api.kilo.ai/api/gateway/v1`) |
| `KILO_MODEL` | Model id (default `minimax/minimax-m3`) |

[`CloudDispatchPort`]: https://docs.rs/substrate-core/latest/substrate_core/cloud_dispatch_port/trait.CloudDispatchPort.html
