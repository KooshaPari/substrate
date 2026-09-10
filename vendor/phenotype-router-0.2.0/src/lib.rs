//! Phenotype router decision layer (ADR-050 / ADR-051, §8 router-architecture
//! ACCEPTED 2026-06-20).
//!
//! This crate owns the **decision layer** of the Phenotype router architecture.
//! It is a hexagonal L4 Port/Adapter primitive (ADR-038): the [`DecisionLayer`]
//! trait is the Port side, every concrete strategy (`BifrostAdapter`,
//! `HelloWorld`) is an Adapter. The decision layer calls into Bifrost
//! (transport, ADR-051); Bifrost never calls back. The decision layer is
//! where 9 plugins (`intelligentrouter`, `smartfallback`, `learning`,
//! `promptadapter`, `contextfolding`, `voyage`, `researchintel`,
//! `contentsafety`, `toolrouter`) plus the Q3-2026 `vector-store` slot are
//! loaded against the SDK defined in ADR-052.
//!
//! ## Substrate role
//!
//! Per ADR-023 Rule 3, `phenotype-router` is a `pheno-*-lib` — pure reusable
//! Rust library, single concern (decision layer), single crate. It is the
//! canonical substrate for **all** LLM-routing decisions in the Phenotype
//! fleet. The `otlp` feature (on by default) wires every decision into
//! `pheno-tracing` (ADR-012 / ADR-036) so spans flow to OTLP collectors
//! per ADR-052 §3.
//!
//! ## Quality bar
//!
//! This crate ships per ADR-023 Rule 3.1 + ADR-040 (60 % coverage gate for
//! federated-service tier) + ADR-042 (7-element substrate quality bar) +
//! ADR-038 (hexagonal L4 Port). See `SPEC.md`, `README.md`, and `WORKLOG.md`
//! at the repo root.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod decision;
mod hello_world;
mod bifrost_adapter;
pub mod sdk;
pub mod plugins;

// Observability substrate (ADR-012 / ADR-036B). Default-on.
#[cfg(feature = "otlp")]
pub mod otel;
#[cfg(feature = "otlp")]
pub mod tracing;

// Chaos matrix (L25 anti-fragility pillar). Gated on the `chaos` feature
// because it pulls in `rand` via the lib crate. CI runs the chaos test
// with `--features chaos`; normal `cargo test` (no features) skips it.
#[cfg(feature = "chaos")]
pub mod chaos;
#[cfg(feature = "chaos")]
pub use chaos as chaos_matrix;

pub use decision::{Decision, DecisionError, DecisionLayer, Request, Response};
pub use hello_world::{hello_response, HelloWorld, HelloWorldPort, HelloWorldResponse};
pub use bifrost_adapter::BifrostAdapter;
pub use sdk::{
    Capabilities, ConnectorConfig, ConnectorError, ConnectorHandle, ConnectorPort,
    DecisionPlugin, HealthStatus, LlmError, LlmPort, LlmRequest, LlmResponse,
    Phase, PluginDecision, PluginError,
};

#[cfg(feature = "otlp")]
pub use otel::{OtelConfig, OtlpDecisionRecorder};
#[cfg(feature = "otlp")]
pub use tracing as tracing_facade;
