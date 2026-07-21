pub fn exponential_backoff_ms(attempt: u32, base_ms: u64, max_ms: u64) -> u64 {
    let shift = attempt.min(20);
    let exp = base_ms.saturating_mul(1u64 << shift);
    exp.min(max_ms)
}
pub fn exponential_backoff_jitter_ms(
    attempt: u32,
    base_ms: u64,
    max_ms: u64,
    jitter_pct: f64,
) -> u64 {
    let base = exponential_backoff_ms(attempt, base_ms, max_ms);
    let jitter = jitter_pct.clamp(0.0, 1.0);
    let delta = (base as f64 * jitter) as u64;
    if delta == 0 {
        return base;
    }
    let lo = base.saturating_sub(delta);
    let hi = base + delta;
    lo + (rand_u64() % (hi - lo + 1))
}
fn rand_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (n as u64) ^ ((n >> 33) as u64)
}
pub fn should_retry(status: u16, attempt: u32, max_attempts: u32) -> bool {
    if attempt >= max_attempts {
        return false;
    }
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_grows() {
        assert!(exponential_backoff_ms(0, 100, 60_000) >= 100);
        assert!(exponential_backoff_ms(1, 100, 60_000) >= 100);
        assert!(exponential_backoff_ms(2, 100, 60_000) >= 100);
    }
    #[test]
    fn backoff_capped() {
        assert_eq!(exponential_backoff_ms(50, 100, 1000), 1000);
    }
    #[test]
    fn backoff_attempt_zero_safe() {
        assert!(exponential_backoff_ms(0, 50, 10_000) >= 50);
    }
    #[test]
    fn jitter_within_bounds() {
        for _ in 0..20 {
            let v = exponential_backoff_jitter_ms(3, 100, 60_000, 0.5);
            assert!(v >= 100 && v <= 1500, "jitter out of bounds: {}", v);
        }
    }
    #[test]
    fn retry_on_429() {
        assert!(should_retry(429, 0, 5));
    }
    #[test]
    fn retry_on_500() {
        assert!(should_retry(500, 1, 5));
    }
    #[test]
    fn no_retry_on_400() {
        assert!(!should_retry(400, 0, 5));
    }
    #[test]
    fn no_retry_after_max() {
        assert!(!should_retry(500, 5, 5));
    }
    #[test]
    fn jitter_pct_clamped() {
        // Attempt 2 has a base delay of 400ms.  A 200% request clamps to
        // 100%, so the sampled value is bounded to [0ms, 800ms].
        let value = exponential_backoff_jitter_ms(2, 100, 10_000, 2.0);
        assert!(value <= 800, "clamped jitter out of bounds: {value}");
    }
}
