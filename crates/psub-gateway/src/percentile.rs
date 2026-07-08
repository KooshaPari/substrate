pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
pub fn median(sorted: &[f64]) -> f64 { percentile(sorted, 50.0) }
pub fn p50(sorted: &[f64]) -> f64 { percentile(sorted, 50.0) }
pub fn p95(sorted: &[f64]) -> f64 { percentile(sorted, 95.0) }
pub fn p99(sorted: &[f64]) -> f64 { percentile(sorted, 99.0) }
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() { return 0.0; }
    values.iter().sum::<f64>() / values.len() as f64
}
pub fn stddev(values: &[f64]) -> f64 {
    if values.len() < 2 { return 0.0; }
    let m = mean(values);
    let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_p50_odd() { assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 50.0), 3.0); }
    #[test] fn test_p95_large() { let mut v: Vec<f64> = (1..=100).map(|i| i as f64).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); assert_eq!(percentile(&v, 95.0), 95.0); }
    #[test] fn test_median() { assert_eq!(median(&[1.0, 2.0, 3.0, 4.0, 5.0]), 3.0); }
    #[test] fn test_empty() { assert_eq!(percentile(&[], 50.0), 0.0); assert_eq!(mean(&[]), 0.0); }
    #[test] fn test_mean() { assert!((mean(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-9); }
    #[test] fn test_stddev() { assert!(stddev(&[1.0, 2.0, 3.0, 4.0, 5.0]) > 1.4 && stddev(&[1.0, 2.0, 3.0, 4.0, 5.0]) < 1.5); }
}
