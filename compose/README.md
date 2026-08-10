# Proc-Compose

This directory contains JSON compose manifests for substrate daemons managed
by the proc-compose supervisor (built into the substrate TUI dashboard).

## Convention

Each `.json` file describes one daemon process. The file name should match the
daemon name (e.g. `substrate-gateway.json`).

| Field         | Type             | Description                                |
|---------------|------------------|--------------------------------------------|
| `name`        | string           | Human-readable service name                |
| `binary`      | optional string  | Path to the compiled binary                |
| `run_command` | optional string  | Shell command to start the service         |
| `health_check`| optional string  | Shell command that exits 0 when healthy    |
| `port`        | optional integer | Primary listen port (null if Unix-socket)  |
| `restart`     | optional string  | Restart policy (always, on-failure, no)    |
| `depends_on`  | string[]         | Other services that must start first       |

The TUI reads this directory at startup and on refresh to populate the
Proc-Compose section of the dashboard.

## Daemons

| Manifest                 | Crate / Source                        | Port  |
|--------------------------|---------------------------------------|-------|
| `substrate-gateway.json` | `crates/gateway` (Rust, axum)        | 8010  |
| `forge-daemon.json`      | `forge-daemon/` (Zig, kqueue)        | —     |
