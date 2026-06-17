# driver-mcp sync with PhenoMCPServers

Canonical MCP server source: `KooshaPari/PhenoMCPServers` → `servers/substrate/`.

This directory is a **dev convenience mirror**. Before release, run:

```bash
python scripts/check_driver_mcp_sync.py
```

If drift is reported, copy from PhenoMCPServers:

```bash
# from substrate repo root, with PhenoMCPServers checked out alongside
rsync -a --delete ../PhenoMCPServers/servers/substrate/ driver-mcp/ \
  --exclude tests --exclude pyproject.toml --exclude README.md
```

Long-term (ADR-019): substrate will depend on the PhenoMCPServers package instead of duplicating this tree.
