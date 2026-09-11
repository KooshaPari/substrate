//! Plugin SDK traits — Rust port of ADR-052 plugin SDK spec.
//!
//! ADR-052 (the Bifrost plugin SDK spec) is defined in Go. This module is
//! the **Rust** translation of the same contract:
//!
//! - [`LlmPort`] — async provider port. Mirrors ADR-051 §4 transport rules:
//!   plugins call into the router via this trait; the trait is the only
//!   surface that crosses the substrate boundary.
//! - [`DecisionPlugin`] — sync in-process decision hook. Mirrors ADR-052
//!   §1 (5-method Plugin interface), narrowed for the v0.1 lib surface.
//! - [`ConnectorPort`] — async external-data connector (vector stores,
//!   retrievers, MCP servers, remote HTTP services). Mirrors ADR-052 §1
//!   plugin-with-`CapNetworkIO` shape.
//!
//! All three traits emit OTel-compatible spans via the `tracing` crate
//! (ADR-012 / ADR-036B — `pheno-tracing` substrate). Plugin authors MUST
//! wrap `apply` / `send` / `connect` in a `#[tracing::instrument]` span
//! named `phenotype.router.plugin.<plugin-name>.apply` per ADR-052 §3.

use async_trait::async_trait;
use thiserror::Error;

/// Pipeline phase a plugin participates in (ADR-052 §1.1).
///
/// The router invokes plugins in `Phase` order. A plugin may only register
/// for one phase (the router rejects multi-phase plugins at load time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Pre-routing (e.g. `contentsafety` with `CapPreRoutingMandatory`).
    PreRouting,
    /// Provider selection (e.g. `intelligentrouter`, `smart-fallback`).
    ProviderSelection,
    /// Request transform (e.g. `promptadapter`, `contextfolding`).
    RequestTransform,
    /// Tool selection (e.g. `toolrouter`).
    ToolSelection,
    /// Post-routing (e.g. `voyage` rerank, `researchintel`).
    PostRouting,
    /// Observability hooks; no flow mutation.
    Observability,
}

impl Phase {
    /// Stable ordering for the router's plugin dispatch.
    pub fn order(self) -> u8 {
        match self {
            Phase::PreRouting => 0,
            Phase::ProviderSelection => 1,
            Phase::RequestTransform => 2,
            Phase::ToolSelection => 3,
            Phase::PostRouting => 4,
            Phase::Observability => 5,
        }
    }
}

/// Capabilities bitmask (ADR-052 §1).
///
/// Plugins opt in to behaviors; plugins that don't opt in MUST NOT perform
/// the corresponding behavior (the router may skip the plugin if a
/// capability is required).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities(pub u32);

impl Capabilities {
    /// No capabilities.
    pub const NONE: Capabilities = Capabilities(0);
    /// Plugin makes outbound calls (network I/O).
    pub const NETWORK_IO: Capabilities = Capabilities(1 << 0);
    /// Plugin maintains per-process state (rare; loses hot-reload).
    pub const STATEFUL: Capabilities = Capabilities(1 << 1);
    /// Must run in `Phase::PreRouting` (mandatory; e.g. `contentsafety`).
    pub const PRE_ROUTING_MANDATORY: Capabilities = Capabilities(1 << 2);
    /// Plugin reasons about o1/o3-style tokens.
    pub const REASONING_AWARE: Capabilities = Capabilities(1 << 3);

    /// Returns true if `self` includes all bits of `other`.
    pub fn contains(self, other: Capabilities) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Bitwise OR — combines two capability sets.
    pub fn union(self, other: Capabilities) -> Capabilities {
        Capabilities(self.0 | other.0)
    }
}

impl std::ops::BitOr for Capabilities {
    type Output = Capabilities;
    fn bitor(self, rhs: Capabilities) -> Capabilities {
        self.union(rhs)
    }
}

impl std::fmt::Display for Capabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.contains(Capabilities::NETWORK_IO) {
            parts.push("NETWORK_IO");
        }
        if self.contains(Capabilities::STATEFUL) {
            parts.push("STATEFUL");
        }
        if self.contains(Capabilities::PRE_ROUTING_MANDATORY) {
            parts.push("PRE_ROUTING_MANDATORY");
        }
        if self.contains(Capabilities::REASONING_AWARE) {
            parts.push("REASONING_AWARE");
        }
        if parts.is_empty() {
            f.write_str("NONE")
        } else {
            f.write_str(&parts.join("|"))
        }
    }
}

/// Health status of an async port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Port is reachable and ready to serve.
    Healthy,
    /// Port is reachable but degraded; the router may shed load.
    Degraded,
    /// Port is unreachable; the router MUST route around it.
    Unhealthy,
}

/// Errors emitted by `LlmPort` adapters.
#[derive(Debug, Error)]
pub enum LlmError {
    /// Provider returned a non-2xx HTTP status.
    #[error("provider error: status={status} body={body}")]
    Provider {
        /// HTTP status code from the upstream provider.
        status: u16,
        /// Raw response body (truncated to 4 KiB by the port layer).
        body: String,
    },
    /// Network-level failure (DNS, connect, timeout).
    #[error("network error: {0}")]
    Network(String),
    /// The request was malformed.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// The port is not currently configured (missing credentials, etc.).
    #[error("port not configured: {0}")]
    NotConfigured(String),
}

