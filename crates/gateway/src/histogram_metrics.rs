//! Simple latency histogram with bucket counters.
//!
//! Bucket boundaries are exclusive upper bounds. The implicit `+Inf` bucket
//! captures everything above the largest boundary. Percentiles use linear
//! interpolation between the bucket boundary and the next, matching the
//! convention used by Prometheus' `histogram_quantile`.

#[derive(Debug, Clone)]
pub struct Histogram {
    buckets: Vec<(f64, u64)>,
    count: u64,
    sum: f64,
    min: f64,
    max: f64,
}

impl Histogram {
    pub fn with_buckets(boundaries: &[f64]) -> Self {
        let mut sorted: Vec<f64> = boundaries.iter().copied().filter(|b| b.is_finite()).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted.dedup();
        Self {
            buckets: sorted.into_iter().map(|b| (b, 0)).collect(),
            count: 0,
            sum: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }

    pub fn record(&mut self, value: f64) {
        if !value.is_finite() {
            return;
        }
        self.count += 1;
        self.sum += value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        for (boundary, counter) in self.buckets.iter_mut() {
            if value <= *boundary {
                *counter += 1;
            }
        }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }

    pub fn min(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.min
        }
    }

    pub fn max(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.max
        }
    }

    pub fn snapshot(&self) -> Vec<(f64, u64)> {
        self.buckets.clone()
    }

    pub fn p50(&self) -> f64 {
        self.percentile(0.50)
    }

    pub fn p99(&self) -> f64 {
        self.percentile(0.99)
    }

    fn percentile(&self, q: f64) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let rank = (q * self.count as f64).ceil() as u64;
        let target = rank.max(1);

        let mut cumulative: u64 = 0;
        let mut prev_boundary = 0.0_f64;
        let mut prev_count: u64 = 0;
        for (boundary, counter) in &self.buckets {
            cumulative = *counter;
            if cumulative >= target {
                let bucket_count = cumulative - prev_count;
                if bucket_count == 0 {
                    return *boundary;
                }
                let pos_in_bucket = (target - prev_count) as f64;
                let frac = pos_in_bucket / bucket_count as f64;
                return prev_boundary + (*boundary - prev_boundary) * frac;
            }
            prev_boundary = *boundary;
            prev_count = cumulative;
        }
        // Fall back to max if everything fits below the last boundary.
        if self.count > 0 {
            self.max
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_histogram_has_zero_count_and_zero_mean() {
        let h = Histogram::with_buckets(&[1.0, 5.0, 10.0]);
        assert_eq!(h.count(), 0);
        assert_eq!(h.mean(), 0.0);
        assert_eq!(h.min(), 0.0);
        assert_eq!(h.max(), 0.0);
        assert_eq!(h.p50(), 0.0);
        assert_eq!(h.p99(), 0.0);
    }

    #[test]
    fn records_inclusive_bucket_counts() {
        let mut h = Histogram::with_buckets(&[1.0, 5.0, 10.0]);
        h.record(0.5);
        h.record(1.0);
        h.record(3.0);
        h.record(7.0);
        h.record(100.0); // overflow bucket
        let snap = h.snapshot();
        assert_eq!(snap, vec![(1.0, 2), (5.0, 3), (10.0, 4)]);
        assert_eq!(h.count(), 5);
    }

    #[test]
    fn mean_is_sum_over_count() {
        let mut h = Histogram::with_buckets(&[10.0]);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            h.record(v);
        }
        assert_eq!(h.mean(), 3.0);
        assert_eq!(h.min(), 1.0);
        assert_eq!(h.max(), 5.0);
    }

    #[test]
    fn p50_falls_in_lowest_bucket_for_balanced_data() {
        let mut h = Histogram::with_buckets(&[10.0, 20.0, 30.0]);
        for v in 1..=10 {
            h.record(v as f64);
        }
        // rank = ceil(0.5 * 10) = 5; 5th observation sits at value 5.0
        let p50 = h.p50();
        assert!((p50 - 5.0).abs() < 1.0, "p50={}", p50);
    }

    #[test]
    fn p99_handles_overflow_bucket() {
        let mut h = Histogram::with_buckets(&[10.0]);
        for v in 1..=100 {
            h.record(v as f64);
        }
        let p99 = h.p99();
        // rank = ceil(0.99 * 100) = 99; falls above the 10.0 boundary
        assert!(p99 > 10.0, "p99={}", p99);
        assert!(p99 <= h.max(), "p99={} > max={}", p99, h.max());
    }

    #[test]
    fn rejects_non_finite_values() {
        let mut h = Histogram::with_buckets(&[1.0]);
        h.record(f64::NAN);
        h.record(f64::INFINITY);
        h.record(0.5);
        assert_eq!(h.count(), 1);
        assert_eq!(h.mean(), 0.5);
    }
}