//! Hello-world parity test for the decision layer.
//!
//! Verifies:
//! - `BifrostAdapter` allows a normal request
//! - `BifrostAdapter` denies a request whose id starts with `deny:`
//! - `HelloWorld` echoes the request and returns `Allow`
//! - `hello_response` produces a valid `Response` with trace annotations

use phenotype_router::{
    hello_response, BifrostAdapter, Decision, DecisionLayer, HelloWorld, HelloWorldPort, Request,
    Response,
};

fn req(id: &str) -> Request {
    Request {
        id: id.to_string(),
        payload: "hello".to_string(),
    }
}

#[test]
fn bifrost_adapter_allows_normal_requests() {
    let adapter = BifrostAdapter::new();
    let resp = adapter.decide(&req("tool:search"));
    assert_eq!(resp.decision, Decision::Allow);
    assert!(resp.trace.iter().any(|(k, v)| k == "router.adapter" && v == "bifrost"));
}

#[test]
fn bifrost_adapter_denies_deny_prefixed_ids() {
    let adapter = BifrostAdapter::new();
    let resp = adapter.decide(&req("deny:tool:bad"));
    match resp.decision {
        Decision::Deny(reason) => assert!(reason.contains("bifrost")),
        other => panic!("expected Deny, got {:?}", other),
    }
}

#[test]
fn hello_world_echoes_request() {
    let hw = HelloWorld;
    let r = req("tool:echo");
    let out = hw.hello(&r);
    assert_eq!(out.id, "tool:echo");
    assert_eq!(out.payload, "hello");
    assert_eq!(out.decision, Decision::Allow);
}

#[test]
fn hello_response_has_trace_annotations() {
    let resp = hello_response(&req("tool:trace"));
    assert_eq!(resp.decision, Decision::Allow);
    assert!(resp.trace.iter().any(|(k, _)| k == "router.port"));
    assert!(resp.trace.iter().any(|(k, _)| k == "router.id"));
    assert!(resp.trace.iter().any(|(k, _)| k == "router.payload"));
}

#[test]
fn response_allow_helper_constructs_allow() {
    let r: Response = Response::allow();
    assert_eq!(r.decision, Decision::Allow);
    assert!(r.trace.is_empty());
}

#[test]
fn response_deny_helper_constructs_deny() {
    let r = Response::deny("nope");
    match r.decision {
        Decision::Deny(reason) => assert_eq!(reason, "nope"),
        other => panic!("expected Deny, got {:?}", other),
    }
}