/// Errors emitted by `DecisionPlugin` adapters.
#[derive(Debug, Error)]
pub enum PluginError {
    /// The plugin's decision logic raised a recoverable error.
    #[error("plugin error: {0}")]
    Plugin(String),
    /// The plugin was asked to handle a request shape it doesn't support.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Errors emitted by `ConnectorPort` adapters.
#[derive(Debug, Error)]
pub enum ConnectorError {
    /// Failed to establish the data-plane connection.
    #[error("connect error: {0}")]
    Connect(String),
    /// Authentication or authorization failed.
    #[error("auth error: {0}")]
    Auth(String),
    /// The connector's data source returned an error response.
    #[error("backend error: {0}")]
    Backend(String),
}

/// Minimal request shape used by [`LlmPort::send`].
///
/// Plugin authors who need richer payloads should compose with the
/// `phenotype-router` [`crate::decision::Request`] type at the call site;
/// the LLM-port surface stays minimal to keep async-port contracts tight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRequest {
    /// Stable request identifier (used for span attribution).
    pub id: String,
    /// Source model family (e.g. `"gpt-4o"`, `"claude-3"`).
    pub source_model: String,
    /// Target model family (e.g. `"claude-3-opus"`).
    pub target_model: String,
    /// Free-form prompt payload.
    pub prompt: String,
    /// Caller-supplied tenant / routing key.
    pub routing_key: Option<String>,
}

/// Response from an [`LlmPort::send`] call.
///
/// `cost_usd` is `f64`, so the type implements `PartialEq` only (not `Eq`).
/// That is fine: `LlmResponse` is never used as a `HashMap` key; equality
/// is only ever asserted in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    /// Echo of [`LlmRequest::id`].
    pub id: String,
    /// Model that produced the response.
    pub model: String,
    /// Free-form completion.
    pub completion: String,
    /// Tokens consumed (input + output).
    pub tokens: u32,
    /// Estimated cost in USD; `None` if the port cannot estimate.
    pub cost_usd: Option<f64>,
}

/// Connector configuration handed to [`ConnectorPort::connect`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorConfig {
    /// Connector-specific URL (HTTP endpoint, file path, etc.).
    pub url: String,
    /// Optional credentials token.
    pub token: Option<String>,
    /// Optional request timeout in milliseconds.
    pub timeout_ms: Option<u32>,
}

/// Handle returned by [`ConnectorPort::connect`]; concrete types live in
/// the implementing crate. The trait specifies `name` + `ping` for
/// health checks. Downcasting back to the concrete handle type happens
/// via the `name()` discriminator at the call site (no `Any` upcast —
/// `dyn Any` is not `Send + Sync`, and the connector boundary requires
/// both). The `name()` approach is enough for the v0.3.0 surface because
/// every concrete handle publishes a unique name; v0.4.0 may add a
/// `Box<dyn Any + Send + Sync>` upcast if a downcast use case emerges.
pub trait ConnectorHandle: Send + Sync + std::fmt::Debug {
    /// Stable handle name (for span attribution + metrics).
    fn name(&self) -> &str;
    /// Cheap liveness probe.
    fn ping(&self) -> Result<HealthStatus, ConnectorError>;
}

/// LlmPort — async provider port (ADR-051 transport-surface translation).
///
/// Mirrors the Bifrost plugin's `TransportInterceptor` + provider-call
/// shape, narrowed to a single async `send` method. The port is the
/// **only** surface that crosses the substrate boundary: plugin authors
/// MUST NOT call upstream provider SDKs directly.
#[async_trait]
pub trait LlmPort: Send + Sync {
    /// Stable, kebab-case plugin name (e.g. `"researchintel"`).
    fn name(&self) -> &str;
    /// Plugin semver (ADR-052 §5).
    fn version(&self) -> &str;
    /// Capability bitmask.
    fn capabilities(&self) -> Capabilities {
        Capabilities::NETWORK_IO
    }
    /// Issue one request. MUST emit exactly one OTel span (see module docs).
    async fn send(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError>;
    /// Health probe. MUST be cheap (no provider I/O).
    async fn health(&self) -> Result<HealthStatus, LlmError> {
        Ok(HealthStatus::Healthy)
    }
    /// Best-effort cost estimate; `None` if the port cannot estimate.
    fn cost_estimate(&self, _req: &LlmRequest) -> Option<f64> {
        None
    }
}

/// DecisionPlugin — sync in-process router decision hook (ADR-052 §1).
///
/// The router invokes `apply` once per request that flows through the
/// plugin's [`Phase`]. `apply` MUST be cheap (no network I/O unless the
/// plugin declared `CapNetworkIO`).
pub trait DecisionPlugin: Send + Sync {
    /// Stable, kebab-case plugin name.
    fn name(&self) -> &str;
    /// Plugin semver.
    fn version(&self) -> &str;
    /// Which pipeline phase the plugin runs in.
    fn phase(&self) -> Phase;
    /// Capability bitmask.
    fn capabilities(&self) -> Capabilities {
        Capabilities::NONE
    }
    /// Resolve a request into a decision.
    ///
    /// MUST emit exactly one OTel span named
    /// `phenotype.router.plugin.<name>.apply` per ADR-052 §3.
    fn apply(&self, req: &crate::decision::Request) -> Result<PluginDecision, PluginError>;
}

/// Decision returned by a [`DecisionPlugin`].
///
/// This is the plugin-layer view of [`crate::decision::Decision`]: it adds
/// optional metadata (rewritten prompt, chosen model, annotations) that
/// the router can attach to spans and downstream phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDecision {
    /// Underlying allow / deny decision.
    pub decision: crate::decision::Decision,
    /// Optional rewritten prompt the router should pass downstream.
    pub rewritten_prompt: Option<String>,
    /// Optional model override (e.g. promptadapter picks a better target).
    pub chosen_model: Option<String>,
    /// Free-form annotations for span / log attachment.
    pub annotations: Vec<(String, String)>,
}

