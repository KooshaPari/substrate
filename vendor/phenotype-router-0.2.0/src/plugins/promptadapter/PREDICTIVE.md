# PREDICTIVE.md — `promptadapter` plugin (ADR-047 4-criterion rule)

> Predictive DRY check for the `promptadapter` plugin's promotion to
> Tier-1 substrate status. Per ADR-047, a 4-criterion rule gates the
> promotion: every box must be checked off before the plugin is
> treated as canonical across the fleet.

## 1. Is the capability already provided by an existing crate/substrate?

**Checked.** No crate in the fleet provides a prompt-transformation
plugin with the Bifrost-compatible interface (model-to-model rewrite,
transform registry, safe passthrough). Closest analogues:

- `pheno-context` (ADR-035) — windowed context, not per-request transform.
- `pheno-port-adapter` (ADR-038) — generic L4 port, no domain logic.
- `phenotype-router` itself — only owns the decision layer.

`promptadapter` is the canonical home for *prompt-rewriting
capability* in the fleet. No duplication.

## 2. Will the capability be reused by 3+ downstream consumers within 12 months?

**Checked.** Three confirmed consumers on the v13 roadmap:

- `phenotype-router` itself (Phase 2 of the decision pipeline).
- `phenotype-go-sdk` — Go consumers calling the plugin via the
  cross-language `phenotype-router` SDK.
- `phenotype-python-sdk` — Python consumers (langchain + llama-index
  adapters already have integration tickets open).

Two additional in-flight design partners (`pheno-otel` dashboard +
`OmniRoute` cluster profiles) have signalled intent to adopt. The
3-consumer threshold is met today.

## 3. Is the interface stable enough that duplicating it elsewhere would cause divergence?

**Checked.** The interface is the ADR-052 `DecisionPlugin` trait
(`name`, `version`, `phase`, `capabilities`, `apply`), which is
already in `phenotype-router` v0.2.0 and consumed by the Bifrost
plugin registry. The plugin's `Transform` + `TransformRegistry` types
are an internal implementation detail; the public surface is the
`DecisionPlugin` impl only. Divergence risk is low.

## 4. Will the cost of creating + maintaining a separate crate exceed the cost of inlining it?

**Checked.** `promptadapter` is ~450 LoC with ~12 unit tests. As a
separate crate it would need its own `Cargo.toml`, CI workflow,
changelog, and release cadence. The marginal cost of inlining (a
`pub mod` + the `#[cfg(test)]` block) is essentially zero. Inlining
wins; promotion to Tier-1 substrate is *not* recommended (the plugin
is shipped as part of `phenotype-router` v0.2.0, not as a standalone
crate).

## Decision

**Status: INLINE — do NOT promote to Tier-1 substrate crate.**

Rationale: criterion 1 (no duplication), criterion 2 (3+ consumers
in 12mo), and criterion 3 (stable interface) are all satisfied, but
criterion 4 (cost of separate crate exceeds inlining cost) is not.
The plugin ships as `phenotype_router::plugins::promptadapter` in
v0.2.0; promotion to a standalone crate is deferred until the
plugin footprint grows past ~1,000 LoC or until 2+ non-router
consumers require a direct git dep.

## Changelog

- 2026-06-21 — Initial PREDICTIVE.md (v13 3-plugin port wave, branch
  `feat/v13-3-plugin-ports-2026-06-21`).
