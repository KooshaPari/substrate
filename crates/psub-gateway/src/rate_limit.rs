//! Per-provider token-bucket rate limiter.
//!
//! Each provider gets an independent token bucket configured via
//! `RateLimiterConfig`.  The bucket is hand-rolled using wall-clock time so
//! the crate stays dependency-free (no `governor` required).
//!
//! Env-var overrides (per-provider, using the provider name uppercased):
//!   `{PROVIDER}_RATE_RPS`   — requests per second  (float, default 10.0)
//!   `{PROVIDER}_RATE_BURST` — burst size            (u32,   default 20)

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Returned when a provider's token bucket is empty.
#[derive(Debug, Clone)]
pub struct RateLimitError {
    pub provider: String,
    /// Seconds until at least one token refills.
    pub retry_after_secs: f64,
}

impl fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rate limit exceeded for provider '{}'; retry after {:.2}s",
            self.provider, self.retry_after_secs
        )
    }
}

impl std::error::Error for RateLimitError {}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Per-provider bucket parameters.
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Steady-state fill rate (tokens per second).
    pub requests_per_second: f64,
    /// Maximum tokens the bucket can hold (burst allowance).
    pub burst_size: u32,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10.0,
            burst_size: 20,
        }
    }
}

impl RateLimiterConfig {
    /// Read from env-vars `{PROVIDER_UPPER}_RATE_RPS` and `{PROVIDER_UPPER}_RATE_BURST`.
    /// Falls back to `Default` for any missing/invalid value.
    pub fn from_env(provider: &str) -> Self {
        let prefix = provider.to_uppercase().replace('-', "_");
        let rps = std::env::var(format!("{prefix}_RATE_RPS"))
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| *v > 0.0)
            .unwrap_or(10.0);
        let burst = std::env::var(format!("{prefix}_RATE_BURST"))
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(20);
        Self {
            requests_per_second: rps,
            burst_size: burst,
        }
    }
}

// ---------------------------------------------------------------------------
// Single-provider bucket
// ---------------------------------------------------------------------------

/// Token bucket for one provider.
#[derive(Debug)]
struct TokenBucket {
    config: RateLimiterConfig,
    /// Current token count (fractional to allow sub-second precision).
    tokens: f64,
    last_refill: Instant,
    /// Cumulative 429 hits for metrics.
    hits: u64,
}

impl TokenBucket {
    fn new(config: RateLimiterConfig) -> Self {
        let burst = config.burst_size as f64;
        Self {
            config,
            tokens: burst,
            last_refill: Instant::now(),
            hits: 0,
        }
    }

    /// Refill tokens based on elapsed time, then attempt to consume one.
    ///
    /// Returns `Ok(())` on success, or `Err(retry_after_secs)` when empty.
    fn try_consume(&mut self) -> Result<(), f64> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;

        // Refill
        let new_tokens = elapsed * self.config.requests_per_second;
        self.tokens = (self.tokens + new_tokens).min(self.config.burst_size as f64);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            self.hits += 1;
            // Time until one more token is available
            let deficit = 1.0 - self.tokens;
            let retry = deficit / self.config.requests_per_second;
            Err(retry)
        }
    }

    fn hits(&self) -> u64 {
        self.hits
    }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Thread-safe store of per-provider token buckets.
#[derive(Clone, Default)]
pub struct RateLimiterStore {
    inner: Arc<Mutex<HashMap<String, TokenBucket>>>,
}

impl RateLimiterStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-seed a provider with an explicit config (useful in tests).
    pub fn register(&self, provider: &str, config: RateLimiterConfig) {
        let mut guard = self.inner.lock().expect("rate limiter lock poisoned");
        guard.insert(provider.to_string(), TokenBucket::new(config));
    }

    /// Attempt to consume one token for `provider`.
    ///
    /// Lazily creates a bucket from env-vars (or defaults) on first access.
    /// Returns `Err(RateLimitError)` when the bucket is empty.
    pub fn check_and_consume(&self, provider: &str) -> Result<(), RateLimitError> {
        let mut guard = self.inner.lock().expect("rate limiter lock poisoned");
        let bucket = guard
            .entry(provider.to_string())
            .or_insert_with(|| TokenBucket::new(RateLimiterConfig::from_env(provider)));
        bucket
            .try_consume()
            .map_err(|retry_after_secs| RateLimitError {
                provider: provider.to_string(),
                retry_after_secs,
            })
    }

    /// Return the current `rate_limit_hits` counter for each provider.
    pub fn hits_snapshot(&self) -> HashMap<String, u64> {
        let guard = self.inner.lock().expect("rate limiter lock poisoned");
        guard.iter().map(|(k, v)| (k.clone(), v.hits())).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tiny_store(rps: f64, burst: u32) -> RateLimiterStore {
        let store = RateLimiterStore::new();
        store.register(
            "testprovider",
            RateLimiterConfig {
                requests_per_second: rps,
                burst_size: burst,
            },
        );
        store
    }

    #[test]
    fn burst_allows_up_to_burst_size() {
        let store = tiny_store(1.0, 5);
        for _ in 0..5 {
            assert!(store.check_and_consume("testprovider").is_ok());
        }
    }

    #[test]
    fn limit_hit_returns_error_after_burst() {
        let store = tiny_store(1.0, 3);
        for _ in 0..3 {
            store.check_and_consume("testprovider").unwrap();
        }
        let err = store.check_and_consume("testprovider").unwrap_err();
        assert_eq!(err.provider, "testprovider");
        assert!(err.retry_after_secs > 0.0);
    }

    #[test]
    fn counter_increments_on_limit_hit() {
        let store = tiny_store(1.0, 1);
        store.check_and_consume("testprovider").unwrap(); // consumes
        store.check_and_consume("testprovider").unwrap_err(); // hit #1
        store.check_and_consume("testprovider").unwrap_err(); // hit #2
        let snap = store.hits_snapshot();
        assert_eq!(snap["testprovider"], 2);
    }

    #[test]
    fn bucket_refills_after_delay() {
        let store = tiny_store(1000.0, 1); // very fast refill
        store.check_and_consume("testprovider").unwrap(); // drain
                                                          // Sleep 2ms — at 1000 rps a 2ms sleep should give ≥2 tokens
        std::thread::sleep(Duration::from_millis(2));
        assert!(store.check_and_consume("testprovider").is_ok());
    }

    #[test]
    fn lazy_provider_gets_default_config() {
        let store = RateLimiterStore::new();
        // No registration — uses defaults (burst=20)
        for _ in 0..20 {
            assert!(store.check_and_consume("newprovider").is_ok());
        }
        assert!(store.check_and_consume("newprovider").is_err());
    }

    #[test]
    fn multiple_providers_are_independent() {
        let store = tiny_store(1.0, 2);
        store.register(
            "other",
            RateLimiterConfig {
                requests_per_second: 1.0,
                burst_size: 5,
            },
        );
        // Drain testprovider
        for _ in 0..2 {
            store.check_and_consume("testprovider").unwrap();
        }
        assert!(store.check_and_consume("testprovider").is_err());
        // other still has tokens
        assert!(store.check_and_consume("other").is_ok());
    }
}
