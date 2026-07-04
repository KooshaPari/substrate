//! Retry with exponential back-off and full jitter for upstream HTTP calls.
//!
//! Retries on 5xx, 429, and connection/timeout errors.
//! Returns immediately on 4xx (except 429).
//! After `max_attempts` the last error is returned wrapped in [`RetryError::Exhausted`].
//!
//! Full-jitter formula: `sleep = rand(0, min(max_delay_ms, base_delay_ms * 2^attempt))`
//!
//! # Environment variables
//! - `SUBSTRATE_RETRY_ATTEMPTS` — overrides `max_attempts` (default 3)
//! - `SUBSTRATE_RETRY_BASE_MS`  — overrides `base_delay_ms` (default 100)

use std::future::Future;
use std::time::Duration;

use rand::Rng;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration for the retry loop.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (including the first).
    pub max_attempts: u32,
    /// Base delay in milliseconds for the exponential-back-off curve.
    pub base_delay_ms: u64,
    /// Hard cap on the computed delay (milliseconds).
    pub max_delay_ms: u64,
    /// When `true` the delay is randomised in `[0, computed_delay]` (full jitter).
    pub jitter: bool,
}

impl RetryPolicy {
    /// Default policy: 3 attempts, 100 ms base, 5 s cap, jitter enabled.
    pub fn default_policy() -> Self {
        let max_attempts = std::env::var("SUBSTRATE_RETRY_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3u32);
        let base_delay_ms = std::env::var("SUBSTRATE_RETRY_BASE_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100u64);
        Self {
            max_attempts,
            base_delay_ms,
            max_delay_ms: 5_000,
            jitter: true,
        }
    }

    /// Compute the sleep duration for the given attempt index (0-based).
    ///
    /// Uses `min(max_delay_ms, base_delay_ms * 2^attempt)` then optionally
    /// applies full jitter: `rand(0, computed)`.
    pub fn sleep_duration(&self, attempt: u32) -> Duration {
        // 2^attempt — saturate to u64::MAX on overflow.
        let exp = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
        let cap = self
            .base_delay_ms
            .saturating_mul(exp)
            .min(self.max_delay_ms);
        let ms = if self.jitter && cap > 0 {
            rand::thread_rng().gen_range(0..=cap)
        } else {
            cap
        };
        Duration::from_millis(ms)
    }
}

/// Error returned when all retry attempts are exhausted.
#[derive(Debug)]
pub struct RetryExhausted {
    /// Total number of attempts made.
    pub attempts: u32,
    /// The last error observed.
    pub last_error: String,
}

impl std::fmt::Display for RetryExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "retry exhausted after {} attempt(s): {}",
            self.attempts, self.last_error
        )
    }
}

impl std::error::Error for RetryExhausted {}

// ---------------------------------------------------------------------------
// Retry classification
// ---------------------------------------------------------------------------

/// Whether an upstream HTTP status code should trigger a retry.
///
/// - 429 → retry (rate-limited)
/// - 5xx → retry (server error / transient)
/// - other 4xx → **no** retry (client error, permanent)
/// - 2xx / 3xx → never called (success path)
pub fn should_retry_status(status: u16) -> bool {
    status == 429 || status >= 500
}

// ---------------------------------------------------------------------------
// Core retry loop
// ---------------------------------------------------------------------------

/// Retry `f` according to `policy`.
///
/// `f` is a zero-argument async closure that returns `Result<T, RetryableError>`.
/// [`RetryableError`] carries both the error message and whether the status code
/// means the call should be retried.
///
/// On success the value is returned immediately.
/// When all attempts are exhausted `Err(RetryExhausted { … })` is returned.
pub async fn with_retry<F, Fut, T>(policy: &RetryPolicy, mut f: F) -> Result<T, RetryExhausted>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RetryableError>>,
{
    let mut last_err = String::from("no attempts made");

    for attempt in 0..policy.max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                last_err = e.message.clone();
                if !e.retryable {
                    // Permanent error — do not waste the remaining attempts.
                    return Err(RetryExhausted {
                        attempts: attempt + 1,
                        last_error: last_err,
                    });
                }
                // Retryable: sleep before the next attempt (except after the last).
                if attempt + 1 < policy.max_attempts {
                    tokio::time::sleep(policy.sleep_duration(attempt)).await;
                }
            }
        }
    }

    Err(RetryExhausted {
        attempts: policy.max_attempts,
        last_error: last_err,
    })
}

/// A typed error returned by the closure passed to [`with_retry`].
#[derive(Debug)]
pub struct RetryableError {
    /// Human-readable error message.
    pub message: String,
    /// `true` when the caller should retry (5xx, 429, timeout/connection error).
    pub retryable: bool,
}

