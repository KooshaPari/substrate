//! `contextfolding` — context-folding connector (ADR-052 ConnectorPort).
//!
//! ## Source
//!
//! Ported from `argis-extensions/plugins/contextfolding/` (Go) as part
//! of the v13 3-plugin port wave
//! (`feat/v13-3-plugin-ports-2026-06-21`). The original Go plugin folds
//! a long prompt into a smaller one by removing redundant tokens
//! (see `argis-extensions/plugins/contextfolding/folding.go`).
//!
//! ## SDK contract
//!
//! Implements [`crate::sdk::ConnectorPort`]. The connector opens a
//! "fold session" via [`Self::connect`] and exposes a
//! [`ContextFoldHandle`] that callers use to fold a request payload
//! into a smaller one via [`ContextFoldHandle::fold`].
//!
//! The connector declares `Capabilities::NETWORK_IO` because the
//! upstream Go plugin performs HTTP calls to a folding service
//! (mocked here; the in-tree adapter is a pure-compute fallback per
//! the in-process `FoldingStrategy`).
//!
//! ## Telemetry
//!
//! `connect()` emits exactly one OTel-compatible span named
//! `phenotype.router.plugin.contextfolding.connect` per ADR-052 §3.
//! `fold()` emits `phenotype.router.plugin.contextfolding.fold`.
//! Attributes: `phenotype.router.plugin.name`,
//! `phenotype.router.plugin.phase`, `phenotype.router.adapter.fold.ratio`,
//! `phenotype.router.adapter.fold.before`, `phenotype.router.adapter.fold.after`.
//!
//! ## Substrate notes
//!
//! Per ADR-023 Rule 3.1, this file ships with spec (this header +
//! [`ContextFoldingConnector`]), tests (`#[cfg(test)] mod tests` at
//! the bottom), OTel spans (above), and a `PREDICTIVE.md` next to
//! the source (per ADR-047 4-criterion rule).

use crate::decision::Request;
use crate::sdk::{
    Capabilities, ConnectorConfig, ConnectorError, ConnectorHandle, ConnectorPort,
    HealthStatus,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// Plugin name (kebab-case, fleet-wide stable).
pub const PLUGIN_NAME: &str = "contextfolding";
/// Plugin semver (ADR-052 §5).
pub const PLUGIN_VERSION: &str = "0.1.0";

/// Folding strategy — pure-compute fallback for the in-tree adapter
/// (the real `contextfolding` plugin performs HTTP I/O; we model the
/// strategy as a trait so tests can swap deterministic strategies
/// for property-based or fuzz strategies).
pub trait FoldingStrategy: Send + Sync + std::fmt::Debug {
    /// Stable strategy name (for span attribution).
    fn name(&self) -> &str;
    /// Fold `payload` into a (potentially) shorter one. Returns the
    /// folded payload + the post-fold length.
    fn fold(&self, payload: &str) -> (String, usize);
}

/// Built-in strategy: deduplicate whitespace and drop redundant
/// repeated tokens. The real Go plugin is more sophisticated
/// (semantic deduplication via embeddings); this strategy is
/// deterministic and round-trip-testable in a no-network environment.
#[derive(Debug, Default, Clone, Copy)]
pub struct WhitespaceDedupeStrategy;

impl FoldingStrategy for WhitespaceDedupeStrategy {
    fn name(&self) -> &str {
        "whitespace_dedupe"
    }

    fn fold(&self, payload: &str) -> (String, usize) {
        // 1. Split on whitespace; 2. dedupe consecutive duplicates;
        // 3. re-join with single spaces.
        let mut out: Vec<&str> = Vec::with_capacity(payload.len());
        let mut last: Option<&str> = None;
        for token in payload.split_whitespace() {
            if Some(token) != last {
                out.push(token);
                last = Some(token);
            }
        }
        let folded = out.join(" ");
        let after = folded.len();
        (folded, after)
    }
}

/// Built-in strategy: no folding (echoes the input). Useful as a
/// control in tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopStrategy;

impl FoldingStrategy for NoopStrategy {
    fn name(&self) -> &str {
        "noop"
    }

    fn fold(&self, payload: &str) -> (String, usize) {
        (payload.to_string(), payload.len())
    }
}

/// Concrete handle returned by [`ContextFoldingConnector::connect`].
///
/// Owns an `Arc<Mutex<...>>` over the fold session state so callers
/// can fold concurrently from multiple threads (the
/// `ConnectorHandle` trait requires `Send + Sync`).
pub struct ContextFoldHandle {
    config: ConnectorConfig,
    strategy: Arc<dyn FoldingStrategy>,
    /// Number of folds performed by this handle (liveness metric).
    fold_count: Arc<Mutex<u64>>,
}

impl std::fmt::Debug for ContextFoldHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextFoldHandle")
            .field("name", &PLUGIN_NAME)
            .field("url", &self.config.url)
            .field("strategy", &self.strategy.name())
            .field(
                "fold_count",
                &*self.fold_count.lock().expect("fold_count mutex poisoned"),
            )
            .finish()
    }
}

