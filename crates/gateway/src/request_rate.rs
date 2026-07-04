//! Request rate tracking: sliding window requests-per-second counter.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WINDOW_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub struct RequestRate {
    inner: Arc<Mutex<RateInner>>,
}

#[derive(Debug)]
struct RateInner {
    timestamps: VecDeque<Instant>,
    window: Duration,
}

impl RequestRate {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(RateInner { timestamps: VecDeque::new(), window: Duration::from_secs(WINDOW_SECS) })) }
    }
    pub fn record(&self) {
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        inner.timestamps.push_back(now);
        let cutoff = now - inner.window;
        while inner.timestamps.front().map_or(false, |t| *t < cutoff) {
            inner.timestamps.pop_front();
        }
    }
    pub fn rate_per_sec(&self) -> f64 {
        let inner = self.inner.lock().unwrap();
        if inner.timestamps.is_empty() { return 0.0; }
        inner.timestamps.len() as f64 / WINDOW_SECS as f64
    }
    pub fn count_in_window(&self) -> usize {
        self.inner.lock().unwrap().timestamps.len()
    }
}

impl Default for RequestRate {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn empty_rate_is_zero() { assert_eq!(RequestRate::new().rate_per_sec(), 0.0); }
    #[test] fn record_increments_count() { let r = RequestRate::new(); r.record(); r.record(); assert_eq!(r.count_in_window(), 2); }
    #[test] fn rate_reflects_count() { let r = RequestRate::new(); for _ in 0..10 { r.record(); } assert!(r.rate_per_sec() > 0.0); }
    #[test] fn clone_shares_state() { let r = RequestRate::new(); let r2 = r.clone(); r.record(); assert_eq!(r2.count_in_window(), 1); }
    #[test] fn default_is_empty() { assert_eq!(RequestRate::default().count_in_window(), 0); }
    #[test] fn high_volume_bounded() { let r = RequestRate::new(); for _ in 0..1000 { r.record(); } assert_eq!(r.count_in_window(), 1000); }
}