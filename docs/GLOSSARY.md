# Glossary

| Term | Meaning |
| --- | --- |
| **dispatch** | The act of accepting a task and invoking a selected engine. |
| **driver** | An adapter that translates substrate's dispatch contract to an external or local runtime. |
| **engine** | A model, CLI, or worker capable of executing a task. |
| **gateway** | The HTTP/MCP edge that authenticates requests, applies limits, and exposes health and metrics. |
| **lease** | A time-bounded claim that prevents two workers from processing the same task concurrently. |
| **port** | A domain-level trait that defines an integration boundary without naming an implementation. |
| **adapter** | An implementation of a port for a concrete service, protocol, or storage backend. |
| **routing policy** | The rules that select an engine or fallback tier for a task. |
| **tier** | A routing class ordered from inexpensive/fast workers to stronger or more capable engines. |
| **structured result** | The stable result envelope containing status, output, errors, and provenance metadata. |
| **SLO / SLI** | A service-level objective and the indicator used to measure it; see `docs/SLO.md`. |
| **worktree** | An independent checkout of a Git branch used for isolated development or evaluation. |

For the full request and event contracts, see `SPEC.md`, `ARCHITECTURE.md`, and
the generated API reference in `docs/openapi.yaml`.