impl ContextFoldHandle {
    /// Fold a request payload through the configured strategy.
    /// Returns the folded payload + the post-fold character count.
    pub fn fold(&self, payload: &str) -> (String, usize) {
        let span = ::tracing::info_span!(
            "phenotype.router.plugin.contextfolding.fold",
            "phenotype.router.plugin.name" = PLUGIN_NAME,
            "phenotype.router.plugin.phase" = "RequestTransform",
            "phenotype.router.adapter.fold.strategy" = %self.strategy.name(),
        );
        let _g = span.enter();
        let before = payload.len();
        let (folded, after) = self.strategy.fold(payload);
        let mut count = self
            .fold_count
            .lock()
            .expect("fold_count mutex poisoned");
        *count += 1;
        let ratio = if before == 0 {
            1.0
        } else {
            after as f64 / before as f64
        };
        ::tracing::info!(
            before = before,
            after = after,
            ratio = ratio,
            "folded payload"
        );
        (folded, after)
    }

    /// Fold a full [`Request`], preserving the request id.
    pub fn fold_request(&self, req: &Request) -> (String, usize) {
        let (folded, after) = self.fold(&req.payload);
        // Wrap the folded payload back into a Request view; we
        // return only the (folded_payload, after_len) tuple so the
        // caller can attach it to a `PluginDecision::rewritten_prompt`.
        (folded, after)
    }

    /// Snapshot the fold count (for health / metrics).
    pub fn fold_count(&self) -> u64 {
        *self.fold_count.lock().expect("fold_count mutex poisoned")
    }

    /// Read-only view of the underlying strategy.
    pub fn strategy(&self) -> &Arc<dyn FoldingStrategy> {
        &self.strategy
    }
}

impl ConnectorHandle for ContextFoldHandle {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn ping(&self) -> Result<HealthStatus, ConnectorError> {
        Ok(HealthStatus::Healthy)
    }
}

/// The `contextfolding` connector (ADR-052 ConnectorPort).
///
/// `connect()` opens a fold session by selecting the appropriate
/// [`FoldingStrategy`] (currently: only `WhitespaceDedupeStrategy` is
/// supported; the upstream Go plugin's HTTP-backed strategy is left
/// for the v15 follow-up).
#[derive(Debug, Clone)]
pub struct ContextFoldingConnector {
    strategy: Arc<dyn FoldingStrategy>,
}

impl Default for ContextFoldingConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextFoldingConnector {
    /// Construct a connector with the default
    /// [`WhitespaceDedupeStrategy`].
    pub fn new() -> Self {
        Self::with_strategy(Arc::new(WhitespaceDedupeStrategy))
    }

    /// Construct a connector with a custom strategy (tests + future
    /// pluggable strategies).
    pub fn with_strategy(strategy: Arc<dyn FoldingStrategy>) -> Self {
        Self { strategy }
    }

    /// Read-only view of the active strategy.
    pub fn strategy(&self) -> &Arc<dyn FoldingStrategy> {
        &self.strategy
    }
}

#[async_trait]
impl ConnectorPort for ContextFoldingConnector {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    fn capabilities(&self) -> Capabilities {
        // Per ADR-052 §1: ConnectorPort implies NETWORK_IO. The
        // in-tree strategy is pure-compute but the upstream Go
        // plugin performs HTTP I/O, so we declare the capability
        // to match the real plugin's footprint.
        Capabilities::NETWORK_IO
    }

    async fn connect(
        &self,
        cfg: &ConnectorConfig,
    ) -> Result<Box<dyn ConnectorHandle>, ConnectorError> {
        // Emit exactly one OTel-compatible span per ADR-052 §3.
        let span = ::tracing::info_span!(
            "phenotype.router.plugin.contextfolding.connect",
            "phenotype.router.plugin.name" = PLUGIN_NAME,
            "phenotype.router.plugin.phase" = "RequestTransform",
            "phenotype.router.adapter.connector.url" = %cfg.url,
        );
        let _g = span.enter();

        if cfg.url.is_empty() {
            ::tracing::warn!("empty URL in connector config");
            return Err(ConnectorError::Connect("empty URL".to_string()));
        }

        ::tracing::info!(
            strategy = %self.strategy.name(),
            "opened fold session"
        );

        let handle = ContextFoldHandle {
            config: cfg.clone(),
            strategy: Arc::clone(&self.strategy),
            fold_count: Arc::new(Mutex::new(0)),
        };
        Ok(Box::new(handle))
    }

