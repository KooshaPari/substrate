# sharecli

Shared CLI process manager for multi-project agent orchestration.

## Purpose

Centralized process management for Phenotype's multi-project agent infrastructure:
- Single control plane for all agent processes across repos
- Resource pooling to reduce memory overhead
- Unified monitoring and health checks
- Graceful lifecycle management

## Quick Start

```bash
# Initialize configuration
sharecli config init

# Add a project
sharecli project add portage ~/CodeProjects/Phenotype/repos/portage

# List managed processes
sharecli ps

# Start a harness
sharecli start portage --harness=claude

# Stop all processes
sharecli stop --all

# Health check
sharecli status
```

## Architecture

```
sharecli/
├── bin/              # CLI entry point
├── src/
│   ├── commands/     # CLI subcommands
│   ├── runtime/      # Process spawning
│   ├── projects/     # Project registry
│   └── monitoring/   # Health/stats
└── config/
    └── sharecli.toml # Global config
```

## Configuration

See `config/sharecli.toml.example` for full configuration.

## License

MIT
