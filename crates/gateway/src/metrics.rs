//! In-memory request metrics for the gateway.
//!
//! Tracks total requests, errors, latency, and per-provider breakdowns using
//! lock-free [`AtomicU64`] globals for the aggregate path and a [`Mutex`]-guarded
//! [`HashMap`] for per-provider detail.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

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
}