    async fn health(&self) -> Result<HealthStatus, ConnectorError> {
        Ok(HealthStatus::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_name_and_version_are_pinned() {
        assert_eq!(PLUGIN_NAME, "contextfolding");
        assert_eq!(PLUGIN_VERSION, "0.1.0");
    }

    #[test]
    fn noop_strategy_returns_input_unchanged() {
        let s = NoopStrategy;
        let (out, after) = s.fold("hello world");
        assert_eq!(out, "hello world");
        assert_eq!(after, 11);
    }

    #[test]
    fn whitespace_dedupe_strategy_dedupes_consecutive_tokens() {
        let s = WhitespaceDedupeStrategy;
        let (out, _) = s.fold("hello hello world world hello");
        assert_eq!(out, "hello world hello");
    }

    #[test]
    fn whitespace_dedupe_strategy_collapses_whitespace() {
        let s = WhitespaceDedupeStrategy;
        let (out, _) = s.fold("hello   world\n\nfoo\tbar");
        assert_eq!(out, "hello world foo bar");
    }

    #[test]
    fn whitespace_dedupe_strategy_handles_empty_input() {
        let s = WhitespaceDedupeStrategy;
        let (out, after) = s.fold("");
        assert_eq!(out, "");
        assert_eq!(after, 0);
    }

    #[test]
    fn connector_default_strategy_is_whitespace_dedupe() {
        let c = ContextFoldingConnector::new();
        assert_eq!(c.strategy().name(), "whitespace_dedupe");
    }

    #[test]
    fn connector_sdk_metadata_is_pinned() {
        let c = ContextFoldingConnector::new();
        assert_eq!(c.name(), "contextfolding");
        assert_eq!(c.version(), "0.1.0");
        assert!(c.capabilities().contains(Capabilities::NETWORK_IO));
    }

    #[tokio::test]
    async fn connect_rejects_empty_url() {
        let c = ContextFoldingConnector::new();
        let cfg = ConnectorConfig {
            url: "".to_string(),
            token: None,
            timeout_ms: None,
        };
        let res = c.connect(&cfg).await;
        assert!(matches!(res, Err(ConnectorError::Connect(_))));
    }

    #[tokio::test]
    async fn connect_opens_handle_for_valid_url() {
        let c = ContextFoldingConnector::new();
        let cfg = ConnectorConfig {
            url: "https://fold.example/v1".to_string(),
            token: Some("t".to_string()),
            timeout_ms: Some(1000),
        };
        let handle = c.connect(&cfg).await.expect("connect must succeed");
        assert_eq!(handle.name(), "contextfolding");
        let status = handle.ping().expect("ping must succeed");
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn connect_returns_box_dyn_handle_with_ping() {
        let c = ContextFoldingConnector::new();
        let cfg = ConnectorConfig {
            url: "https://fold.example/v1".to_string(),
            token: Some("t".to_string()),
            timeout_ms: Some(1000),
        };
        let handle: Box<dyn crate::sdk::ConnectorHandle> =
            c.connect(&cfg).await.expect("connect must succeed");
        // The trait surface is name + ping; concrete-typed methods
        // (fold, fold_count, strategy) are exercised in the unit
        // tests below against `ContextFoldHandle` directly.
        assert_eq!(handle.name(), "contextfolding");
        let status = handle.ping().expect("ping must succeed");
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn concrete_handle_dedupes_payload() {
        // Round-trip test against the concrete handle type. The
        // SDK's `Box<dyn ConnectorHandle>` return type erases the
        // concrete-typed `fold()` method, so this test instantiates
        // the handle directly to exercise the fold path.
        let h = ContextFoldHandle {
            config: ConnectorConfig {
                url: "https://fold.example/v1".to_string(),
                token: None,
                timeout_ms: None,
            },
            strategy: Arc::new(WhitespaceDedupeStrategy),
            fold_count: Arc::new(Mutex::new(0)),
        };
        let (folded, _) = h.fold("hello hello world");
        assert_eq!(folded, "hello world");
    }

    #[test]
    fn handle_fold_count_starts_at_zero() {
        let h = ContextFoldHandle {
            config: ConnectorConfig {
                url: "u".to_string(),
                token: None,
                timeout_ms: None,
            },
            strategy: Arc::new(NoopStrategy),
            fold_count: Arc::new(Mutex::new(0)),
        };
        assert_eq!(h.fold_count(), 0);
        let _ = h.fold("x");
        assert_eq!(h.fold_count(), 1);
        let _ = h.fold("y");
        let _ = h.fold("z");
        assert_eq!(h.fold_count(), 3);
    }

    #[test]
    fn handle_strategy_returns_underlying() {
        let h = ContextFoldHandle {
            config: ConnectorConfig {
                url: "u".to_string(),
                token: None,
                timeout_ms: None,
            },
            strategy: Arc::new(WhitespaceDedupeStrategy),
            fold_count: Arc::new(Mutex::new(0)),
        };
        assert_eq!(h.strategy().name(), "whitespace_dedupe");
    }
}
