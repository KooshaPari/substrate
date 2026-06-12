# SSOT — thegent-dispatch

## State
- Default branch: main
- Last verified: 2026-06-08
- CI status: green
- Open PRs: 0
- Open branches: 1 (main)
- Stashes: 0

## Architecture
- Hexagonal: yes
- Ports: ProviderPort, DispatchPort
- Adapters: CLI adapter (Rust), MCP adapter (Python)
- Domain: Argv builder, provider routing, tier dispatch

## Merges
- dispatch-mcp -> python/dispatch-mcp/ (2026-06-08)

## Next Steps
1. [ ] Wire Python MCP adapter into Rust CLI
2. [ ] Add unified dispatch registry
3. [ ] Add cross-language tests
