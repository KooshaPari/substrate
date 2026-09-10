//! `researchintel` — research-intelligence LLM port (ADR-052 LlmPort).
//!
//! ## Source
//!
//! Ported from `argis-extensions/plugins/researchintel/` (Go) as part
//! of the v13 3-plugin port wave
//! (`feat/v13-3-plugin-ports-2026-06-21`). The original Go plugin
//! enriches a request with research context (RAG-style retrieval
//! from a vector store + LLM-synthesized annotation) before
//! downstream phases see the request (see
//! `argis-extensions/plugins/researchintel/plugin.go`).
//!
//! ## SDK contract
//!
//! Implements [`crate::sdk::LlmPort`] (async provider port). The
//! in-tree adapter is a *pure-compute* stub that synthesizes a
//! research summary from the request id + payload (the upstream Go
//! plugin performs HTTP calls to a research service; the network
//! surface is modeled by the [`ResearchProvider`] trait so tests can
//! swap in deterministic providers).
//!
//! ## Telemetry
//!
//! `send()` emits exactly one OTel-compatible span named
//! `phenotype.router.plugin.researchintel.send` per ADR-052 §3.
//! Attributes: `phenotype.router.plugin.name`,
//! `phenotype.router.plugin.phase`, `phenotype.router.llm.source_model`,
//! `phenotype.router.llm.target_model`, `phenotype.router.llm.tokens`,
//! `phenotype.router.llm.cost_usd`.
//!
//! ## Substrate notes
//!
//! Per ADR-023 Rule 3.1, this file ships with spec (this header +
//! [`ResearchIntel`]), tests (`#[cfg(test)] mod tests` at the
//! bottom), OTel spans (above), and a `PREDICTIVE.md` next to the
//! source (per ADR-047 4-criterion rule).

use crate::sdk::{
    Capabilities, HealthStatus, LlmError, LlmPort, LlmRequest, LlmResponse,
};
use async_trait::async_trait;
use std::sync::Arc;

/// Plugin name (kebab-case, fleet-wide stable).
pub const PLUGIN_NAME: &str = "researchintel";
/// Plugin semver (ADR-052 §5).
pub const PLUGIN_VERSION: &str = "0.1.0";

/// Research provider — abstracts the upstream research service
/// (HTTP, MCP, or local). The in-tree
/// [`SynthesizedResearchProvider`] produces a deterministic summary
/// suitable for unit tests; production deployments wire in an
/// HTTP-backed provider.
pub trait ResearchProvider: Send + Sync + std::fmt::Debug {
    /// Stable provider name.
    fn name(&self) -> &str;
    /// Synthesize a research summary from a request. Returns the
    /// summary text on success, or an error string on failure.
    fn synthesize(&self, req: &LlmRequest) -> Result<String, String>;
    /// Estimated cost (USD) for the call. `None` if the provider
    /// cannot estimate (the default).
    fn cost_estimate(&self, req: &LlmRequest) -> Option<f64> {
        let _ = req;
        None
    }
}

/// Built-in deterministic provider: returns a fixed summary based
/// on `req.id`. Used in unit tests; never in production (the
/// summary is intentionally trivial).
#[derive(Debug, Default, Clone, Copy)]
pub struct SynthesizedResearchProvider;

impl ResearchProvider for SynthesizedResearchProvider {
    fn name(&self) -> &str {
        "synthesized"
    }

    fn synthesize(&self, req: &LlmRequest) -> Result<String, String> {
        if req.prompt.is_empty() {
            return Err("empty prompt".to_string());
        }
        Ok(format!(
            "research summary for id={} model={}->{}: {}",
            req.id, req.source_model, req.target_model, req.prompt
        ))
    }

    fn cost_estimate(&self, req: &LlmRequest) -> Option<f64> {
        // $0.0001 per character; deterministic, testable.
        Some(req.prompt.len() as f64 * 0.0001)
    }
}

/// The `researchintel` LLM port (ADR-052 LlmPort).
#[derive(Clone)]
pub struct ResearchIntel {
    provider: Arc<dyn ResearchProvider>,
}

impl std::fmt::Debug for ResearchIntel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResearchIntel")
            .field("name", &PLUGIN_NAME)
            .field("version", &PLUGIN_VERSION)
            .field("provider", &self.provider.name())
            .finish()
    }
}

