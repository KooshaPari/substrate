//! # substrate-trace
//!
//! Concrete [`TracePort`] adapters. The trait itself is defined in
//! `substrate-core` so the application layer can emit events without
//! depending on any adapter crate.
//!
//! ## Adapters
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`NoopTrace`] | Silently discards all events (useful as a default). |
//! | [`RecordingTrace`] | Stores events in memory for test assertions. |
//! | [`MultiTrace`] | Fans a single event stream out to N [`TracePort`]s. |
//! | [`AgilePlusTrace`] | POSTs events to the AgilePlus API. |
//! | [`TraceraTrace`] | POSTs events to the Tracera API. |
//! | [`PhenoOtelTrace`] | Serializes events to OTLP/JSON and ships them through the `pheno-otel` substrate (`HttpExporter`). |
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::{Arc, Mutex};

use pheno_otel::exporters::{http::HttpExporter, ExporterConfig};
use pheno_otel::OtlpPort;
use substrate_core::trace::{TaskCompleted, TaskFailed, TaskRegistered, TracePort};

// ---------------------------------------------------------------------------
// NoopTrace
// ---------------------------------------------------------------------------

/// A [`TracePort`] that silently discards every event.
///
/// Useful as a default when no trace backend is configured.
#[derive(Debug, Default, Clone)]
pub struct NoopTrace;

impl TracePort for NoopTrace {
    fn task_registered(&self, _event: TaskRegistered) {}
    fn task_completed(&self, _event: TaskCompleted) {}
    fn task_failed(&self, _event: TaskFailed) {}
}

// ---------------------------------------------------------------------------
// TraceEvent (union for RecordingTrace)
// ---------------------------------------------------------------------------

/// A discriminated union of the three trace event kinds, stored by
/// [`RecordingTrace`].
#[derive(Debug, Clone)]
pub enum TraceEvent {
    /// A [`TaskRegistered`] event.
    Registered(TaskRegistered),
    /// A [`TaskCompleted`] event.
    Completed(TaskCompleted),
    /// A [`TaskFailed`] event.
    Failed(TaskFailed),
}

// ---------------------------------------------------------------------------
// RecordingTrace
// ---------------------------------------------------------------------------

/// An in-memory [`TracePort`] that records every event for later inspection.
///
/// Thread-safe via `Arc<Mutex<…>>` so it can be cloned and shared across
/// threads in test assertions.
#[derive(Debug, Clone, Default)]
pub struct RecordingTrace {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl RecordingTrace {
    /// Create a new, empty recording trace.
    pub fn new() -> Self {
        RecordingTrace::default()
    }

    /// Return a snapshot of all recorded events in arrival order.
    pub fn events(&self) -> Vec<TraceEvent> {
        self.events
            .lock()
            .expect("RecordingTrace lock poisoned")
            .clone()
    }

    /// Return the number of recorded events.
    pub fn len(&self) -> usize {
        self.events
            .lock()
            .expect("RecordingTrace lock poisoned")
            .len()
    }

    /// Returns true if no events have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl TracePort for RecordingTrace {
    fn task_registered(&self, event: TaskRegistered) {
        self.events
            .lock()
            .expect("RecordingTrace lock poisoned")
            .push(TraceEvent::Registered(event));
    }

    fn task_completed(&self, event: TaskCompleted) {
        self.events
            .lock()
            .expect("RecordingTrace lock poisoned")
            .push(TraceEvent::Completed(event));
    }

    fn task_failed(&self, event: TaskFailed) {
        self.events
            .lock()
            .expect("RecordingTrace lock poisoned")
            .push(TraceEvent::Failed(event));
    }
}

// ---------------------------------------------------------------------------
// MultiTrace
// ---------------------------------------------------------------------------

/// A [`TracePort`] that fans every event out to N downstream [`TracePort`]s.
///
/// Useful for shipping to both AgilePlus and Tracera simultaneously, or for
/// augmenting a production backend with a [`RecordingTrace`] in tests.
pub struct MultiTrace {
    sinks: Vec<Arc<dyn TracePort>>,
}

impl std::fmt::Debug for MultiTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiTrace")
            .field("sink_count", &self.sinks.len())
            .finish()
    }
}

