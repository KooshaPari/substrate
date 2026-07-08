//! In-memory request metrics for the gateway.
//!
//! Tracks total requests, errors, latency, and per-provider breakdowns using
//! lock-free [`AtomicU64`] globals for the aggregate path and a [`Mutex`]-guarded
//! [`HashMap`] for per-provider detail.
//!
//! Also exposes a [`prometheus`]-backed [`HistogramVec`] for latency observations
//! at `gateway_latency_ms`, keyed by upstream `provider` label.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use prometheus::{exponential_buckets, register_histogram_vec, HistogramOpts, HistogramVec};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Prometheus latency histogram
// ---------------------------------------------------------------------------

/// Global [`HistogramVec`] for per-provider request latency (milliseconds).
///
/// Initialized lazily on first access. Buckets follow an exponential progression
/// starting at 10ms with a factor of 2 across 10 buckets (10, 20, 40, …, 5120ms),
/// giving useful resolution across both LAN-fast and slow upstream paths.
static LATENCY_HISTOGRAM: OnceLock<HistogramVec> = OnceLock::new();

/// Returns the process-global [`HistogramVec`] for `gateway_latency_ms`.
pub fn latency_histogram() -> &'static HistogramVec {
    LATENCY_HISTOGRAM.get_or_init(|| {
        register_histogram_vec!(
            HistogramOpts::new("gateway_latency_ms", "Request latency in milliseconds")
                .buckets(exponential_buckets(10.0, 2.0, 10).unwrap()),
            &["provider"]
        )
        .unwrap()
    })
}

/// Record one latency observation for `provider` (in milliseconds).
pub fn record_latency(provider: &str, latency_ms: f64) {
    latency_histogram()
        .with_label_values(&[provider])
        .observe(latency_ms);
}

// ---------------------------------------------------------------------------
// Per-provider counters
// ---------------------------------------------------------------------------

/// Counters for a single upstream provider.
#[derive(Debug, Default, Clone)]
pub struct ProviderMetrics {
    pub request_count: u64,
    pub error_count: u64,
    pub total_latency_ms: u64,
}

impl ProviderMetrics {
    /// Average latency in milliseconds, or 0 when no requests have been recorded.
    pub fn avg_latency_ms(&self) -> u64 {
        self.total_latency_ms
            .checked_div(self.request_count)
            .unwrap_or(0)
    }
}

/// JSON-serialisable snapshot of per-provider counters.
#[derive(Debug, Serialize)]
pub struct ProviderMetricsSnapshot {
    pub requests: u64,
    pub errors: u64,
    pub avg_latency_ms: u64,
}

// ---------------------------------------------------------------------------
// Aggregate store
// ---------------------------------------------------------------------------

/// Thread-safe store for gateway request metrics.
///
/// Clone is cheap — all interior state is reference-counted.
#[derive(Debug, Clone, Default)]
pub struct MetricsStore {
    total_requests: Arc<AtomicU64>,
    total_errors: Arc<AtomicU64>,
    total_latency_ms: Arc<AtomicU64>,
    per_provider: Arc<Mutex<HashMap<String, ProviderMetrics>>>,
}

impl MetricsStore {
    /// Construct a zeroed metrics store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one completed request for `provider`.
    ///
    /// `error` should be `true` when the upstream returned a non-2xx response or
    /// the request failed before a response was received.
    pub fn record(&self, provider: &str, latency_ms: u64, error: bool) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
        if error {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }

        let mut map = self
            .per_provider
            .lock()
            .expect("per_provider lock poisoned");
        let entry = map.entry(provider.to_string()).or_default();
        entry.request_count += 1;
        entry.total_latency_ms += latency_ms;
        if error {
            entry.error_count += 1;
        }
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.total_errors.store(0, Ordering::Relaxed);
        self.total_latency_ms.store(0, Ordering::Relaxed);
        let mut map = self
            .per_provider
            .lock()
            .expect("per_provider lock poisoned");
        map.clear();
    }

    /// Snapshot the current aggregate counters.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let total_errors = self.total_errors.load(Ordering::Relaxed);
        let total_latency_ms = self.total_latency_ms.load(Ordering::Relaxed);

        let error_rate = if total_requests == 0 {
            0.0_f64
        } else {
            total_errors as f64 / total_requests as f64
        };

        let avg_latency_ms = total_latency_ms.checked_div(total_requests).unwrap_or(0);

        let map = self
            .per_provider
            .lock()
            .expect("per_provider lock poisoned");
        let per_provider = map
            .iter()
            .map(|(name, pm)| {
                (
                    name.clone(),
                    ProviderMetricsSnapshot {
                        requests: pm.request_count,
                        errors: pm.error_count,
                        avg_latency_ms: pm.avg_latency_ms(),
                    },
                )
            })
            .collect();

        MetricsSnapshot {
            total_requests,
            total_errors,
            error_rate,
            avg_latency_ms,
            per_provider,
            rate_limit_hits: HashMap::new(),
        }
    }
}

