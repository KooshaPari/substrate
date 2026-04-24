# Changelog

All notable changes to this project will be documented in this file.

## ✨ Features
- Feat(thegent-dispatch): v0.1 — unified CLI dispatcher for coding-agent providers

- Argv builders for Forge, Codex, Gemini, Copilot, Cursor, Droid, Minimax, Claude
- Hard-rejects --model on --provider copilot (Haiku-locked)
- --dry-run preview + --emit json for machine consumption
- Wraps in `thegent bg --owner --format json --` when --session bg
- Standalone workspace (not part of repos/ parent)
- 6 integration tests, clippy-clean
- Registered as global `thegent` skill in ~/.claude/skills/

Complements cheap-llm-mcp for Minimax routing. Absorption is hybrid per design —
provider-specific skills (codex-agent, copilot-agent, etc.) stay for unique flags
(reasoning tri-state, Haiku-lock, etc.).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com> (`796d624`)
## 🔨 Other
- Chore(ci): adopt phenotype-tooling quality-gate + fr-coverage (`6e65b61`)