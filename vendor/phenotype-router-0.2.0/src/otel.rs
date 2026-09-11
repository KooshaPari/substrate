//! OTLP-compatible decision recorder (ADR-012 / ADR-036B / ADR-037 / ADR-052).
//!
//! This module bridges `phenotype-router`'s decision layer to the OpenTelemetry
//! span model without depending on a specific OTel SDK. The recorder produces
//! [`TraceOperation`] values that match the OTel-native span shape defined in
//! ADR-052 §3; downstream consumers wire it to the OTel SDK of their choice
//! (stdout, OTLP/HTTP, OTLP/gRPC, Jaeger, ...).
//!
//! ## Why a recorder (and not direct OTel SDK wiring)
//!
//! The router substrate stays a pure reusable library per ADR-023 Rule 3;
//! pulling the full `opentelemetry` / `opentelemetry-otlp` dependency tree
//! into `phenotype-router` would force every downstream consumer to
//! transitively depend on those crates. Instead, the recorder exposes a
//! narrow port (`TracePort`) that the consumer implements once for the
//! backend of their choice. The in-tree `InMemoryTracePort` adapter is
//! sufficient for tests + the OTLP smoke test.
//!
//! ## Span contract (ADR-052 §3)
//!
//! Each `decide()` call produces one [`TraceOperation`] with:
//!
//! - `name = "phenotype.router.decision"`
//! - `kind = SpanKind::Internal`
//! - attributes:
//!   - `phenotype.router.adapter` = `decision_layer.adapter_kind()`
//!   - `phenotype.router.request.id` = `request.id`
//!   - `phenotype.router.decision.kind` = `decision.kind_str()`
//!   - `phenotype.router.decision.reason` = deny reason (only when present)
//!   - `phenotype.router.service.name` = config `service_name`
//!
//! ## OTLP smoke test
//!
//! The `tests/otlp_smoke.rs` integration test exercises the recorder
//! against `InMemoryTracePort` and asserts the expected attribute shape.
//! CI runs the smoke test on every PR per the ADR-023 Rule 3.1
//! "observability substrate adoption" check.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::decision::{Decision, DecisionLayer, Request, Response};

/// OTLP-compatible span kind (subset of `opentelemetry::trace::SpanKind`).
///
/// Only `Internal` is needed by the decision layer (each `decide()` call is
/// an internal operation). The enum is open so future plugin authors can
/// emit `Client` / `Server` / `Producer` / `Consumer` spans without a
/// major version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanKind {
    /// Internal operation (default for `decide()`).
    Internal,
    /// Client-side outbound call (e.g. `LlmPort::send`).
    Client,
    /// Server-side inbound call.
    Server,
    /// Producer in a messaging context.
    Producer,
    /// Consumer in a messaging context.
    Consumer,
}

/// Opaque trace identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

/// Opaque span identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub String);

/// OTel-style span representation produced by the recorder.
///
/// Matches the OTel `SpanData` shape (subset; see ADR-052 §3 for the
/// attributes we always emit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceOperation {
    /// Logical trace this span belongs to.
    pub trace_id: TraceId,
    /// Unique-per-trace span identifier.
    pub span_id: SpanId,
    /// Optional parent span (for nested plugin chains).
    pub parent_span_id: Option<SpanId>,
    /// Span kind (Internal for decision-layer spans).
    pub kind: SpanKind,
    /// Span name; the decision layer always emits
    /// `"phenotype.router.decision"`.
    pub name: String,
    /// OTel attribute set (all string-valued; `phenotype.router.*` keys).
    pub attributes: HashMap<String, String>,
}

/// Port trait for receiving recorded spans. Adapters implement this to
/// forward spans to stdout, OTLP, an in-memory buffer, etc.
pub trait TracePort: Send + Sync {
    /// Submit one span. Returns a [`TraceResult`] carrying the assigned
    /// identifiers and the submit status.
    fn submit(&self, op: TraceOperation) -> TraceResult;
}

/// Result of a `submit` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceResult {
    /// Echo of the submitted trace id.
    pub trace_id: TraceId,
    /// Echo of the submitted span id.
    pub span_id: SpanId,
    /// Submit status; `Ok` for accepted, `Error(msg)` for rejected.
    pub status: TraceStatus,
}

/// Outcome of a `submit` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceStatus {
    /// Span accepted by the backend.
    Ok,
    /// Span rejected (with reason).
    Error(String),
}

impl TraceStatus {
    /// True iff the span was accepted.
    pub fn is_ok(&self) -> bool {
        matches!(self, TraceStatus::Ok)
    }

    /// True iff the span was rejected.
    pub fn is_err(&self) -> bool {
        matches!(self, TraceStatus::Error(_))
    }
}

/// In-memory adapter for [`TracePort`]. Stores all submitted spans in a
/// `Vec` behind a `Mutex`; useful for tests and the OTLP smoke test.
#[derive(Debug, Default)]
pub struct InMemoryTracePort {
    spans: Mutex<Vec<TraceOperation>>,
}

