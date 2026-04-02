# sharecli

Shared CLI process manager for multi-project agent orchestration.

## Purpose

Centralized process management for Phenotype's multi-project agent infrastructure:
- Single control plane for all agent processes across repos
- Resource pooling to reduce memory overhead
- Unified monitoring and health checks
- Graceful lifecycle management
- Process-compose integration

## Quick Start

```bash
# Initialize configuration
sharecli config init

# Add a project
sharecli project add helios-cli ~/CodeProjects/Phenotype/repos/helios-cli

# Discover all projects in a directory
sharecli project discover ~/CodeProjects/Phenotype/repos

# List registered projects
sharecli project list

# List managed processes
sharecli ps

# Status with resource summary
sharecli status

# Start a harness process
sharecli start helios-cli --harness claude

# Stop by project
sharecli stop --project helios-cli

# Stop all processes
sharecli stop --all

# Generate process-compose.yml for registered projects
sharecli project generate

# Run with pooled runtime
sharecli run node --project my-project

# Set project resource limits
sharecli limits set helios-cli --memory 4096 --max-procs 10

# Check project limits
sharecli check helios-cli

# Optimize - analyze and suggest improvements
sharecli optimize

# Prune idle processes (dry-run by default)
sharecli prune --idle 30m --dry-run
```

## Commands

| Command | Description |
|---------|-------------|
| `sharecli ps` | List managed processes with filtering |
| `sharecli start` | Start harness process for a project |
| `sharecli stop` | Stop processes by PID, project, or harness |
| `sharecli status` | Health check with resource summary |
| `sharecli config` | Config init, validate, show, get, set |
| `sharecli project` | Add, remove, list, show, discover, generate projects |
| `sharecli run` | Run with pooled runtime (node/bun) |
| `sharecli pool` | Show shared runtime pool status |
| `sharecli health` | Probe shared runtime health (supports `--harness` hints) |
| `sharecli limits` | Set/get project resource limits |
| `sharecli check` | Check project resource limits |
| `sharecli optimize` | Analyze and suggest resource optimizations |
| `sharecli prune` | Kill idle processes |

## Architecture

```
sharecli/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Library exports
│   ├── config.rs        # TOML config + CLI command enums
│   ├── runtime.rs       # ProcessPool + SharedRuntime
│   ├── monitoring.rs    # HealthStatus, ProcessStats
│   └── commands/
│       └── mod.rs       # All CLI command implementations
├── config/
│   ├── sharecli.toml.example
│   └── process-compose/
│       └── template.yml
└── Cargo.toml
```

## Configuration

Configuration is stored in `~/.config/sharecli/config.toml`:

```toml
[projects]
helios-cli = "~/CodeProjects/Phenotype/repos/helios-cli"
portage = "~/CodeProjects/Phenotype/repos/portage"

[runtime]
max_memory_mb = 4096
max_processes = 100
```

## Process-Compose Integration

Generate a `process-compose.yml` for your registered projects:

```bash
sharecli project generate
```

This creates services for each registered project with health probes and logging.

## Resource Limits

Set per-project resource limits to prevent runaway processes:

```bash
sharecli limits set my-project --memory 2048 --max-procs 5
sharecli check my-project
```

## Optimization

Analyze running processes and suggest optimizations:

```bash
sharecli optimize
```

## License

MIT