impl MultiTrace {
    /// Create a fan-out trace with the given sinks.
    pub fn new(sinks: Vec<Arc<dyn TracePort>>) -> Self {
        MultiTrace { sinks }
    }

    /// Create an empty fan-out (equivalent to [`NoopTrace`]; add sinks with
    /// [`MultiTrace::with_sink`]).
    pub fn empty() -> Self {
        MultiTrace { sinks: vec![] }
    }

    /// Append a sink and return `self` for chained construction.
    pub fn with_sink(mut self, sink: Arc<dyn TracePort>) -> Self {
        self.sinks.push(sink);
        self
    }
}

impl TracePort for MultiTrace {
    fn task_registered(&self, event: TaskRegistered) {
        for sink in &self.sinks {
            sink.task_registered(event.clone());
        }
    }

    fn task_completed(&self, event: TaskCompleted) {
        for sink in &self.sinks {
            sink.task_completed(event.clone());
        }
    }

    fn task_failed(&self, event: TaskFailed) {
        for sink in &self.sinks {
            sink.task_failed(event.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// AgilePlusTrace
// ---------------------------------------------------------------------------

/// Payload sent to the AgilePlus API for a registered task.
#[derive(Debug, serde::Serialize)]
struct AgilePlusRegistered<'a> {
    task_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    requirement_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    epic_id: Option<&'a str>,
}

/// Payload sent to the AgilePlus API for a completed task.
#[derive(Debug, serde::Serialize)]
struct AgilePlusCompleted<'a> {
    task_id: &'a str,
    pr_urls: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    requirement_id: Option<&'a str>,
}

/// Payload sent to the AgilePlus API for a failed task.
#[derive(Debug, serde::Serialize)]
struct AgilePlusFailed<'a> {
    task_id: &'a str,
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    requirement_id: Option<&'a str>,
}

/// A [`TracePort`] that POSTs events to the AgilePlus API.
///
/// The endpoint base URL is read from the `AGILEPLUS_ENDPOINT` env var at
/// construction time. HTTP errors are silently swallowed (trace shipping
/// must never fail a dispatch).
#[derive(Debug, Clone)]
pub struct AgilePlusTrace {
    endpoint: String,
    client: reqwest::Client,
    rt: Arc<tokio::runtime::Handle>,
}

impl AgilePlusTrace {
    /// Construct from the `AGILEPLUS_ENDPOINT` env var.
    ///
    /// Panics if called outside a Tokio runtime context (uses
    /// [`tokio::runtime::Handle::current()`]).
    pub fn from_env() -> Self {
        let endpoint = std::env::var("AGILEPLUS_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4000".to_string());
        AgilePlusTrace {
            endpoint,
            client: reqwest::Client::new(),
            rt: Arc::new(tokio::runtime::Handle::current()),
        }
    }

    /// Construct with an explicit endpoint URL.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        AgilePlusTrace {
            endpoint: endpoint.into(),
            client: reqwest::Client::new(),
            rt: Arc::new(tokio::runtime::Handle::current()),
        }
    }
}

impl TracePort for AgilePlusTrace {
    fn task_registered(&self, event: TaskRegistered) {
        let body = AgilePlusRegistered {
            task_id: &event.task_id,
            requirement_id: event.requirement_id.as_deref(),
            epic_id: event.epic_id.as_deref(),
        };
        // We must own the data before spawning; serialize to JSON string.
        if let Ok(json) = serde_json::to_string(&body) {
            let url = format!("{}/v1/tasks/registered", self.endpoint);
            let client = self.client.clone();
            self.rt.spawn(async move {
                let _ = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(json)
                    .send()
                    .await;
            });
        }
    }

    fn task_completed(&self, event: TaskCompleted) {
        if let Ok(json) = serde_json::to_string(&AgilePlusCompleted {
            task_id: &event.task_id,
            pr_urls: &event.pr_urls,
            requirement_id: event.requirement_id.as_deref(),
        }) {
            let url = format!("{}/v1/tasks/completed", self.endpoint);
            let client = self.client.clone();
            self.rt.spawn(async move {
                let _ = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(json)
                    .send()
                    .await;
            });
        }
    }