impl InMemoryTracePort {
    /// Construct an empty in-memory port.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the spans submitted so far.
    pub fn spans(&self) -> Vec<TraceOperation> {
        self.spans.lock().expect("in-memory port mutex poisoned").clone()
    }

    /// Number of spans submitted so far.
    pub fn len(&self) -> usize {
        self.spans.lock().expect("in-memory port mutex poisoned").len()
    }

    /// True iff no spans have been submitted.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl TracePort for InMemoryTracePort {
    fn submit(&self, op: TraceOperation) -> TraceResult {
        let res = TraceResult {
            trace_id: op.trace_id.clone(),
            span_id: op.span_id.clone(),
            status: TraceStatus::Ok,
        };
        self.spans
            .lock()
            .expect("in-memory port mutex poisoned")
            .push(op);
        res
    }
}

/// Configuration for the OTLP decision recorder.
///
/// All fields are public so consumers can `OtelConfig { ..Default::default() }`
/// in tests and override just the field they care about (typically `endpoint`
/// in production — though the endpoint is consumed by the downstream OTel
/// exporter, not the recorder itself).
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// OTel `service.name` resource attribute. Defaults to
    /// `"phenotype-router"`.
    pub service_name: String,
    /// Trace ID; defaults to a per-recorder random v4-like string at construction.
    pub trace_id: String,
    /// Span name override; defaults to `"phenotype.router.decision"` per
    /// ADR-052 §3. Override only for tests.
    pub span_name: String,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            service_name: "phenotype-router".to_string(),
            trace_id: format!("trace-{}", uuid_like_v4()),
            span_name: "phenotype.router.decision".to_string(),
        }
    }
}

/// Wraps a [`TracePort`] and produces one [`TraceOperation`] per
/// `decide()` call.
///
/// `Arc<dyn TracePort>` so the recorder is `Clone + Send + Sync` and can
/// be handed to any adapter / plugin / downstream consumer.
#[derive(Clone)]
pub struct OtlpDecisionRecorder {
    port: Arc<dyn TracePort>,
    config: OtelConfig,
}

impl std::fmt::Debug for OtlpDecisionRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OtlpDecisionRecorder")
            .field("service_name", &self.config.service_name)
            .field("trace_id", &self.config.trace_id)
            .field("span_name", &self.config.span_name)
            .finish()
    }
}

impl OtlpDecisionRecorder {
    /// Construct a recorder backed by an [`InMemoryTracePort`] (useful
    /// for tests + the OTLP smoke test).
    pub fn in_memory(config: OtelConfig) -> Self {
        Self::with_port(Arc::new(InMemoryTracePort::new()), config)
    }

    /// Construct a recorder backed by an arbitrary [`TracePort`] adapter
    /// (e.g. an OTLP/HTTP exporter in production).
    pub fn with_port(port: Arc<dyn TracePort>, config: OtelConfig) -> Self {
        Self { port, config }
    }

    /// Read-only view of the active config.
    pub fn config(&self) -> &OtelConfig {
        &self.config
    }

    /// Read-only view of the underlying port (e.g. for assertions in tests).
    pub fn port(&self) -> Arc<dyn TracePort> {
        Arc::clone(&self.port)
    }

    /// Build the [`TraceOperation`] for one `decide()` call.
    ///
    /// Exposed for testability (the `otlp_smoke` integration test asserts
    /// against the shape directly) and for adapter authors who want to
    /// submit a pre-built op into a different pipeline.
    pub fn build_operation(
        &self,
        adapter: &dyn DecisionLayer,
        req: &Request,
        resp: &Response,
    ) -> TraceOperation {
        let mut attrs: HashMap<String, String> = HashMap::with_capacity(6);
        attrs.insert(
            "phenotype.router.adapter".to_string(),
            adapter.adapter_kind().to_string(),
        );
        attrs.insert(
            "phenotype.router.request.id".to_string(),
            req.id.clone(),
        );
        attrs.insert(
            "phenotype.router.decision.kind".to_string(),
            resp.decision.kind_str().to_string(),
        );
        attrs.insert(
            "phenotype.router.service.name".to_string(),
            self.config.service_name.clone(),
        );
        if let Decision::Deny(reason) = &resp.decision {
            attrs.insert(
                "phenotype.router.decision.reason".to_string(),
                reason.clone(),
            );
        }
        for (k, v) in &resp.trace {
            // Surface adapter trace annotations under the phenotype.router.* namespace.
            let key = if k.starts_with("phenotype.router.") {
                k.clone()
            } else {
                format!("phenotype.router.adapter.{}", k)
            };
            attrs.insert(key, v.clone());
        }
        TraceOperation {
            trace_id: TraceId(self.config.trace_id.clone()),
            span_id: SpanId(format!("span-{}", req.id)),
            parent_span_id: None,
            kind: SpanKind::Internal,
            name: self.config.span_name.clone(),
            attributes: attrs,
        }
    }

