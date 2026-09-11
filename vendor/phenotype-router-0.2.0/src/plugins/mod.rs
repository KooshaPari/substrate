//! Plugin implementations ported from the Bifrost regression set.
//!
//! Per v11 plan §L3 (T3.4 / T3.5 / T3.8) and v13 scope (3 plugin ports).
//! Each plugin implements one of the three SDK traits ([`crate::sdk::LlmPort`],
//! [`crate::sdk::DecisionPlugin`], [`crate::sdk::ConnectorPort`]) per
//! ADR-052 and ships with:
//!
//! - unit tests in a `#[cfg(test)] mod tests` block at the bottom
//! - OTel-compatible `tracing` spans per ADR-012 / ADR-036B
//! - `PREDICTIVE.md` next to the source documenting the ADR-047 4-criterion
//!   predictive-DRY check for promotion to Tier-1 substrate status
//!
//! ## v13 3-plugin port wave (`feat/v13-3-plugin-ports-2026-06-21`)
//!
//! - `promptadapter` (ADR-052 DecisionPlugin, phase=RequestTransform) —
//!   rewrites prompts via an in-process transform registry; safe
//!   passthrough when no transform matches. See `promptadapter.rs` +
//!   `promptadapter/PREDICTIVE.md`.
//! - `contextfolding` (ADR-052 ConnectorPort) — folds long payloads
//!   into shorter ones via a pluggable `FoldingStrategy` (default:
//!   `WhitespaceDedupeStrategy`). See `contextfolding.rs` +
//!   `contextfolding/PREDICTIVE.md`.
//! - `researchintel` (ADR-052 LlmPort) — synthesizes a research
//!   summary via a pluggable `ResearchProvider` (default:
//!   `SynthesizedResearchProvider`). See `researchintel.rs` +
//!   `researchintel/PREDICTIVE.md`.

pub mod promptadapter;
pub mod contextfolding;
pub mod researchintel;
