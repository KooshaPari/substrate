# PREDICTIVE.md — `contextfolding` plugin (ADR-047 4-criterion rule)

> Predictive DRY check for the `contextfolding` plugin's promotion
> to Tier-1 substrate status. Per ADR-047, a 4-criterion rule gates
> the promotion: every box must be checked off before the plugin is
> treated as canonical across the fleet.

## 1. Is the capability already provided by an existing crate/substrate?

**Checked.** No crate in the fleet provides a context-folding
connector with the Bifrost-compatible interface (open a session,
fold payloads, expose a `ping` health probe, NETWORK_IO capability).
Closest analogues:

- `pheno-context` (ADR-035) — windowed context, not fold/dedup.
- `pheno-port-adapter` (ADR-038) — generic L4 port, no domain logic.
- `phenotype-router`'s own `BifrostAdapter` — decision layer only;
  no folding.

`contextfolding` is the canonical home for *payload-folding
capability* in the fleet. No duplication.

## 2. Will the capability be reused by 3+ downstream consumers within 12 months?

**Checked.** Three confirmed consumers on the v13-v14 roadmap:

- `phenotype-router` itself (Phase 2 of the decision pipeline; the
  folding handle is opened during request setup).
- `phenotype-python-sdk` langchain adapter (Q3-2026 ticket open).
- `pheno-otel` cost-card dashboard (Q3-2026 ticket open; folding
  needed to fit large request payloads in the dashboard table).

One additional confirmed consumer (`pheno-prompt-test` regression
harness) plus one in-flight design partner (`phenotype-journeys`
long-running workflow DSL). Threshold met.

## 3. Is the interface stable enough that duplicating it elsewhere would cause divergence?

**Checked.** The interface is the ADR-052 `ConnectorPort` trait
(`name`, `version`, `capabilities`, `connect`, `health`) plus the
`ConnectorHandle` trait (`name`, `ping`). Both are in
`phenotype-router` v0.2.0 and consumed by the Bifrost plugin
registry. The plugin's `FoldingStrategy` trait is an internal
implementation detail; the public surface is the `ConnectorPort`
impl only. Divergence risk is low.

## 4. Will the cost of creating + maintaining a separate crate exceed the cost of inlining it?

**Checked.** `contextfolding` is ~430 LoC with ~13 unit tests. As a
separate crate it would need its own `Cargo.toml`, CI workflow,
changelog, and release cadence. The marginal cost of inlining (a
`pub mod` + the `#[cfg(test)]` block) is essentially zero. Inlining
wins; promotion to Tier-1 substrate is *not* recommended.

## Decision

**Status: INLINE — do NOT promote to Tier-1 substrate crate.**

Rationale: criterion 1 (no duplication), criterion 2 (3+ consumers
in 12mo), and criterion 3 (stable interface) are all satisfied, but
criterion 4 (cost of separate crate exceeds inlining cost) is not.
The plugin ships as `phenotype_router::plugins::contextfolding` in
v0.2.0; promotion to a standalone crate is deferred until the
plugin footprint grows past ~1,000 LoC or until 2+ non-router
consumers require a direct git dep.

## Changelog

- 2026-06-21 — Initial PREDICTIVE.md (v13 3-plugin port wave, branch
  `feat/v13-3-plugin-ports-2026-06-21`).
