use std::time::Instant;

pub struct TokenBucket { capacity: f64, refill_per_sec: f64, tokens: f64, last_refill: Instant }
impl TokenBucket {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self { capacity, refill_per_sec, tokens: capacity, last_refill: Instant::now() }
    }
    pub fn try_acquire(&mut self, amount: f64) -> bool {
        self.refill();
        if self.tokens >= amount { self.tokens -= amount; true } else { false }
    }
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = now;
    }
    pub fn available(&mut self) -> f64 { self.refill(); self.tokens }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn full_bucket_allows() { let mut b = TokenBucket::new(10.0, 1.0); assert!(b.try_acquire(5.0)); assert!(b.try_acquire(5.0)); assert!(!b.try_acquire(1.0)); }
    #[test] fn empty_blocks() { let mut b = TokenBucket::new(1.0, 0.001); b.try_acquire(1.0); assert!(!b.try_acquire(1.0)); }
    #[test] fn available_after_acquire() { let mut b = TokenBucket::new(10.0, 1.0); b.try_acquire(5.0); assert!(b.available() < 6.0); }
}