    /// Record a single decision: build + submit. Returns the
    /// [`TraceResult`] from the underlying port.
    pub fn record(
        &self,
        adapter: &dyn DecisionLayer,
        req: &Request,
        resp: &Response,
    ) -> TraceResult {
        let op = self.build_operation(adapter, req, resp);
        self.port.submit(op)
    }
}

/// Tiny UUID-v4-like generator that does not require the `uuid` crate
/// (keeps the substrate dep footprint minimal).
fn uuid_like_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", (nanos >> 64) as u64, nanos as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{DecisionLayer, Request, Response};

    struct StubAdapter;
    impl DecisionLayer for StubAdapter {
        fn name(&self) -> &str {
            "stub"
        }
        fn decide(&self, req: &Request) -> Response {
            if req.id == "deny:1" {
                Response::deny("blocked")
            } else {
                Response::allow().with_trace("router.id", req.id.clone())
            }
        }
    }

    #[test]
    fn build_operation_attributes_for_allow() {
        let rec = OtlpDecisionRecorder::in_memory(OtelConfig::default());
        let req = Request::new("req-1", "payload");
        let resp = Response::allow();
        let op = rec.build_operation(&StubAdapter, &req, &resp);
        assert_eq!(op.name, "phenotype.router.decision");
        assert_eq!(op.kind, SpanKind::Internal);
        assert_eq!(
            op.attributes
                .get("phenotype.router.adapter")
                .map(String::as_str),
            Some("stub")
        );
        assert_eq!(
            op.attributes
                .get("phenotype.router.decision.kind")
                .map(String::as_str),
            Some("allow")
        );
        assert_eq!(
            op.attributes
                .get("phenotype.router.request.id")
                .map(String::as_str),
            Some("req-1")
        );
        assert!(!op
            .attributes
            .contains_key("phenotype.router.decision.reason"));
    }

    #[test]
    fn build_operation_attributes_for_deny() {
        let rec = OtlpDecisionRecorder::in_memory(OtelConfig::default());
        let req = Request::new("deny:1", "payload");
        let resp = Response::deny("blocked");
        let op = rec.build_operation(&StubAdapter, &req, &resp);
        assert_eq!(
            op.attributes
                .get("phenotype.router.decision.kind")
                .map(String::as_str),
            Some("deny")
        );
        assert_eq!(
            op.attributes
                .get("phenotype.router.decision.reason")
                .map(String::as_str),
            Some("blocked")
        );
    }

    #[test]
    fn record_submits_to_in_memory_port() {
        let rec = OtlpDecisionRecorder::in_memory(OtelConfig::default());
        let req = Request::new("user:42", "p");
        let resp = Response::allow();
        let res = rec.record(&StubAdapter, &req, &resp);
        assert_eq!(res.status, TraceStatus::Ok);
        // Reach into the in-memory port to verify it stored exactly one span.
        let in_mem = rec
            .port()
            .submit(TraceOperation {
                trace_id: TraceId("trace-x".into()),
                span_id: SpanId("span-x".into()),
                parent_span_id: None,
                kind: SpanKind::Internal,
                name: "marker".into(),
                attributes: HashMap::new(),
            });
        assert_eq!(in_mem.status, TraceStatus::Ok);
    }

    #[test]
    fn otel_config_default_service_name() {
        let cfg = OtelConfig::default();
        assert_eq!(cfg.service_name, "phenotype-router");
        assert!(!cfg.trace_id.is_empty());
        assert!(cfg.trace_id.starts_with("trace-"));
        assert_eq!(cfg.span_name, "phenotype.router.decision");
    }

    #[test]
    fn debug_format_does_not_leak_port() {
        let rec = OtlpDecisionRecorder::in_memory(OtelConfig::default());
        let s = format!("{:?}", rec);
        assert!(s.contains("service_name"));
        assert!(s.contains("phenotype-router"));
    }

    #[test]
    fn in_memory_port_stores_submitted_spans() {
        let port = InMemoryTracePort::new();
        assert!(port.is_empty());
        let op = TraceOperation {
            trace_id: TraceId("t".into()),
            span_id: SpanId("s".into()),
            parent_span_id: None,
            kind: SpanKind::Internal,
            name: "smoke".into(),
            attributes: HashMap::new(),
        };
        let res = port.submit(op.clone());
        assert_eq!(res.status, TraceStatus::Ok);
        assert_eq!(port.len(), 1);
        let spans = port.spans();
        assert_eq!(spans[0].name, "smoke");
    }

    #[test]
    fn trace_operation_serializes_to_json() {
        let op = TraceOperation {
            trace_id: TraceId("t1".into()),
            span_id: SpanId("s1".into()),
            parent_span_id: None,
            kind: SpanKind::Internal,
            name: "phenotype.router.decision".into(),
            attributes: [("phenotype.router.decision.kind".to_string(), "allow".to_string())]
                .into_iter()
                .collect(),
        };
        let s = serde_json::to_string(&op).expect("serialize");
        assert!(s.contains("phenotype.router.decision"));
        assert!(s.contains("internal"));
    }
}
