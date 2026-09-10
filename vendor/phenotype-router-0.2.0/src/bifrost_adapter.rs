//! Bifrost adapter stub (ADR-050 / ADR-051).
//!
//! The real Bifrost decision library is implemented in Go and lives in
//! `phenotype-gateway/packages/bifrost/`. The Rust-side adapter is a stub
//! for the hello-world scaffold; a future FFI bridge will replace the
//! body of [`BifrostAdapter::decide`] with a call into the Go module.

use crate::decision::{Decision, DecisionLayer, Request, Response};

/// Stub adapter that mirrors the Bifrost decision library's external
/// contract: requests whose id starts with `"deny:"` are denied, all
/// others are allowed. This mirrors the real Bifrost behaviour closely
/// enough to be a valid hello-world fixture.
#[derive(Debug, Default, Clone, Copy)]
pub struct BifrostAdapter;

impl BifrostAdapter {
    /// Construct a new adapter. The const fn is used so the type can be
    /// declared `const`-friendly at call sites.
    pub const fn new() -> Self {
        Self
    }
}

impl DecisionLayer for BifrostAdapter {
    fn name(&self) -> &str {
        "bifrost"
    }

    fn adapter_kind(&self) -> &str {
        "bifrost"
    }

    fn decide(&self, req: &Request) -> Response {
        if req.id.starts_with("deny:") {
            return Response {
                decision: Decision::Deny(format!("bifrost: denied id={}", req.id)),
                trace: vec![("router.adapter".to_string(), "bifrost".to_string())],
            };
        }
        Response {
            decision: Decision::Allow,
            trace: vec![
                ("router.adapter".to_string(), "bifrost".to_string()),
                ("router.id".to_string(), req.id.clone()),
            ],
        }
    }
}
