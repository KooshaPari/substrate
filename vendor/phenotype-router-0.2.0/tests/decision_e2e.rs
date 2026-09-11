//! End-to-end integration test for the decision layer.
//!
//! Exercises the full path: `Request → BifrostAdapter → Response →
//! assert invariants`. Mirrors what a downstream consumer (gateway,
//! bifrost-side adapter) would do; serves as the canonical e2e coverage
//! per ADR-040 (federated-service tier: ≥60 % coverage gate).

use phenotype_router::{BifrostAdapter, Decision, DecisionLayer, HelloWorld, HelloWorldPort, Request};

#[test]
fn bifrost_allows_plain_request() {
    let adapter = BifrostAdapter::new();
    let resp = adapter.decide(&Request::new("user:42", "what's the weather?"));
    assert_eq!(resp.decision, Decision::Allow);
    assert!(resp.trace.iter().any(|(k, v)| k == "router.adapter" && v == "bifrost"));
}

#[test]
fn bifrost_denies_id_with_deny_prefix() {
    let adapter = BifrostAdapter::new();
    let resp = adapter.decide(&Request::new("deny:user:1", "blocked"));
    match resp.decision {
        Decision::Deny(reason) => assert!(reason.contains("denied id=deny:user:1")),
        other => panic!("expected Deny, got {:?}", other),
    }
}

#[test]
fn bifrost_health_default_is_ok() {
    let adapter = BifrostAdapter::new();
    assert!(adapter.health().is_ok());
}

#[test]
fn hello_world_echoes_request() {
    let hw = HelloWorld;
    let resp = hw.hello(&Request::new("user:42", "hello"));
    assert_eq!(resp.id, "user:42");
    assert_eq!(resp.payload, "hello");
    assert_eq!(resp.decision, Decision::Allow);
}

#[test]
fn decision_kind_str_is_stable_across_calls() {
    // ADR-052 §3 pins the OTLP span attribute `phenotype.router.decision.kind`
    // to the value returned by `Decision::kind_str()`. Any change here is a
    // breaking observability change.
    let adapter = BifrostAdapter::new();
    let allow = adapter.decide(&Request::new("a", "p"));
    let deny = adapter.decide(&Request::new("deny:b", "p"));
    assert_eq!(allow.decision.kind_str(), "allow");
    assert_eq!(deny.decision.kind_str(), "deny");
}

#[test]
fn response_with_trace_appends_in_call_order() {
    // Used by adapters that need to surface trace annotations (e.g.
    // BifrostAdapter with `router.id` and `router.adapter` keys). Order
    // matters for stable OTLP attribute serialization.
    let adapter = BifrostAdapter::new();
    let resp = adapter.decide(&Request::new("user:42", "p"));
    let names: Vec<&str> = resp.trace.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, vec!["router.adapter", "router.id"]);
}

#[test]
fn sdk_phase_ordering_is_total() {
    use phenotype_router::Phase;
    let phases = [
        Phase::PreRouting,
        Phase::ProviderSelection,
        Phase::RequestTransform,
        Phase::ToolSelection,
        Phase::PostRouting,
        Phase::Observability,
    ];
    for window in phases.windows(2) {
        assert!(window[0].order() < window[1].order(), "phase order violated: {:?}", window);
    }
}

#[test]
fn sdk_capabilities_union_is_commutative() {
    use phenotype_router::Capabilities;
    let a = Capabilities::NETWORK_IO | Capabilities::REASONING_AWARE;
    let b = Capabilities::REASONING_AWARE | Capabilities::NETWORK_IO;
    assert_eq!(a, b);
    assert!(a.contains(Capabilities::NETWORK_IO));
    assert!(a.contains(Capabilities::REASONING_AWARE));
    assert!(!a.contains(Capabilities::STATEFUL));
}
