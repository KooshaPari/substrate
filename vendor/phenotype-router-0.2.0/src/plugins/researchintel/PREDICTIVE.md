# PREDICTIVE.md — `researchintel` plugin (ADR-047 4-criterion rule)

> Predictive DRY check for the `researchintel` plugin's promotion to
> Tier-1 substrate status. Per ADR-047, a 4-criterion rule gates the
> promotion: every box must be checked off before the plugin is
> treated as canonical across the fleet.

## 1. Is the capability already provided by an existing crate/substrate?

**Checked.** No crate in the fleet provides a research-intelligence
LLM port with the Bifrost-compatible interface (async `send` /
`health` / `cost_estimate`, NETWORK_IO capability, OTel span
contract). Closest analogues:

- `pheno-mcp-router` (ADR-013) — generic LLM router, no research.
- `phenotype-router`'s own SDK traits — interfaces only; no
  implementations outside the in-tree ports.
- `pheno-otel` — observability, not LLM synthesis.

`researchintel` is the canonical home for *research-intelligence
capability* in the fleet. No duplication.

## 2. Will the capability be reused by 3+ downstream consumers within 12 months?

**Checked.** Three confirmed consumers on the v13-v15 roadmap:

- `phenotype-router` itself (Phase 5 of the decision pipeline; the
  research summary enriches the request before downstream phases).
- `phenotype-go-sdk` Go consumers (research-intensive tools
  already on the v14 backlog).
- `phenoAI` flagship app (the user-facing research flow; design
  partner ticket open).

One additional confirmed consumer (`pheno-prompt-test` regression
harness) plus one in-flight design partner (`phenotype-journeys`
research-heavy workflows). Threshold met.

## 3. Is the interface stable enough that duplicating it elsewhere would cause divergence?

**Checked.** The interface is the ADR-052 `LlmPort` trait
(`name`, `version`, `capabilities`, `send`, `health`,
`cost_estimate`), which is already in `phenotype-router` v0.2.0 and
consumed by the Bifrost plugin registry. The plugin's
`ResearchProvider` trait is an internal implementation detail; the
public surface is the `LlmPort` impl only. Divergence risk is low.

## 4. Will the cost of creating + maintaining a separate crate exceed the cost of inlining it?

**Checked.** `researchintel` is ~380 LoC with ~13 unit tests. As a
separate crate it would need its own `Cargo.toml`, CI workflow,
changelog, and release cadence. The marginal cost of inlining (a
`pub mod` + the `#[cfg(test)]` block) is essentially zero. Inlining
wins; promotion to Tier-1 substrate is *not* recommended.

## Decision

**Status: INLINE — do NOT promote to Tier-1 substrate crate.**

Rationale: criterion 1 (no duplication), criterion 2 (3+ consumers
in 12mo), and criterion 3 (stable interface) are all satisfied, but
criterion 4 (cost of separate crate exceeds inlining cost) is not.
The plugin ships as `phenotype_router::plugins::researchintel` in
v0.2.0; promotion to a standalone crate is deferred until the
plugin footprint grows past ~1,000 LoC or until 2+ non-router
consumers require a direct git dep.

## Changelog

- 2026-06-21 — Initial PREDICTIVE.md (v13 3-plugin port wave, branch
  `feat/v13-3-plugin-ports-2026-06-21`).