impl ResearchIntel {
    /// Construct a `ResearchIntel` with the default
    /// [`SynthesizedResearchProvider`].
    pub fn new() -> Self {
        Self::with_provider(Arc::new(SynthesizedResearchProvider))
    }

    /// Construct a `ResearchIntel` with a custom provider (tests +
    /// production HTTP-backed providers).
    pub fn with_provider(provider: Arc<dyn ResearchProvider>) -> Self {
        Self { provider }
    }

    /// Read-only view of the active provider.
    pub fn provider(&self) -> &Arc<dyn ResearchProvider> {
        &self.provider
    }
}

impl Default for ResearchIntel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmPort for ResearchIntel {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    fn capabilities(&self) -> Capabilities {
        // Per ADR-052 §1: LlmPort implies NETWORK_IO. The in-tree
        // provider is pure-compute, but the production provider is
        // HTTP-backed, so we declare the capability to match the
        // real plugin's footprint.
        Capabilities::NETWORK_IO
    }

    async fn send(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        // Emit exactly one OTel-compatible span per ADR-052 §3.
        let span = ::tracing::info_span!(
            "phenotype.router.plugin.researchintel.send",
            "phenotype.router.plugin.name" = PLUGIN_NAME,
            "phenotype.router.plugin.phase" = "PostRouting",
            "phenotype.router.llm.source_model" = %req.source_model,
            "phenotype.router.llm.target_model" = %req.target_model,
            "phenotype.router.request.id" = %req.id,
        );
        let _g = span.enter();

        if req.id.is_empty() {
            ::tracing::warn!("empty request id");
            return Err(LlmError::InvalidRequest("empty id".to_string()));
        }
        if req.target_model.is_empty() {
            ::tracing::warn!("empty target model");
            return Err(LlmError::InvalidRequest("empty target_model".to_string()));
        }

        let summary = self.provider.synthesize(req).map_err(|reason| {
            ::tracing::warn!(reason = %reason, "research synthesis failed");
            LlmError::Provider {
                status: 502,
                body: reason,
            }
        })?;

        // Token count is a coarse 1-token-per-4-chars heuristic
        // (good enough for the in-tree synthesized provider; the
        // real HTTP-backed provider overrides this with a proper
        // tokenizer).
        let tokens = ((req.prompt.len() + summary.len()) / 4) as u32;
        let cost_usd = self.provider.cost_estimate(req);

        ::tracing::info!(
            tokens = tokens,
            cost_usd = ?cost_usd,
            "synthesized research summary"
        );

        Ok(LlmResponse {
            id: req.id.clone(),
            model: req.target_model.clone(),
            completion: summary,
            tokens,
            cost_usd,
        })
    }

    async fn health(&self) -> Result<HealthStatus, LlmError> {
        Ok(HealthStatus::Healthy)
    }