impl RetryableError {
    /// Construct a retryable error (5xx, 429, connection/timeout).
    pub fn retryable(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            retryable: true,
        }
    }

    /// Construct a permanent error (4xx except 429) — retry loop aborts immediately.
    pub fn permanent(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            retryable: false,
        }
    }

    /// Build from an HTTP status code and body string.
    pub fn from_status(status: u16, body: &str, provider: &str) -> Self {
        let message = format!("upstream provider {provider} returned {status}: {body}");
        if should_retry_status(status) {
            Self::retryable(message)
        } else {
            Self::permanent(message)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // should_retry_status
    // -----------------------------------------------------------------------

    #[test]
    fn retry_on_429() {
        assert!(should_retry_status(429), "429 must be retried");
    }

    #[test]
    fn retry_on_5xx() {
        for code in [500, 502, 503, 504] {
            assert!(should_retry_status(code), "{code} must be retried");
        }
    }

    #[test]
    fn no_retry_on_4xx_except_429() {
        for code in [400, 401, 403, 404, 422] {
            assert!(!should_retry_status(code), "{code} must NOT be retried");
        }
    }

    // -----------------------------------------------------------------------
    // with_retry — exhaustion
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn exhaustion_returns_error_after_max_attempts() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 0,
            max_delay_ms: 0,
            jitter: false,
        };

        let calls = Arc::new(AtomicU32::new(0));
        let calls2 = calls.clone();

        let result = with_retry(&policy, || {
            let c = calls2.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(RetryableError::retryable("transient"))
            }
        })
        .await;

        assert!(result.is_err(), "must fail after exhaustion");
        let err = result.unwrap_err();
        assert_eq!(err.attempts, 3, "must record 3 attempts");
        assert!(
            err.last_error.contains("transient"),
            "must surface last error"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "closure called exactly 3 times"
        );
    }

    // -----------------------------------------------------------------------
    // with_retry — 4xx not retried
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn permanent_error_aborts_immediately() {
        let policy = RetryPolicy {
            max_attempts: 5,
            base_delay_ms: 0,
            max_delay_ms: 0,
            jitter: false,
        };

        let calls = Arc::new(AtomicU32::new(0));
        let calls2 = calls.clone();

        let result = with_retry(&policy, || {
            let c = calls2.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(RetryableError::permanent("client error 403"))
            }
        })
        .await;

        assert!(result.is_err());
        // Only 1 call — permanent error must not trigger further attempts.
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "permanent error: only 1 call"
        );
    }

    // -----------------------------------------------------------------------
    // with_retry — 429 IS retried
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn rate_limit_429_is_retried() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 0,
            max_delay_ms: 0,
            jitter: false,
        };

        let calls = Arc::new(AtomicU32::new(0));
        let calls2 = calls.clone();

        let result = with_retry(&policy, || {
            let c = calls2.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(RetryableError::from_status(429, "rate limited", "test"))
            }
        })
        .await;

        // Must exhaust all 3 attempts (429 is retryable).
        assert!(result.is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "429 must trigger all 3 retries"
        );
    }

    // -----------------------------------------------------------------------
    // sleep_duration — delay grows with cap
    // -----------------------------------------------------------------------

    #[test]
    fn delay_grows_and_caps() {
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay_ms: 100,
            max_delay_ms: 500,
            jitter: false,
        };

        // attempt 0 → 100ms, attempt 1 → 200ms, attempt 2 → 400ms, attempt 3 → capped at 500ms
        assert_eq!(policy.sleep_duration(0), Duration::from_millis(100));
        assert_eq!(policy.sleep_duration(1), Duration::from_millis(200));
        assert_eq!(policy.sleep_duration(2), Duration::from_millis(400));
        assert_eq!(
            policy.sleep_duration(3),
            Duration::from_millis(500),
            "capped at max"
        );
        assert_eq!(
            policy.sleep_duration(9),
            Duration::from_millis(500),
            "still capped"
        );
    }

    // -----------------------------------------------------------------------
    // Jitter produces different values
    // -----------------------------------------------------------------------

    #[test]
    fn jitter_produces_different_delays() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 100,
            max_delay_ms: 5_000,
            jitter: true,
        };

        // Collect 20 delay samples for attempt=3 (cap=800ms); they should not all be equal.
        let samples: Vec<Duration> = (0..20).map(|_| policy.sleep_duration(3)).collect();
        let unique: std::collections::HashSet<_> = samples.iter().map(|d| d.as_millis()).collect();
        assert!(
            unique.len() > 1,
            "jitter must produce varied delays; got {unique:?}"
        );
        // All values must be within [0, 800ms].
        for d in &samples {
            assert!(d.as_millis() <= 800, "jitter exceeded cap: {d:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Success on second attempt
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn succeeds_on_second_attempt() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 0,
            max_delay_ms: 0,
            jitter: false,
        };

        let calls = Arc::new(AtomicU32::new(0));
        let calls2 = calls.clone();

        let result = with_retry(&policy, || {
            let c = calls2.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(RetryableError::retryable("first call fails"))
                } else {
                    Ok(42u32)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "succeeded on attempt 2");
    }
}