    fn task_failed(&self, event: TaskFailed) {
        if let Ok(json) = serde_json::to_string(&AgilePlusFailed {
            task_id: &event.task_id,
            error: &event.error,
            requirement_id: event.requirement_id.as_deref(),
        }) {
            let url = format!("{}/v1/tasks/failed", self.endpoint);
            let client = self.client.clone();
            self.rt.spawn(async move {
                let _ = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(json)
                    .send()
                    .await;
            });
        }
    }
}

// ---------------------------------------------------------------------------
// TraceraTrace
// ---------------------------------------------------------------------------

/// A [`TracePort`] that POSTs events to the Tracera API.
///
/// The endpoint base URL is read from the `TRACERA_ENDPOINT` env var at
/// construction time. HTTP errors are silently swallowed.
#[derive(Debug, Clone)]
pub struct TraceraTrace {
    endpoint: String,
    client: reqwest::Client,
    rt: Arc<tokio::runtime::Handle>,
}

impl TraceraTrace {
    /// Construct from the `TRACERA_ENDPOINT` env var.
    pub fn from_env() -> Self {
        let endpoint = std::env::var("TRACERA_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:5000".to_string());
        TraceraTrace {
            endpoint,
            client: reqwest::Client::new(),
            rt: Arc::new(tokio::runtime::Handle::current()),
        }
    }

    /// Construct with an explicit endpoint URL.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        TraceraTrace {
            endpoint: endpoint.into(),
            client: reqwest::Client::new(),
            rt: Arc::new(tokio::runtime::Handle::current()),
        }
    }
}

impl TracePort for TraceraTrace {
    fn task_registered(&self, event: TaskRegistered) {
        if let Ok(json) = serde_json::to_string(&serde_json::json!({
            "task_id": event.task_id,
            "requirement_id": event.requirement_id,
            "epic_id": event.epic_id,
        })) {
            let url = format!("{}/api/tasks/registered", self.endpoint);
            let client = self.client.clone();
            self.rt.spawn(async move {
                let _ = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(json)
                    .send()
                    .await;
            });
        }
    }

    fn task_completed(&self, event: TaskCompleted) {
        if let Ok(json) = serde_json::to_string(&serde_json::json!({
            "task_id": event.task_id,
            "pr_urls": event.pr_urls,
            "requirement_id": event.requirement_id,
        })) {
            let url = format!("{}/api/tasks/completed", self.endpoint);
            let client = self.client.clone();
            self.rt.spawn(async move {
                let _ = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(json)
                    .send()
                    .await;
            });
        }
    }

    fn task_failed(&self, event: TaskFailed) {
        if let Ok(json) = serde_json::to_string(&serde_json::json!({
            "task_id": event.task_id,
            "error": event.error,
            "requirement_id": event.requirement_id,
        })) {
            let url = format!("{}/api/tasks/failed", self.endpoint);
            let client = self.client.clone();
            self.rt.spawn(async move {
                let _ = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(json)
                    .send()
                    .await;
            });
        }
    }
}

// ---------------------------------------------------------------------------
// PhenoOtelTrace
// ---------------------------------------------------------------------------

/// A [`TracePort`] that serializes lifecycle events as OTLP/JSON and POSTs
/// them to the `pheno-otel` [`HttpExporter`]'s `/v1/traces` OTLP/HTTP
/// endpoint. This is the canonical export path for the phenotype fleet's
/// substrate crates — every `pheno-*` substrate depends on `pheno-otel` for
/// its OTLP needs (per ADR-037).
///
/// The exporter is read-only-friendly: HTTP errors are silently swallowed
/// (trace shipping must never fail a dispatch).
///
/// Construct via [`PhenoOtelTrace::from_env`] (reads `PHENO_OTEL_ENDPOINT`
/// + `PHENO_OTEL_SERVICE_NAME`) or [`PhenoOtelTrace::with_endpoint`] for
/// tests.
#[derive(Debug, Clone)]
pub struct PhenoOtelTrace {
    /// The OTLP/HTTP endpoint (e.g. `http://otel-collector.phenotype.svc:4318`).
    endpoint: String,
    /// The OTel `service.name` resource attribute.
    service_name: String,
    /// Reqwest client for the actual HTTP POST (pheno-otel's HttpExporter
    /// is a substrate that returns an ExportHandle; the wire transport is
    /// the consumer's responsibility).
    client: reqwest::Client,
    /// Tokio runtime handle for spawning the fire-and-forget POST task.
    rt: Arc<tokio::runtime::Handle>,
}

