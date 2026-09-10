//! Router decision port contract (ADR-050 / ADR-038).
//!
//! The decision layer is the boundary where a [`Request`] is mapped to a
//! [`Response`]. Adapters (e.g. [`crate::BifrostAdapter`], [`crate::HelloWorld`])
//! implement [`DecisionLayer`] to plug in different decision strategies without
//! changing the call sites.
//!
//! ## Hexagonal L4 contract (ADR-038)
//!
//! Every decision-layer adapter implements four methods, mirroring the
//! [`pheno-port-adapter`] reference impl:
//!
//! - [`DecisionLayer::name`] — stable adapter identifier (used by OTLP span
//!   attributes and the plugin registry).
//! - [`DecisionLayer::health`] — liveness probe; consumed by the upstream
//!   health-aware provider pool (ADR-006 circuit-breaker pattern).
//! - [`DecisionLayer::decide`] — synchronous decision (the hot path).
//! - [`DecisionLayer::adapter_kind`] — schema tag (`"bifrost"`, `"hello-world"`).
//!
//! `decide()` is sync in v0.3.0; an async overlay is reserved behind the
//! `async` feature flag and is the v0.4.0 plan (see SPEC.md §6).

use thiserror::Error;

/// A request that the decision layer must resolve.
///
/// The shape is intentionally minimal for the v0.3.0 release; richer fields
/// (headers, tracing context, tenant ID, reasoning-effort toggle) will be
/// added once the Bifrost FFI bridge lands in v0.4.0 (see SPEC.md §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Stable identifier for the request (e.g. tool name, route).
    pub id: String,
    /// Free-form payload the adapter may inspect.
    pub payload: String,
}

impl Request {
    /// Construct a new [`Request`] from raw parts. Useful for tests and
    /// adapter authors that wrap a lower-level transport type.
    pub fn new(id: impl Into<String>, payload: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            payload: payload.into(),
        }
    }
}

/// A decision returned by an adapter.
///
/// Mirrors the bifrost decision-library's `Decision` enum
/// (`Allow | Deny(reason)`); a `Defer` variant is reserved for the
/// plugin-chain phase ordering in v0.4.0 (ADR-052 §1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allow the request to proceed.
    Allow,
    /// Defer to the next plugin in the chain (ADR-052 §1.1; reserved —
    /// no consumer in v0.3.0; emitted as `Allow` in the OTLP span
    /// attribute until v0.4.0 lands).
    Defer,
    /// Reject the request with a human-readable reason.
    Deny(String),
}

impl Decision {
    /// Short identifier used in OTLP span attributes (`phenotype.decision.kind`).
    pub fn kind_str(&self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Defer => "defer",
            Decision::Deny(_) => "deny",
        }
    }

    /// True iff this is [`Decision::Allow`]. Convenience for adapter authors.
    pub fn is_allow(&self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// A response returned by an adapter. Wraps a [`Decision`] plus any
/// adapter-specific side-channel data (currently a list of trace fields
/// the call site can attach to a span).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The decision itself.
    pub decision: Decision,
    /// Optional annotations the call site can attach to a span/log line.
    pub trace: Vec<(String, String)>,
}

impl Response {
    /// Construct an `Allow` response with no trace annotations.
    pub fn allow() -> Self {
        Self {
            decision: Decision::Allow,
            trace: Vec::new(),
        }
    }

    /// Construct a `Defer` response with no trace annotations.
    /// Reserved for v0.4.0 (ADR-052 plugin-chain phase ordering); exposed
    /// now so adapter authors can prepare.
    pub fn defer() -> Self {
        Self {
            decision: Decision::Defer,
            trace: Vec::new(),
        }
    }

    /// Construct a `Deny` response with a reason and no trace annotations.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            decision: Decision::Deny(reason.into()),
            trace: Vec::new(),
        }
    }

    /// Attach a single trace annotation. Fluent style for adapter authors.
    pub fn with_trace(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.trace.push((key.into(), value.into()));
        self
    }
}

/// Errors emitted by the decision layer.
#[derive(Debug, Error)]
pub enum DecisionError {
    /// The adapter could not evaluate the request.
    #[error("adapter error: {0}")]
    Adapter(String),
    /// The adapter is currently unhealthy (liveness probe failed).
    /// Callers should retry against a fallback adapter per ADR-006.
    #[error("adapter unhealthy: {0}")]
    Unhealthy(String),
}

