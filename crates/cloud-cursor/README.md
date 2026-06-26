# cloud-cursor

[`CloudDispatchPort`] adapter that drives [Cursor Cloud Agents](https://cursor.com/docs/cloud-agent/api/endpoints) via the REST API (`POST /v1/agents`, poll `GET /v1/agents/{id}/runs/{runId}`).

## Auth

Set `CURSOR_API_KEY` (Bearer or Basic — the adapter sends Basic auth with the key as username).

## Usage

```rust
use cloud_cursor::CursorCloudDispatch;
use substrate_core::cloud_dispatch_port::CloudDispatchPort;

let adapter = CursorCloudDispatch::from_env()?;
let handle = adapter.submit_task(
    "https://github.com/org/repo",
    "main",
    "Add README",
).await?;
```

Alternatively, the Cursor CLI (`cursor-agent.ps1` / `agent.ps1`) can spawn cloud agents interactively; this crate uses the REST API for headless dispatch from substrate.

[`CloudDispatchPort`]: https://docs.rs/substrate-core/latest/substrate_core/cloud_dispatch_port/trait.CloudDispatchPort.html