/// JSON-serialisable point-in-time snapshot of all metrics.
#[derive(Debug, Serialize)]
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub total_errors: u64,
    /// Fraction of requests that resulted in an error (`0.0`–`1.0`).
    pub error_rate: f64,
    pub avg_latency_ms: u64,
    pub per_provider: HashMap<String, ProviderMetricsSnapshot>,
    /// Cumulative HTTP 429 rate-limit hits per provider.
    pub rate_limit_hits: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// Prometheus text format rendering
// ---------------------------------------------------------------------------

/// Escape a Prometheus label value per the exposition format spec:
/// backslash → `\\`, double-quote → `\"`, newline → `\n`.
fn escape_label_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for ch in v.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str(r"\n"),
            c => out.push(c),
        }
    }
    out
}

impl MetricsStore {
    /// Render metrics in Prometheus text exposition format (version 0.0.4).
    ///
    /// `rate_limit_hits` should come from `RateLimiterStore::hits_snapshot()`.
    pub fn prometheus_text(&self, rate_limit_hits: &HashMap<String, u64>) -> String {
        let map = self
            .per_provider
            .lock()
            .expect("per_provider lock poisoned");

        let mut out = String::new();

        // substrate_requests_total
        out.push_str("# HELP substrate_requests_total Total requests handled\n");
        out.push_str("# TYPE substrate_requests_total counter\n");
        for (provider, pm) in map.iter() {
            let lv = escape_label_value(provider);
            out.push_str(&format!(
                "substrate_requests_total{{provider=\"{lv}\"}} {}\n",
                pm.request_count
            ));
        }

        // substrate_errors_total
        out.push_str("# HELP substrate_errors_total Total error responses\n");
        out.push_str("# TYPE substrate_errors_total counter\n");
        for (provider, pm) in map.iter() {
            let lv = escape_label_value(provider);
            out.push_str(&format!(
                "substrate_errors_total{{provider=\"{lv}\"}} {}\n",
                pm.error_count
            ));
        }

        // substrate_latency_ms (cumulative — gauge semantics for total)
        out.push_str("# HELP substrate_latency_ms Cumulative latency in milliseconds\n");
        out.push_str("# TYPE substrate_latency_ms gauge\n");
        for (provider, pm) in map.iter() {
            let lv = escape_label_value(provider);
            out.push_str(&format!(
                "substrate_latency_ms{{provider=\"{lv}\"}} {}\n",
                pm.total_latency_ms
            ));
        }

        // substrate_rate_limit_hits
        out.push_str("# HELP substrate_rate_limit_hits HTTP 429 rate-limit hits per provider\n");
        out.push_str("# TYPE substrate_rate_limit_hits counter\n");
        for (provider, hits) in rate_limit_hits.iter() {
            let lv = escape_label_value(provider);
            out.push_str(&format!(
                "substrate_rate_limit_hits{{provider=\"{lv}\"}} {hits}\n"
            ));
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_store_returns_zero_snapshot() {
        let store = MetricsStore::new();
        let snap = store.snapshot();
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.total_errors, 0);
        assert_eq!(snap.error_rate, 0.0);
        assert_eq!(snap.avg_latency_ms, 0);
        assert!(snap.per_provider.is_empty());
    }

    #[test]
    fn records_success_request() {
        let store = MetricsStore::new();
        store.record("openai", 100, false);
        store.record("openai", 200, false);

        let snap = store.snapshot();
        assert_eq!(snap.total_requests, 2);
        assert_eq!(snap.total_errors, 0);
        assert_eq!(snap.error_rate, 0.0);
        assert_eq!(snap.avg_latency_ms, 150);

        let openai = &snap.per_provider["openai"];
        assert_eq!(openai.requests, 2);
        assert_eq!(openai.errors, 0);
        assert_eq!(openai.avg_latency_ms, 150);
    }

    #[test]
    fn records_error_request() {
        let store = MetricsStore::new();
        store.record("anthropic", 50, false);
        store.record("anthropic", 80, true);

        let snap = store.snapshot();
        assert_eq!(snap.total_requests, 2);
        assert_eq!(snap.total_errors, 1);
        assert!((snap.error_rate - 0.5).abs() < f64::EPSILON);

        let prov = &snap.per_provider["anthropic"];
        assert_eq!(prov.errors, 1);
    }

    #[test]
    fn tracks_multiple_providers_independently() {
        let store = MetricsStore::new();
        store.record("openai", 100, false);
        store.record("forge", 200, false);
        store.record("openai", 300, true);

        let snap = store.snapshot();
        assert_eq!(snap.total_requests, 3);
        assert_eq!(snap.per_provider["openai"].requests, 2);
        assert_eq!(snap.per_provider["forge"].requests, 1);
        assert_eq!(snap.per_provider["openai"].errors, 1);
        assert_eq!(snap.per_provider["forge"].errors, 0);
    }

    #[test]
    fn reset_clears_all_counters() {
        let store = MetricsStore::new();
        store.record("openai", 100, false);
        store.record("forge", 50, true);
        store.reset();

        let snap = store.snapshot();
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.total_errors, 0);
        assert!(snap.per_provider.is_empty());
    }

