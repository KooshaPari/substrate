# cloud-codex

[`CloudDispatchPort`] adapter that drives [Codex Cloud](https://developers.openai.com/codex/cli/reference#codex-cloud) via the `codex` CLI (`codex cloud exec`, `status`, `diff`, `apply`).

## Auth

Codex Cloud commands use the Codex CLI login session. Run `codex login` before dispatching tasks. No separate cloud API key env var is required.

## Env

| Variable | Purpose |
|----------|---------|
| `CODEX_BIN` | Path to the `codex` binary (default `codex` on `PATH`) |
| `CODEX_CLOUD_ENV_ID` | Target environment id for `codex cloud exec --env` (required) |

The repository URL passed to [`CloudDispatchPort::submit_task`] is informational; the linked repo/ref comes from the Codex Cloud environment configuration.

## Usage

```rust
use cloud_codex::CodexCloudDispatch;
use substrate_core::cloud_dispatch_port::CloudDispatchPort;

let adapter = CodexCloudDispatch::from_env()?;
let handle = adapter.submit_task(
    "https://github.com/org/repo",
    "main",
    "Add README",
).await?;
```

## CLI surface

| Operation | Command |
|-----------|---------|
| Submit | `codex cloud exec --env <ENV_ID> "<prompt>" --branch <branch>` |
| Poll | `codex cloud status <task_id>` |
| Harvest diff | `codex cloud diff <task_id>` |

[`CloudDispatchPort`]: https://docs.rs/substrate-core/latest/substrate_core/cloud_dispatch_port/trait.CloudDispatchPort.html