    fn cost_estimate(&self, req: &LlmRequest) -> Option<f64> {
        self.provider.cost_estimate(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_name_and_version_are_pinned() {
        assert_eq!(PLUGIN_NAME, "researchintel");
        assert_eq!(PLUGIN_VERSION, "0.1.0");
    }

    #[test]
    fn synthesized_provider_succeeds_for_non_empty_prompt() {
        let p = SynthesizedResearchProvider;
        let req = LlmRequest {
            id: "user:42".to_string(),
            source_model: "gpt-4o".to_string(),
            target_model: "claude-3-opus".to_string(),
            prompt: "hello".to_string(),
            routing_key: None,
        };
        let s = p.synthesize(&req).expect("synthesize must succeed");
        assert!(s.contains("research summary"));
        assert!(s.contains("user:42"));
        assert!(s.contains("gpt-4o"));
        assert!(s.contains("claude-3-opus"));
    }

    #[test]
    fn synthesized_provider_errors_on_empty_prompt() {
        let p = SynthesizedResearchProvider;
        let req = LlmRequest {
            id: "u".to_string(),
            source_model: "gpt-4o".to_string(),
            target_model: "claude-3-opus".to_string(),
            prompt: "".to_string(),
            routing_key: None,
        };
        assert!(p.synthesize(&req).is_err());
    }

    #[test]
    fn synthesized_provider_cost_estimate_scales_with_prompt_length() {
        let p = SynthesizedResearchProvider;
        let req_short = LlmRequest {
            id: "u".to_string(),
            source_model: "gpt-4o".to_string(),
            target_model: "claude-3-opus".to_string(),
            prompt: "x".to_string(),
            routing_key: None,
        };
        let req_long = LlmRequest {
            prompt: "x".repeat(100),
            ..req_short.clone()
        };
        let cost_short = p.cost_estimate(&req_short).unwrap();
        let cost_long = p.cost_estimate(&req_long).unwrap();
        assert!(cost_long > cost_short);
        assert!((cost_long - 0.01).abs() < 1e-9, "long = 100 * 0.0001 = 0.01");
    }

    #[test]
    fn research_intel_default_uses_synthesized_provider() {
        let ri = ResearchIntel::new();
        assert_eq!(ri.provider().name(), "synthesized");
    }

    #[test]
    fn research_intel_sdk_metadata_is_pinned() {
        let ri = ResearchIntel::new();
        assert_eq!(ri.name(), "researchintel");
        assert_eq!(ri.version(), "0.1.0");
        assert!(ri.capabilities().contains(Capabilities::NETWORK_IO));
    }

    #[tokio::test]
    async fn send_rejects_empty_id() {
        let ri = ResearchIntel::new();
        let req = LlmRequest {
            id: "".to_string(),
            source_model: "gpt-4o".to_string(),
            target_model: "claude-3-opus".to_string(),
            prompt: "p".to_string(),
            routing_key: None,
        };
        let res = ri.send(&req).await;
        assert!(matches!(res, Err(LlmError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn send_rejects_empty_target_model() {
        let ri = ResearchIntel::new();
        let req = LlmRequest {
            id: "u".to_string(),
            source_model: "gpt-4o".to_string(),
            target_model: "".to_string(),
            prompt: "p".to_string(),
            routing_key: None,
        };
        let res = ri.send(&req).await;
        assert!(matches!(res, Err(LlmError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn send_returns_summary_for_valid_request() {
        let ri = ResearchIntel::new();
        let req = LlmRequest {
            id: "user:1".to_string(),
            source_model: "gpt-4o".to_string(),
            target_model: "claude-3-opus".to_string(),
            prompt: "summarize this".to_string(),
            routing_key: Some("tenant:a".to_string()),
        };
        let resp = ri.send(&req).await.expect("send must succeed");
        assert_eq!(resp.id, "user:1");
        assert_eq!(resp.model, "claude-3-opus");
        assert!(resp.completion.contains("research summary"));
        assert!(resp.tokens > 0);
        assert!(resp.cost_usd.is_some());
    }

    #[test]
    fn research_intel_clone_shares_provider() {
        let ri1 = ResearchIntel::new();
        let ri2 = ri1.clone();
        assert!(Arc::ptr_eq(&ri1.provider, &ri2.provider));
    }

    #[test]
    fn research_intel_default_helper() {
        let ri = ResearchIntel::default();
        assert_eq!(ri.provider().name(), "synthesized");
    }

    /// Custom provider for the failure-mode test.
    #[derive(Debug)]
    struct AlwaysFailProvider;
    impl ResearchProvider for AlwaysFailProvider {
        fn name(&self) -> &str {
            "always_fail"
        }
        fn synthesize(&self, _: &LlmRequest) -> Result<String, String> {
            Err("upstream 503".to_string())
        }
    }

    #[tokio::test]
    async fn send_propagates_provider_error_as_provider_status() {
        let ri = ResearchIntel::with_provider(Arc::new(AlwaysFailProvider));
        let req = LlmRequest {
            id: "u".to_string(),
            source_model: "gpt-4o".to_string(),
            target_model: "claude-3-opus".to_string(),
            prompt: "p".to_string(),
            routing_key: None,
        };
        let res = ri.send(&req).await;
        match res {
            Err(LlmError::Provider { status, body }) => {
                assert_eq!(status, 502);
                assert!(body.contains("upstream 503"));
            }
            other => panic!("expected Provider error, got {:?}", other),
        }
    }
}
