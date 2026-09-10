//! Plugin chain integration test (ADR-052 §5).
//!
//! Exercises the 3 v13-ported plugins (`promptadapter`,
//! `contextfolding`, `researchintel`) against the same end-to-end
//! decision-layer request flow. The test asserts that the 3 plugins
//! compose cleanly (no interface drift, no async/sync mismatch, no
//! span-name collision) and that the chain's output shape matches
//! the documented ADR-052 §5 contract.

use phenotype_router::plugins::contextfolding::ContextFoldingConnector;
use phenotype_router::plugins::promptadapter::{
    IdentityTransform, PromptAdapter, TransformRegistry,
};
use phenotype_router::plugins::researchintel::{
    ResearchIntel, ResearchProvider, SynthesizedResearchProvider,
};
use phenotype_router::sdk::{
    Capabilities, ConnectorConfig, ConnectorHandle, ConnectorPort, DecisionPlugin, LlmPort,
    LlmRequest, Phase,
};
use phenotype_router::{Decision, Request, Response};
use std::sync::Arc;

#[test]
fn promptadapter_passthrough_for_unmatched_id() {
    let mut reg = TransformRegistry::new();
    reg.register(Arc::new(IdentityTransform));
    let pa = PromptAdapter::new(reg);
    let d = pa
        .apply(&Request::new("user:42", "hello world"))
        .expect("apply must succeed");
    assert!(matches!(d.decision, Decision::Allow));
    assert!(d.rewritten_prompt.is_none());
    assert!(d.chosen_model.is_none());
}

#[test]
fn promptadapter_rewrites_payload() {
    let mut reg = TransformRegistry::new();
    reg.register(Arc::new(IdentityTransform));
    let pa = PromptAdapter::new(reg);
    let d = pa
        .apply(&Request::new("identity:user:42", "hello world"))
        .expect("apply must succeed");
    assert!(matches!(d.decision, Decision::Allow));
    assert_eq!(d.rewritten_prompt.as_deref(), Some("hello world"));
    assert_eq!(d.chosen_model.as_deref(), Some("identity:user:42"));
}

#[test]
fn promptadapter_deny_propagates_via_response_mirror() {
    // Construct a transform that always errors so we exercise the
    // deny branch.
    struct AlwaysErrorTransform;
    impl phenotype_router::plugins::promptadapter::Transform for AlwaysErrorTransform {
        fn name(&self) -> &str {
            "always_error"
        }
        fn apply(&self, _: &Request) -> Result<Request, String> {
            Err("transform exploded".to_string())
        }
    }

    let mut reg = TransformRegistry::new();
    reg.register(Arc::new(AlwaysErrorTransform));
    let pa = PromptAdapter::new(reg);
    let d = pa
        .apply(&Request::new("always_error:user:42", "p"))
        .expect("apply must succeed even when transform errors");
    match &d.decision {
        Decision::Deny(reason) => assert!(reason.contains("transform exploded")),
        other => panic!("expected Deny, got {other:?}"),
    }
    // The Response mirror must agree with the decision kind (the
    // OTLP recorder consumes Response, not PluginDecision).
    let legacy: Response = match &d.decision {
        Decision::Allow => Response::allow(),
        Decision::Defer => Response::defer(),
        Decision::Deny(r) => Response::deny(r.clone()),
    };
    match legacy.decision {
        Decision::Deny(r) => assert!(r.contains("transform exploded")),
        other => panic!("expected legacy Deny, got {other:?}"),
    }
}

#[tokio::test]
async fn contextfolding_connector_round_trip() {
    let connector = ContextFoldingConnector::new();
    let cfg = ConnectorConfig {
        url: "https://fold.example/v1".to_string(),
        token: None,
        timeout_ms: None,
    };
    let handle: Box<dyn ConnectorHandle> = connector
        .connect(&cfg)
        .await
        .expect("connect must succeed");
    assert_eq!(handle.name(), "contextfolding");
    let status = handle.ping().expect("ping must succeed");
    assert_eq!(status, phenotype_router::sdk::HealthStatus::Healthy);
    // Confirm the connector exposes the active strategy.
    assert_eq!(connector.strategy().name(), "whitespace_dedupe");
    // Concrete-typed fold path is exercised via the per-plugin unit
    // tests against `ContextFoldHandle` directly (the SDK's
    // `Box<dyn ConnectorHandle>` erases the concrete `fold` method).
    // This integration test asserts the SDK contract — name + ping +
    // capabilities — is wired correctly across the substrate boundary.
}