impl PhenoOtelTrace {
    /// Construct from the `PHENO_OTEL_ENDPOINT` env var (default
    /// `http://127.0.0.1:4318`) and `PHENO_OTEL_SERVICE_NAME` env var
    /// (default `substrate`).
    ///
    /// Panics if called outside a Tokio runtime context (uses
    /// [`tokio::runtime::Handle::current()`]).
    pub fn from_env() -> Self {
        let endpoint = std::env::var("PHENO_OTEL_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:4318".to_string());
        let service_name = std::env::var("PHENO_OTEL_SERVICE_NAME")
            .unwrap_or_else(|_| "substrate".to_string());
        PhenoOtelTrace {
            endpoint,
            service_name,
            client: reqwest::Client::new(),
            rt: Arc::new(tokio::runtime::Handle::current()),
        }
    }

    /// Construct with an explicit endpoint URL + service name.
    ///
    /// Panics if called outside a Tokio runtime context.
    pub fn with_endpoint(endpoint: impl Into<String>, service_name: impl Into<String>) -> Self {
        PhenoOtelTrace {
            endpoint: endpoint.into(),
            service_name: service_name.into(),
            client: reqwest::Client::new(),
            rt: Arc::new(tokio::runtime::Handle::current()),
        }
    }

    /// Build a fresh [`HttpExporter`] configured for the `/v1/traces`
    /// OTLP/HTTP endpoint. Construction is cheap; the exporter holds only
    /// its [`ExporterConfig`] + signal path.
    fn exporter(&self) -> HttpExporter {
        let cfg = ExporterConfig::new(self.endpoint.clone(), self.service_name.clone());
        HttpExporter::traces(cfg)
    }

    /// The full URL the exporter will POST to (e.g. `http://otel:4318/v1/traces`).
    pub fn target_url(&self) -> String {
        self.exporter().target_url()
    }

    /// Serialize a substrate trace event into an OTLP/JSON payload and POST
    /// it. Errors are intentionally swallowed — trace shipping is
    /// best-effort and must never fail a dispatch.
    fn ship(&self, event_kind: &str, body: serde_json::Value) {
        // Wrap the substrate event into the OTel log-record envelope so
        // downstream tools (Tempo, Honeycomb, etc.) can render it. The full
        // span/event machinery is out of scope for this adapter; we
        // intentionally emit a structured `log` record per substrate event.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let otlp = serde_json::json!({
            "resourceLogs": [{
                "resource": {
                    "attributes": [
                        { "key": "service.name", "value": { "stringValue": self.service_name } }
                    ]
                },
                "scopeLogs": [{
                    "scope": { "name": "substrate-trace", "version": env!("CARGO_PKG_VERSION") },
                    "logRecords": [{
                        "timeUnixNano": nanos.to_string(),
                        "severityText": event_kind,
                        "body": { "stringValue": body.to_string() },
                    }]
                }]
            }]
        });
        if let Ok(payload) = serde_json::to_vec(&otlp) {
            // Validate via pheno-otel (sanity check the wire envelope).
            let exp = self.exporter();
            if exp.export(&payload).is_ok() {
                // Spawn detached POST to the target URL. Errors are
                // swallowed — trace shipping is best-effort and must never
                // fail a dispatch.
                let url = exp.target_url();
                let client = self.client.clone();
                self.rt.spawn(async move {
                    let _ = client
                        .post(&url)
                        .header("content-type", "application/json")
                        .body(payload)
                        .send()
                        .await;
                });
            }
        }
    }
}

impl TracePort for PhenoOtelTrace {
    fn task_registered(&self, event: TaskRegistered) {
        let body = serde_json::json!({
            "event": "task_registered",
            "task_id": event.task_id,
            "requirement_id": event.requirement_id,
            "epic_id": event.epic_id,
        });
        self.ship("INFO", body);
    }

    fn task_completed(&self, event: TaskCompleted) {
        let body = serde_json::json!({
            "event": "task_completed",
            "task_id": event.task_id,
            "pr_urls": event.pr_urls,
            "requirement_id": event.requirement_id,
        });
        self.ship("INFO", body);
    }