/// The port trait every router decision adapter must implement.
///
/// Mirrors the [`pheno_port_adapter::PortAdapter`] hexagonal L4 contract
/// (ADR-038) and adds the per-request `decide` hook that the router needs.
/// All adapters MUST be `Send + Sync` so the layer can be plugged into the
/// fleet-wide concurrency model.
pub trait DecisionLayer: Send + Sync {
    /// Stable adapter identifier. MUST be unique fleet-wide; used as the
    /// `phenotype.router.adapter` OTLP span attribute (ADR-052 §3).
    fn name(&self) -> &str;

    /// Schema tag for the adapter implementation. Defaults to `name()`;
    /// adapters override to distinguish `bifrost` from a hello-world fixture
    /// that happens to share an instance name.
    fn adapter_kind(&self) -> &str {
        self.name()
    }

    /// Liveness probe. Default impl returns Ok (the adapter is always
    /// available); health-aware adapters (e.g. the future `SmartFallback`
    /// port) override to surface circuit-breaker state (ADR-006).
    fn health(&self) -> Result<(), DecisionError> {
        Ok(())
    }

    /// Resolve a request into a response.
    ///
    /// MUST be cheap — the hot path. Network I/O is allowed only when the
    /// adapter has explicitly opted in via an internal flag (cf. ADR-052
    /// §1 `CapNetworkIO`); the in-tree adapters (`BifrostAdapter`,
    /// `HelloWorld`) are pure compute.
    fn decide(&self, req: &Request) -> Response;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_kind_str_matches_variant() {
        assert_eq!(Decision::Allow.kind_str(), "allow");
        assert_eq!(Decision::Defer.kind_str(), "defer");
        assert_eq!(Decision::Deny("x".to_string()).kind_str(), "deny");
    }

    #[test]
    fn decision_is_allow_helper() {
        assert!(Decision::Allow.is_allow());
        assert!(!Decision::Defer.is_allow());
        assert!(!Decision::Deny("nope".to_string()).is_allow());
    }

    #[test]
    fn response_allow_has_empty_trace() {
        let r = Response::allow();
        assert_eq!(r.decision, Decision::Allow);
        assert!(r.trace.is_empty());
    }

    #[test]
    fn response_defer_has_empty_trace() {
        let r = Response::defer();
        assert_eq!(r.decision, Decision::Defer);
        assert!(r.trace.is_empty());
    }

    #[test]
    fn response_deny_carries_reason() {
        let r = Response::deny("blocked");
        match r.decision {
            Decision::Deny(reason) => assert_eq!(reason, "blocked"),
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn response_with_trace_appends_key_value() {
        let r = Response::allow()
            .with_trace("adapter", "bifrost")
            .with_trace("id", "tool:search");
        assert_eq!(r.trace.len(), 2);
        assert_eq!(r.trace[0], ("adapter".to_string(), "bifrost".to_string()));
        assert_eq!(r.trace[1], ("id".to_string(), "tool:search".to_string()));
    }

    #[test]
    fn request_new_constructs_fields() {
        let r = Request::new("tool:echo", "hello");
        assert_eq!(r.id, "tool:echo");
        assert_eq!(r.payload, "hello");
    }

    #[test]
    fn decision_error_displays_adapter_prefix() {
        let e = DecisionError::Adapter("boom".to_string());
        assert_eq!(e.to_string(), "adapter error: boom");
    }

    #[test]
    fn decision_error_displays_unhealthy_prefix() {
        let e = DecisionError::Unhealthy("circuit-open".to_string());
        assert_eq!(e.to_string(), "adapter unhealthy: circuit-open");
    }

    /// A trivial adapter that exposes the default `health` impl; this is
    /// the round-trip test for the new `name` / `adapter_kind` methods.
    struct IdentityAdapter;
    impl DecisionLayer for IdentityAdapter {
        fn name(&self) -> &str { "identity" }
        fn decide(&self, req: &Request) -> Response {
            Response::allow().with_trace("identity.id", req.id.clone())
        }
    }

    #[test]
    fn default_health_returns_ok() {
        let a = IdentityAdapter;
        assert!(a.health().is_ok());
    }

    #[test]
    fn default_adapter_kind_falls_back_to_name() {
        let a = IdentityAdapter;
        assert_eq!(a.adapter_kind(), "identity");
    }
}