    // -----------------------------------------------------------------------
    // Prometheus text format tests
    // -----------------------------------------------------------------------

    #[test]
    fn prometheus_text_contains_required_help_and_type_headers() {
        let store = MetricsStore::new();
        store.record("openai", 100, false);
        let text = store.prometheus_text(&HashMap::new());
        assert!(text.contains("# HELP substrate_requests_total"));
        assert!(text.contains("# TYPE substrate_requests_total counter"));
        assert!(text.contains("# HELP substrate_errors_total"));
        assert!(text.contains("# TYPE substrate_errors_total counter"));
        assert!(text.contains("# HELP substrate_latency_ms"));
        assert!(text.contains("# TYPE substrate_latency_ms gauge"));
        assert!(text.contains("# HELP substrate_rate_limit_hits"));
        assert!(text.contains("# TYPE substrate_rate_limit_hits counter"));
    }

    #[test]
    fn prometheus_text_counter_values_are_correct() {
        let store = MetricsStore::new();
        store.record("openai", 500, false);
        store.record("openai", 300, false);
        store.record("openai", 200, true);
        let text = store.prometheus_text(&HashMap::new());
        assert!(text.contains("substrate_requests_total{provider=\"openai\"} 3"));
        assert!(text.contains("substrate_errors_total{provider=\"openai\"} 1"));
        assert!(text.contains("substrate_latency_ms{provider=\"openai\"} 1000"));
    }

    #[test]
    fn prometheus_text_rate_limit_hits_gauge() {
        let store = MetricsStore::new();
        store.record("anthropic", 100, false);
        let mut hits = HashMap::new();
        hits.insert("anthropic".to_string(), 7u64);
        let text = store.prometheus_text(&hits);
        assert!(text.contains("substrate_rate_limit_hits{provider=\"anthropic\"} 7"));
    }

    #[test]
    fn prometheus_text_label_escaping_special_chars() {
        // provider name with backslash, double-quote, newline
        let store = MetricsStore::new();
        store.record("my\\provider\"name\nnewline", 50, false);
        let text = store.prometheus_text(&HashMap::new());
        // backslash escaped as \\, quote as \", newline as \n
        assert!(text.contains(r#"provider="my\\provider\"name\nnewline""#));
    }

    #[test]
    fn prometheus_text_empty_store_has_no_label_lines() {
        let store = MetricsStore::new();
        let text = store.prometheus_text(&HashMap::new());
        // headers present but no actual metric lines with labels
        assert!(!text.contains("provider="));
        assert!(text.contains("# HELP substrate_requests_total"));
    }

    // -----------------------------------------------------------------------
    // Prometheus HistogramVec (latency histogram) tests
    // -----------------------------------------------------------------------

    #[test]
    fn record_latency_no_panic() {
        record_latency("openai", 42.0);
        record_latency("anthropic", 150.0);
        record_latency("openai", 500.0);
    }

    #[test]
    fn histogram_init_is_idempotent() {
        let h1 = latency_histogram();
        let h2 = latency_histogram();
        assert!(std::ptr::eq(h1, h2));
    }
}
