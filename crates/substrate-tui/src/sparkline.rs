//! Latency sparkline chart for provider metrics.
use std::collections::VecDeque;

pub const SPARKLINE_LEN: usize = 60;

#[derive(Debug, Default, Clone)]
pub struct LatencyHistory {
    pub provider: String,
    pub samples: VecDeque<u64>,
    latencies: VecDeque<u64>,
}

impl LatencyHistory {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            samples: VecDeque::with_capacity(SPARKLINE_LEN),
            latencies: VecDeque::new(),
        }
    }
    pub fn push(&mut self, ms: u64) {
        if self.samples.len() >= SPARKLINE_LEN {
            self.samples.pop_front();
        }
        self.samples.push_back(ms);
        self.latencies.push_back(ms);
    }
    pub fn p50(&self) -> Option<u64> {
        percentile(&self.latencies, 50)
    }
    pub fn p95(&self) -> Option<u64> {
        percentile(&self.latencies, 95)
    }
    pub fn max(&self) -> Option<u64> {
        self.latencies.iter().copied().max()
    }
    pub fn as_ratatui_data(&self) -> Vec<u64> {
        self.samples.iter().copied().collect()
    }
}

fn percentile(data: &VecDeque<u64>, pct: usize) -> Option<u64> {
    if data.is_empty() {
        return None;
    }
    let mut sorted: Vec<u64> = data.iter().copied().collect();
    sorted.sort_unstable();
    let pct = pct.clamp(1, 100);
    let idx = (pct * sorted.len()).div_ceil(100) - 1;
    sorted.get(idx).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn push_caps_at_60() {
        let mut h = LatencyHistory::new("test");
        for i in 0..70u64 {
            h.push(i);
        }
        assert_eq!(h.samples.len(), SPARKLINE_LEN);
    }
    #[test]
    fn p50_basic() {
        let mut h = LatencyHistory::new("t");
        for i in 1..=100u64 {
            h.push(i);
        }
        let p = h.p50().unwrap();
        assert!(p >= 49 && p <= 51);
    }
    #[test]
    fn p95_greater_than_p50() {
        let mut h = LatencyHistory::new("t");
        for i in 1..=60u64 {
            h.push(i);
        }
        assert!(h.p95().unwrap() > h.p50().unwrap());
    }
    #[test]
    fn empty_returns_none() {
        let h = LatencyHistory::new("t");
        assert!(h.p50().is_none());
        assert!(h.max().is_none());
    }
    #[test]
    fn max_is_largest() {
        let mut h = LatencyHistory::new("t");
        h.push(10);
        h.push(999);
        h.push(50);
        assert_eq!(h.max().unwrap(), 999);
    }
    #[test]
    fn as_ratatui_data_length() {
        let mut h = LatencyHistory::new("t");
        h.push(1);
        h.push(2);
        h.push(3);
        assert_eq!(h.as_ratatui_data().len(), 3);
    }
}
