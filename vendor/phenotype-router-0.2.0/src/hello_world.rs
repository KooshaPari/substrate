//! Hello-world port fixture.
//!
//! Used as a baseline in parity tests against the real Bifrost adapter
//! (per ADR-050 § "Hello-world port + bifrost lib wrapper").

use crate::decision::{Decision, DecisionError, DecisionLayer, Request, Response};

/// Response from a [`HelloWorldPort`] invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloWorldResponse {
    /// Echo of the request id.
    pub id: String,
    /// Echo of the request payload.
    pub payload: String,
    /// Fixed decision for the hello-world fixture (always `Allow`).
    pub decision: Decision,
}

/// The hello-world port contract.
///
/// The default impl echoes the request id and payload through, returning a
/// fixed `Allow` decision. Adapters can compare their output to this
/// shape to verify wire-format parity.
pub trait HelloWorldPort: Send + Sync {
    /// Invoke the hello-world port.
    fn hello(&self, req: &Request) -> HelloWorldResponse;
}

/// Default hello-world port used as a fixture in parity tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct HelloWorld;

impl HelloWorldPort for HelloWorld {
    fn hello(&self, req: &Request) -> HelloWorldResponse {
        HelloWorldResponse {
            id: req.id.clone(),
            payload: req.payload.clone(),
            decision: Decision::Allow,
        }
    }
}

// `HelloWorld` is also a `DecisionLayer` so it can be plugged into the
// same chaos-matrix / OTLP-recorder harness as `BifrostAdapter`. The
// decision is a no-op (always Allow); the rest of the fixture shape
// (id + payload trace fields) is preserved.
impl DecisionLayer for HelloWorld {
    fn name(&self) -> &str {
        "hello-world"
    }

    fn adapter_kind(&self) -> &str {
        "hello-world"
    }

    fn health(&self) -> Result<(), DecisionError> {
        Ok(())
    }

    fn decide(&self, req: &Request) -> Response {
        hello_response(req)
    }
}

/// Helper for callers that need a [`Response`] view of a hello-world
/// invocation.
pub fn hello_response(req: &Request) -> Response {
    let hw = HelloWorld.hello(req);
    Response::allow()
        .with_trace("router.port", "hello-world")
        .with_trace("router.id", hw.id)
        .with_trace("router.payload", hw.payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_world_port_echoes_request() {
        let hw = HelloWorld;
        let r = Request::new("tool:echo", "hello");
        let out = hw.hello(&r);
        assert_eq!(out.id, "tool:echo");
        assert_eq!(out.payload, "hello");
        assert_eq!(out.decision, Decision::Allow);
    }

    #[test]
    fn hello_world_decision_layer_metadata() {
        let hw = HelloWorld;
        assert_eq!(hw.name(), "hello-world");
        assert_eq!(hw.adapter_kind(), "hello-world");
        assert!(hw.health().is_ok());
    }

    #[test]
    fn hello_world_decide_returns_allow_with_trace() {
        let resp = hello_response(&Request::new("user:1", "p"));
        assert!(resp.decision.is_allow());
        assert!(resp.trace.iter().any(|(k, _)| k == "router.port"));
        assert!(resp.trace.iter().any(|(k, _)| k == "router.id"));
        assert!(resp.trace.iter().any(|(k, _)| k == "router.payload"));
    }
}