#[tokio::test]
async fn researchintel_synthesizes_summary() {
    let ri = ResearchIntel::with_provider(Arc::new(SynthesizedResearchProvider));
    let req = LlmRequest {
        id: "user:42".to_string(),
        source_model: "gpt-4o".to_string(),
        target_model: "claude-3-opus".to_string(),
        prompt: "summarize research on Bifrost plugins".to_string(),
        routing_key: Some("tenant:a".to_string()),
    };
    let resp = ri.send(&req).await.expect("send must succeed");
    assert_eq!(resp.id, "user:42");
    assert_eq!(resp.model, "claude-3-opus");
    assert!(resp.completion.contains("research summary"));
    assert!(resp.tokens > 0);
    assert!(resp.cost_usd.is_some());
}

#[test]
fn researchintel_synthesized_provider_name_is_pinned() {
    let ri = ResearchIntel::new();
    assert_eq!(ri.provider().name(), "synthesized");
}

#[test]
fn all_three_plugins_implement_their_sdk_traits() {
    // Compile-time check that all 3 plugins implement their SDK traits.
    fn assert_decision_plugin<P: DecisionPlugin>(_: &P) {}
    fn assert_connector_port<P: ConnectorPort>(_: &P) {}
    fn assert_llm_port<P: LlmPort>(_: &P) {}

    let pa = PromptAdapter::with_defaults();
    assert_decision_plugin(&pa);
    assert_eq!(pa.phase(), Phase::RequestTransform);
    assert_eq!(pa.capabilities(), Capabilities::NONE);

    let c = ContextFoldingConnector::new();
    assert_connector_port(&c);
    assert!(c.capabilities().contains(Capabilities::NETWORK_IO));

    let ri = ResearchIntel::new();
    assert_llm_port(&ri);
    assert!(ri.capabilities().contains(Capabilities::NETWORK_IO));
}

#[tokio::test]
async fn full_plugin_chain_passes() {
    // End-to-end smoke: a single request flows through all 3 plugins
    // without error. Each plugin is exercised in isolation (the
    // router-level chaining is the consumer's job, not the SDK's)
    // but the test confirms the SDK contract composes.
    let mut reg = TransformRegistry::new();
    reg.register(Arc::new(IdentityTransform));
    let pa = PromptAdapter::new(reg);
    let cf = ContextFoldingConnector::new();
    let ri = ResearchIntel::new();

    // 1. promptadapter rewrites the payload (here, identity echo).
    let req = Request::new("identity:user:42", "hello world");
    let d = pa.apply(&req).expect("promptadapter must succeed");
    assert!(matches!(d.decision, Decision::Allow));

    // 2. contextfolding opens a fold session.
    let cfg = ConnectorConfig {
        url: "https://fold.example/v1".to_string(),
        token: None,
        timeout_ms: None,
    };
    let _handle = cf
        .connect(&cfg)
        .await
        .expect("contextfolding must connect");

    // 3. researchintel sends the (rewritten) prompt to the LLM port.
    let llm_req = LlmRequest {
        id: req.id.clone(),
        source_model: "gpt-4o".to_string(),
        target_model: "claude-3-opus".to_string(),
        prompt: d.rewritten_prompt.clone().unwrap_or(req.payload),
        routing_key: None,
    };
    let resp = ri
        .send(&llm_req)
        .await
        .expect("researchintel must send");
    assert!(resp.completion.contains("research summary"));
}

#[test]
fn plugin_name_strings_are_pinned_for_o11y() {
    // The OTel span keys are derived from these strings; pinning
    // them at the integration boundary prevents silent drift in
    // dashboards / alerts (ADR-012).
    assert_eq!(
        phenotype_router::plugins::promptadapter::PLUGIN_NAME,
        "promptadapter"
    );
    assert_eq!(
        phenotype_router::plugins::contextfolding::PLUGIN_NAME,
        "contextfolding"
    );
    assert_eq!(
        phenotype_router::plugins::researchintel::PLUGIN_NAME,
        "researchintel"
    );
    // Versions are pinned too (ADR-052 §5).
    for v in [
        phenotype_router::plugins::promptadapter::PLUGIN_VERSION,
        phenotype_router::plugins::contextfolding::PLUGIN_VERSION,
        phenotype_router::plugins::researchintel::PLUGIN_VERSION,
    ] {
        assert_eq!(v, "0.1.0");
    }
}

/// Compile-time witness that the synthesized provider satisfies
/// the `ResearchProvider` trait — pinned at the integration layer
/// so a future trait-shape change surfaces here, not in a
/// downstream consumer.
#[allow(dead_code)]
fn _research_provider_witness(p: &dyn ResearchProvider) -> &dyn ResearchProvider {
    p
}