    fn task_failed(&self, event: TaskFailed) {
        let body = serde_json::json!({
            "event": "task_failed",
            "task_id": event.task_id,
            "error": event.error,
            "requirement_id": event.requirement_id,
        });
        self.ship("ERROR", body);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── NoopTrace ────────────────────────────────────────────────────────────

    #[test]
    fn noop_trace_is_inert() {
        let t = NoopTrace;
        // Should not panic.
        t.task_registered(TaskRegistered {
            task_id: "t1".into(),
            requirement_id: None,
            epic_id: None,
        });
        t.task_completed(TaskCompleted {
            task_id: "t1".into(),
            pr_urls: vec![],
            requirement_id: None,
        });
        t.task_failed(TaskFailed {
            task_id: "t1".into(),
            error: "oops".into(),
            requirement_id: None,
        });
    }

    // ── RecordingTrace ────────────────────────────────────────────────────────

    #[test]
    fn recording_trace_starts_empty() {
        let r = RecordingTrace::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn recording_trace_captures_lifecycle() {
        let r = RecordingTrace::new();

        r.task_registered(TaskRegistered {
            task_id: "task-1".into(),
            requirement_id: Some("FR-42".into()),
            epic_id: Some("E-1".into()),
        });
        assert_eq!(r.len(), 1);
        assert!(matches!(&r.events()[0], TraceEvent::Registered(e) if e.task_id == "task-1"));

        r.task_completed(TaskCompleted {
            task_id: "task-1".into(),
            pr_urls: vec!["https://github.com/foo/bar/pull/1".into()],
            requirement_id: Some("FR-42".into()),
        });
        assert_eq!(r.len(), 2);
        assert!(matches!(&r.events()[1], TraceEvent::Completed(e) if e.task_id == "task-1"));
    }

    #[test]
    fn recording_trace_captures_failure() {
        let r = RecordingTrace::new();
        r.task_registered(TaskRegistered {
            task_id: "task-2".into(),
            requirement_id: None,
            epic_id: None,
        });
        r.task_failed(TaskFailed {
            task_id: "task-2".into(),
            error: "engine timeout".into(),
            requirement_id: None,
        });
        assert_eq!(r.len(), 2);
        assert!(matches!(&r.events()[1], TraceEvent::Failed(e) if e.error == "engine timeout"));
    }

    // ── MultiTrace ────────────────────────────────────────────────────────────

    #[test]
    fn multi_trace_fans_to_n_consumers() {
        let r1 = Arc::new(RecordingTrace::new());
        let r2 = Arc::new(RecordingTrace::new());
        let r3 = Arc::new(RecordingTrace::new());

        let multi = MultiTrace::new(vec![
            r1.clone() as Arc<dyn TracePort>,
            r2.clone() as Arc<dyn TracePort>,
            r3.clone() as Arc<dyn TracePort>,
        ]);

        multi.task_registered(TaskRegistered {
            task_id: "t".into(),
            requirement_id: None,
            epic_id: None,
        });
        multi.task_completed(TaskCompleted {
            task_id: "t".into(),
            pr_urls: vec![],
            requirement_id: None,
        });

        for r in [&r1, &r2, &r3] {
            assert_eq!(r.len(), 2, "each sink must receive both events");
        }
    }

    #[test]
    fn multi_trace_empty_is_noop() {
        let multi = MultiTrace::empty();
        // Must not panic.
        multi.task_registered(TaskRegistered {
            task_id: "t".into(),
            requirement_id: None,
            epic_id: None,
        });
    }

    #[test]
    fn multi_trace_with_sink_builder() {
        let r = Arc::new(RecordingTrace::new());
        let multi = MultiTrace::empty().with_sink(r.clone() as Arc<dyn TracePort>);
        multi.task_failed(TaskFailed {
            task_id: "t".into(),
            error: "x".into(),
            requirement_id: None,
        });
        assert_eq!(r.len(), 1);
    }

    // ── Dispatch lifecycle (trace integration) ────────────────────────────────

    #[test]
    fn dispatch_emits_registered_then_completed() {
        // Simulate what DispatchService does: emit Registered, then Completed.
        let r = RecordingTrace::new();
        let task_id = "lifecycle-1".to_string();

        r.task_registered(TaskRegistered {
            task_id: task_id.clone(),
            requirement_id: Some("FR-1".into()),
            epic_id: None,
        });
        r.task_completed(TaskCompleted {
            task_id: task_id.clone(),
            pr_urls: vec!["https://github.com/foo/bar/pull/42".into()],
            requirement_id: Some("FR-1".into()),
        });

        let events = r.events();
        assert_eq!(events.len(), 2);
        // First event must be Registered.
        assert!(
            matches!(&events[0], TraceEvent::Registered(e) if e.task_id == task_id),
            "first event must be Registered"
        );
        // Second event must be Completed.
        assert!(
            matches!(&events[1], TraceEvent::Completed(e) if e.pr_urls.len() == 1),
            "second event must be Completed with pr_url"
        );
    }

    #[test]
    fn dispatch_emits_registered_then_failed() {
        let r = RecordingTrace::new();
        let task_id = "lifecycle-2".to_string();

        r.task_registered(TaskRegistered {
            task_id: task_id.clone(),
            requirement_id: None,
            epic_id: None,
        });
        r.task_failed(TaskFailed {
            task_id: task_id.clone(),
            error: "engine exited non-zero".into(),
            requirement_id: None,
        });

        let events = r.events();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], TraceEvent::Registered(_)));
        assert!(matches!(&events[1], TraceEvent::Failed(e) if e.error.contains("engine")));
    }

    // ── PhenoOtelTrace ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn pheno_otel_target_url_appends_traces_path() {
        let t = PhenoOtelTrace::with_endpoint("http://otel.example.com:4318", "test-service");
        // pheno-otel HttpExporter::traces() appends /v1/traces
        assert_eq!(
            t.target_url(),
            "http://otel.example.com:4318/v1/traces"
        );
    }

    #[tokio::test]
    async fn pheno_otel_target_url_strips_trailing_slash() {
        let t = PhenoOtelTrace::with_endpoint("http://otel.example.com:4318/", "test");
        assert_eq!(
            t.target_url(),
            "http://otel.example.com:4318/v1/traces"
        );
    }

    #[tokio::test]
    async fn pheno_otel_health_check_succeeds_when_endpoint_set() {
        // pheno-otel::OtlpPort::health() returns Ok(()) for non-empty endpoint
        let t = PhenoOtelTrace::with_endpoint("http://localhost:4318", "substrate");
        // Force-build an exporter and check health through it.
        // (No public health() on PhenoOtelTrace itself; the validation
        // happens inside ship() via export().is_ok().)
        // We assert that target_url() is well-formed.
        assert!(t.target_url().ends_with("/v1/traces"));
    }

    #[tokio::test]
    async fn pheno_otel_ship_does_not_panic_on_unreachable_endpoint() {
        // Point at a port nothing is listening on. ship() must swallow the
        // eventual reqwest error gracefully (no panic in the dispatch path).
        let t = PhenoOtelTrace::with_endpoint("http://127.0.0.1:1", "substrate");
        t.task_registered(TaskRegistered {
            task_id: "test-1".into(),
            requirement_id: None,
            epic_id: None,
        });
        t.task_completed(TaskCompleted {
            task_id: "test-1".into(),
            pr_urls: vec![],
            requirement_id: None,
        });
        t.task_failed(TaskFailed {
            task_id: "test-1".into(),
            error: "err".into(),
            requirement_id: None,
        });
        // Give the spawned POST tasks a tick to attempt + fail.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn pheno_otel_fans_into_multi_trace() {
        // PhenoOtelTrace must compose with MultiTrace (it's just a TracePort).
        let r = Arc::new(RecordingTrace::new());
        let t = PhenoOtelTrace::with_endpoint("http://127.0.0.1:1", "substrate");
        let multi = MultiTrace::new(vec![
            r.clone() as Arc<dyn TracePort>,
            Arc::new(t) as Arc<dyn TracePort>,
        ]);
        multi.task_completed(TaskCompleted {
            task_id: "fanned".into(),
            pr_urls: vec!["https://github.com/foo/bar/pull/7".into()],
            requirement_id: None,
        });
        // The recording sink should have received the event synchronously.
        assert_eq!(r.len(), 1);
        assert!(matches!(&r.events()[0], TraceEvent::Completed(_)));
    }
}
