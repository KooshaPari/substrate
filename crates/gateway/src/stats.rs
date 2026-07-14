pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}
pub fn variance(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / values.len() as f64
}
pub fn stddev(values: &[f64]) -> f64 {
    variance(values).sqrt()
}
pub fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mean_basic() {
        assert!((mean(&[1.0, 2.0, 3.0]) - 2.0).abs() < 1e-9);
    }
    #[test]
    fn variance_basic() {
        assert!((variance(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]) - 4.0).abs() < 1e-9);
    }
    #[test]
    fn stddev_basic() {
        let s = stddev(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(s > 1.4 && s < 1.6);
    }
    #[test]
    fn median_odd() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), 2.0);
    }
    #[test]
    fn median_even() {
        assert_eq!(median(vec![1.0, 2.0, 3.0, 4.0, 5.0]), 3.0);
    }
}
