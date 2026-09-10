//! Tracing façade (ADR-012 / ADR-036B / ADR-037).
//!
//! `phenotype-router` re-exports the `tracing` crate so plugin
//! authors can use the standard `tracing::{info!, warn!, error!, debug!,
//! trace!, span!, instrument}` macros without adding a separate
//! `tracing` dependency in their `Cargo.toml`.
//!
//! Per ADR-012 + ADR-036B, the canonical tracing substrate for the
//! fleet is `pheno-tracing` (the dedicated substrate crate). The
//! `phenotype-router` re-export here is the *macro* façade only; the
//! *substrate* OTLP adapter still lives in `pheno-tracing` and is wired
//! in by the consumer's `main.rs`. This crate does not re-implement
//! the substrate; that would create a circular dep per ADR-023
//! Rule 3 (no random `phenoShared`/re-exports that duplicate substrate
//! functionality).
//!
//! ## Span contract
//!
//! Plugin code is encouraged to emit spans in addition to the
//! decision-layer OTel span produced by the `OtlpDecisionRecorder`
//! (see [`crate::otel`]). The convention is:
//!
//! - `name = "phenotype.router.plugin.<plugin_name>.<method>"`
//! - attributes: `phenotype.router.plugin.name`, `phenotype.router.plugin.phase`
//!
//! The consumer is free to attach additional attributes; the substrate
//! does not enforce a fixed schema beyond the OTel attribute namespace
//! `phenotype.router.*` (which ADR-012 reserves for fleet-wide use).

pub use ::tracing;

/// Init the global tracing subscriber. Convenience for the consumer's
/// `main.rs`; does not need to be called in test crates (they typically
/// use a `tracing-test` or `tracing_subscriber::fmt::TestExt`).
///
/// No-op if a global subscriber is already installed (the typical
/// behavior of `tracing::subscriber::set_global_default`).
pub fn try_init() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("phenotype_router=info"));
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .finish(),
    );
}

/// Emit a structured plugin event at `info` level.
///
/// Adapter authors should prefer this over `tracing::info!` so the
/// `phenotype.router.plugin.*` attribute namespace stays consistent
/// across all 9 plugins.
#[macro_export]
macro_rules! plugin_event {
    ($plugin:expr, $phase:expr, $($arg:tt)+) => {{
        ::tracing::info!(
            plugin.name = %$plugin,
            plugin.phase = %$phase,
            $($arg)+
        );
    }};
}

/// OTel decision-span name constant (ADR-052 §3).
///
/// Kept in sync with the recorder's default `OtelConfig::span_name`. Exposed
/// here so adapter authors and downstream consumers can reference the same
/// canonical span name in their own tracing/OTLP configurations.
pub const DECISION_SPAN_NAME: &str = "phenotype.router.decision";

/// Type alias for a `tracing::Span` whose name is the OTel decision span
/// convention. Use [`DECISION_SPAN_NAME`] for the canonical string when
/// emitting via the OTLP layer directly.
pub type DecisionSpan = ::tracing::Span;
