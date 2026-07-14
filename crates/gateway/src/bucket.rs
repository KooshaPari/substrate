pub struct TokenBucket2 {
    capacity: u32,
    refill_rate: f64,
    tokens: f64,
    last_refill_ms: u64,
}
impl TokenBucket2 {
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            refill_rate,
            tokens: capacity as f64,
            last_refill_ms: 0,
        }
    }
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0) as u64
    }
    pub fn refill(&mut self) {
        let now = Self::now_ms();
        let elapsed = (now.saturating_sub(self.last_refill_ms)) as f64 / 1000.0;
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill_ms = now;
    }
    pub fn try_consume(&mut self, amount: u32) -> bool {
        self.refill();
        if self.tokens >= amount as f64 {
            self.tokens -= amount as f64;
            true
        } else {
            false
        }
    }
    pub fn available(&mut self) -> u32 {
        self.refill();
        self.tokens as u32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn burst_then_wait() {
        let mut b = TokenBucket2::new(5, 1.0);
        assert!(b.try_consume(5));
        assert!(!b.try_consume(1));
    }
    #[test]
    fn refill_after_time() {
        let mut b = TokenBucket2::new(5, 1.0);
        b.try_consume(5);
        assert!(!b.try_consume(1));
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(b.try_consume(1));
    }
    #[test]
    fn available_max_cap() {
        let mut b = TokenBucket2::new(10, 0.001);
        assert_eq!(b.available(), 10);
    }
}
