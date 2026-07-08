use std::time::{Duration, Instant};
pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last: Instant,
}
impl TokenBucket {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self { capacity, refill_per_sec, tokens: capacity, last: Instant::now() }
    }
    pub fn try_acquire(&mut self, n: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last = now;
        if self.tokens >= n { self.tokens -= n; true } else { false }
    }
    pub fn available(&mut self) -> f64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last = now;
        self.tokens
    }
    pub fn capacity(&self) -> f64 { self.capacity }
}
pub fn retry_after_429(retry_after_header: Option<&str>, reset_at: Option<u64>, now_unix: u64) -> Duration {
    if let Some(h) = retry_after_header {
        if let Ok(secs) = h.parse::<u64>() { return Duration::from_secs(secs); }
        if let Ok(ts) = h.parse::<u64>() {
            if ts > now_unix { return Duration::from_secs(ts - now_unix); }
        }
    }
    if let Some(reset) = reset_at {
        if reset > now_unix { return Duration::from_secs(reset - now_unix); }
    }
    Duration::from_secs(60)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn bucket_grants_initial() {
        let mut b = TokenBucket::new(10.0, 1.0);
        assert!(b.try_acquire(5.0));
        assert!(b.try_acquire(5.0));
        assert!(!b.try_acquire(1.0));
    }
    #[test] fn bucket_refills() {
        let mut b = TokenBucket::new(2.0, 100.0);
        assert!(b.try_acquire(2.0));
        assert!(!b.try_acquire(1.0));
        std::thread::sleep(Duration::from_millis(20));
        assert!(b.try_acquire(1.0));
    }
    #[test] fn bucket_capacity() {
        let b = TokenBucket::new(100.0, 5.0);
        assert_eq!(b.capacity(), 100.0);
    }
    #[test] fn retry_after_secs() { assert_eq!(retry_after_429(Some("30"), None, 0), Duration::from_secs(30)); }
    #[test] fn retry_after_large() { assert_eq!(retry_after_429(Some("100"), None, 80), Duration::from_secs(100)); }
    #[test] fn retry_after_reset() { assert_eq!(retry_after_429(None, Some(200), 150), Duration::from_secs(50)); }
    #[test] fn retry_after_default() { assert_eq!(retry_after_429(None, None, 0), Duration::from_secs(60)); }
}
