//! Basic statistical primitives over a slice of `f64` samples.
//!
//! All functions are O(n) on the input length except [`median`] and
//! [`percentile`], which copy and sort (O(n log n)). Empty input returns
//! `None` rather than panicking — callers should decide what to do
//! when they have no samples.

use std::collections::HashMap;

/// Arithmetic mean. Returns `None` for an empty slice.
pub fn mean(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let sum: f64 = samples.iter().sum();
    Some(sum / samples.len() as f64)
}

/// Sample variance (Bessel-corrected, divides by n - 1).
/// Returns `None` for slices with fewer than 2 elements.
pub fn variance(samples: &[f64]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let m = mean(samples)?;
    let s: f64 = samples.iter().map(|x| (x - m).powi(2)).sum();
    Some(s / (samples.len() as f64 - 1.0))
}

/// Sample standard deviation (sqrt of [`variance`]).
pub fn stddev(samples: &[f64]) -> Option<f64> {
    variance(samples).map(|v| v.sqrt())
}

/// Median (50th percentile) — copies and sorts internally.
pub fn median(samples: &[f64]) -> Option<f64> {
    percentile(samples, 50.0)
}

/// Mode — most-frequent value in the slice. Returns `None` if every
/// value appears exactly once (or input is empty).
///
/// Equality is by exact `==` on `f64`; values that compare equal but
/// have different representations (`0.0` vs `-0.0`) are treated as
/// distinct. For bucketed mode, sort and discretize first.
pub fn mode(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut counts: HashMap<i64, (f64, usize)> = HashMap::new();
    for &x in samples {
        let key = x.to_bits() as i64;
        let entry = counts.entry(key).or_insert((x, 0));
        entry.1 += 1;
    }
    let mut best_value = None;
    let mut best_count = 1usize;
    for (_k, (v, c)) in counts.iter() {
        if *c > best_count {
            best_count = *c;
            best_value = Some(*v);
        }
    }
    best_value
}

/// Linear (perceptron-style) percentile. `p` in [0, 100].
/// Returns `None` for empty slices.
pub fn percentile(samples: &[f64], p: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    if !(0.0..=100.0).contains(&p) {
        return None;
    }
    let mut sorted: Vec<f64> = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if p == 100.0 {
        return Some(*sorted.last().unwrap());
    }
    if p == 0.0 {
        return Some(*sorted.first().unwrap());
    }
    let n = sorted.len() as f64;
    let rank = (p / 100.0) * (n - 1.0);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        return Some(sorted[lo]);
    }
    let frac = rank - lo as f64;
    let a = sorted[lo];
    let b = sorted[hi];
    Some(a + (b - a) * frac)
}

/// Sum of all samples.
pub fn sum(samples: &[f64]) -> f64 {
    samples.iter().sum()
}

/// Minimum sample, or `None` if empty.
pub fn min(samples: &[f64]) -> Option<f64> {
    samples.iter().copied().reduce(f64::min)
}

/// Maximum sample, or `None` if empty.
pub fn max(samples: &[f64]) -> Option<f64> {
    samples.iter().copied().reduce(f64::max)
}

/// Pearson correlation coefficient between two equal-length samples.
/// Returns `None` if lengths differ or either is constant (zero variance).
pub fn correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() != b.len() || a.len() < 2 {
        return None;
    }
    let ma = mean(a)?;
    let mb = mean(b)?;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for i in 0..a.len() {
        let x = a[i] - ma;
        let y = b[i] - mb;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    let denom = (da * db).sqrt();
    if denom == 0.0 {
        return None;
    }
    Some(num / denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 1e-9,
            "{} ≈ {} (diff {})",
            a,
            b,
            (a - b).abs()
        );
    }

    #[test]
    fn mean_basic() {
        assert_eq!(mean(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some(3.0));
        assert_eq!(mean(&[]), None);
        assert_eq!(mean(&[42.0]), Some(42.0));
    }

    #[test]
    fn variance_basic() {
        // Variance of [1,2,3,4,5] = 2.5 (sample, Bessel-corrected).
        assert_eq!(variance(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some(2.5));
        assert_eq!(variance(&[5.0]), None);
        assert_eq!(variance(&[]), None);
    }

    #[test]
    fn stddev_basic() {
        let sd = stddev(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        approx(sd, 2.138089935299495);
    }

    #[test]
    fn median_odd_count() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[5.0]), Some(5.0));
    }

    #[test]
    fn median_even_count() {
        // Median of [1,2,3,4] = 2.5
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }

    #[test]
    fn median_empty() {
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn percentile_basic() {
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.0), Some(1.0));
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 25.0), Some(2.0));
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 50.0), Some(3.0));
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 75.0), Some(4.0));
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 100.0), Some(5.0));
    }

    #[test]
    fn percentile_interpolation() {
        // Median of even count is the avg of two middle values.
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 50.0), Some(2.5));
        // Q1 = 1.5 for [1,2,3,4,5,6]
        assert_eq!(
            percentile(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 25.0),
            Some(2.25)
        );
    }

    #[test]
    fn mode_basic() {
        assert_eq!(mode(&[1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0]), Some(3.0));
        assert_eq!(mode(&[1.0]), None); // Single value has no "most frequent"
    }

    #[test]
    fn min_max_basic() {
        assert_eq!(min(&[3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]), Some(1.0));
        assert_eq!(max(&[3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]), Some(9.0));
        assert_eq!(min(&[]), None);
        assert_eq!(max(&[]), None);
    }

    #[test]
    fn sum_basic() {
        assert_eq!(sum(&[1.0, 2.0, 3.0]), 6.0);
        assert_eq!(sum(&[]), 0.0);
    }

    #[test]
    fn correlation_perfect_positive() {
        let x = &[1.0, 2.0, 3.0, 4.0, 5.0];
        let y = &[2.0, 4.0, 6.0, 8.0, 10.0];
        approx(correlation(x, y).unwrap(), 1.0);
    }

    #[test]
    fn correlation_perfect_negative() {
        let x = &[1.0, 2.0, 3.0, 4.0, 5.0];
        let y = &[5.0, 4.0, 3.0, 2.0, 1.0];
        approx(correlation(x, y).unwrap(), -1.0);
    }

    #[test]
    fn correlation_undefined_for_constant() {
        let x = &[1.0, 1.0, 1.0, 1.0];
        let y = &[1.0, 2.0, 3.0, 4.0];
        assert_eq!(correlation(x, y), None);
    }

    #[test]
    fn correlation_length_mismatch() {
        let x = &[1.0, 2.0];
        let y = &[1.0, 2.0, 3.0];
        assert_eq!(correlation(x, y), None);
    }
}