impl PluginDecision {
    /// Allow with no metadata.
    pub fn allow() -> Self {
        Self {
            decision: crate::decision::Decision::Allow,
            rewritten_prompt: None,
            chosen_model: None,
            annotations: Vec::new(),
        }
    }

    /// Allow + rewritten prompt.
    pub fn rewrite(prompt: impl Into<String>) -> Self {
        Self {
            decision: crate::decision::Decision::Allow,
            rewritten_prompt: Some(prompt.into()),
            chosen_model: None,
            annotations: Vec::new(),
        }
    }

    /// Allow + model override.
    pub fn route_to(model: impl Into<String>) -> Self {
        Self {
            decision: crate::decision::Decision::Allow,
            rewritten_prompt: None,
            chosen_model: Some(model.into()),
            annotations: Vec::new(),
        }
    }

    /// Deny with reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            decision: crate::decision::Decision::Deny(reason.into()),
            rewritten_prompt: None,
            chosen_model: None,
            annotations: Vec::new(),
        }
    }
}

/// ConnectorPort — async external-data connector (vector stores,
/// retrievers, MCP servers, remote HTTP services).
///
/// Plugins that need to fetch data from a remote backend implement
/// `ConnectorPort` rather than reaching across the substrate boundary
/// directly. The router enforces the contract (ADR-051 §4).
#[async_trait]
pub trait ConnectorPort: Send + Sync {
    /// Stable, kebab-case connector name.
    fn name(&self) -> &str;
    /// Plugin semver.
    fn version(&self) -> &str;
    /// Capability bitmask; `NETWORK_IO` is implied but explicit for clarity.
    fn capabilities(&self) -> Capabilities {
        Capabilities::NETWORK_IO
    }
    /// Open a connection. MUST emit exactly one OTel span named
    /// `phenotype.router.plugin.<name>.connect` per ADR-052 §3.
    async fn connect(
        &self,
        cfg: &ConnectorConfig,
    ) -> Result<Box<dyn ConnectorHandle>, ConnectorError>;
    /// Health probe. MUST be cheap.
    async fn health(&self) -> Result<HealthStatus, ConnectorError> {
        Ok(HealthStatus::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_contains_and_union() {
        let c = Capabilities::NETWORK_IO | Capabilities::REASONING_AWARE;
        assert!(c.contains(Capabilities::NETWORK_IO));
        assert!(c.contains(Capabilities::REASONING_AWARE));
        assert!(!c.contains(Capabilities::STATEFUL));
    }

    #[test]
    fn capabilities_display() {
        assert_eq!(format!("{}", Capabilities::NONE), "NONE");
        assert_eq!(
            format!("{}", Capabilities::NETWORK_IO | Capabilities::STATEFUL),
            "NETWORK_IO|STATEFUL"
        );
    }

    #[test]
    fn phase_ordering_is_stable() {
        let phases = [
            Phase::PreRouting,
            Phase::ProviderSelection,
            Phase::RequestTransform,
            Phase::ToolSelection,
            Phase::PostRouting,
            Phase::Observability,
        ];
        for window in phases.windows(2) {
            assert!(window[0].order() < window[1].order());
        }
    }

    #[test]
    fn plugin_decision_helpers() {
        let allow = PluginDecision::allow();
        assert_eq!(allow.decision, crate::decision::Decision::Allow);
        assert!(allow.rewritten_prompt.is_none());

        let rewritten = PluginDecision::rewrite("hello");
        assert_eq!(rewritten.rewritten_prompt.as_deref(), Some("hello"));

        let routed = PluginDecision::route_to("claude-3-opus");
        assert_eq!(routed.chosen_model.as_deref(), Some("claude-3-opus"));

        let denied = PluginDecision::deny("blocked");
        match denied.decision {
            crate::decision::Decision::Deny(reason) => assert_eq!(reason, "blocked"),
            _ => panic!("expected deny"),
        }
    }
}
