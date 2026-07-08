//! Random number distributions over `f64` samples.
//!
//! Each function takes a uniform source `u: impl FnMut() -> f64` in
//! [0, 1) and returns samples from the named distribution. Use with
//! any RNG that yields uniform doubles — `rand::random()`,
//! `fastrand::f64()`, a custom PRNG, etc.
//!
//! Implemented distributions:
//! - [`uniform_f64`] / [`uniform_int`] — flat in [lo, hi) / [lo, hi].
//! - [`normal`] — standard normal via Box-Muller transform.
//! - [`exponential`] — exponential with mean 1/lambda.
//! - [`bernoulli`] — 0/1 with success probability p.
//! - [`poisson`] — Poisson via Knuth's algorithm (small lambda).
//! - [`cauchy`] — Cauchy/Lorentz (heavy-tailed).

/// Flat real uniform in [lo, hi).
pub fn uniform_f64<R: FnMut() -> f64>(u: &mut R, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * u()
}

/// Flat integer uniform in [lo, hi] (inclusive on both ends).
pub fn uniform_int<R: FnMut() -> f64>(u: &mut R, lo: i64, hi: i64) -> i64 {
    let span = (hi - lo + 1) as f64;
    lo + (u() * span).floor() as i64
}

/// Bernoulli trial: returns 1 with probability `p`, else 0.
pub fn bernoulli<R: FnMut() -> f64>(u: &mut R, p: f64) -> u8 {
    if u() < p {
        1
    } else {
        0
    }
}

/// Exponential(λ) sample with mean 1/λ. λ > 0.
/// Standard inverse-CDF: -ln(1-u) / λ.
pub fn exponential<R: FnMut() -> f64>(u: &mut R, lambda: f64) -> f64 {
    assert!(lambda > 0.0, "exponential lambda must be > 0");
    let mut x = u();
    // Avoid log(0); for u close to 1, sample again (extremely rare).
    if x >= 1.0 {
        x = 0.999_999_999_999_999;
    }
    -(1.0 - x).ln() / lambda
}

/// Standard normal (μ=0, σ=1) sample via Box-Muller polar.
pub fn normal<R: FnMut() -> f64>(u: &mut R) -> f64 {
    loop {
        let u1 = u();
        let u2 = u();
        let r = (-2.0 * u1.max(1e-15).ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        let x = r * theta.cos();
        // Reject r outside the unit disk for polar Box-Muller? Plain
        // Box-Muller accepts all (r, theta); we use the alternate
        // formulation and return one of the two samples per call.
        let _ = r * theta.sin(); // could be returned for efficiency
        return x;
    }
}

/// Standard normal samples in pairs (saves one call to u()). The
/// returned tuple is `(n1, n2)`; both are independent standard normals.
pub fn normal_pair<R: FnMut() -> f64>(u: &mut R) -> (f64, f64) {
    let u1 = u().max(1e-15);
    let u2 = u();
    let r = (-2.0 * u1.ln()).sqrt();
    let theta = 2.0 * std::f64::consts::PI * u2;
    (r * theta.cos(), r * theta.sin())
}

/// Cauchy/Lorentz distribution (heavy-tailed). γ > 0 is the scale.
pub fn cauchy<R: FnMut() -> f64>(u: &mut R, gamma: f64) -> f64 {
    assert!(gamma > 0.0, "cauchy gamma must be > 0");
    gamma * (std::f64::consts::PI * (u() - 0.5)).tan()
}

/// Poisson(λ) sample using Knuth's algorithm (good for small λ).
/// For λ > 30, normal approximation is more efficient.
pub fn poisson<R: FnMut() -> f64>(u: &mut R, lambda: f64) -> u64 {
    assert!(lambda >= 0.0, "poisson lambda must be >= 0");
    if lambda < 30.0 {
        // Knuth's algorithm.
        let l = (-lambda).exp();
        let mut k = 0u64;
        let mut p = 1.0f64;
        loop {
            k += 1;
            p *= u();
            if p <= l {
                return k - 1;
            }
        }
    } else {
        // Normal approximation: λ large, sqrt(λ) * normal + λ.
        let n = normal(u);
        (lambda + n * lambda.sqrt()).max(0.0).round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic xorshift64*-based PRNG exposed as a `FnMut() -> f64`.
    /// Used by tests for reproducibility; production code should use a
    /// cryptographically secure RNG.
    fn make_rng(seed: u64) -> impl FnMut() -> f64 {
        let mut state = seed | 1;
        move || {
            let mut x = state;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            state = x;
            ((x >> 11) as f64) / ((1u64 << 53) as f64)
        }
    }

    fn approx(a: f64, b: f64, eps: f64) {
        assert!((a - b).abs() < eps, "{} ≈ {} (|diff|={})", a, b, (a - b).abs());
    }

    #[test]
    fn uniform_basic() {
        let mut r = make_rng(42);
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for _ in 0..10_000 {
            let v = uniform_f64(&mut r, 5.0, 10.0);
            if v < min { min = v; }
            if v > max { max = v; }
            assert!(v >= 5.0 && v < 10.0);
        }
        assert!(min < 5.5);
        assert!(max > 9.5);
    }

    #[test]
    fn uniform_int_basic() {
        let mut r = make_rng(7);
        for _ in 0..1000 {
            let v = uniform_int(&mut r, 1, 6); // die roll
            assert!(v >= 1 && v <= 6);
        }
    }

    #[test]
    fn bernoulli_distribution() {
        let mut r = make_rng(123);
        let mut ones = 0;
        let n = 10_000;
        for _ in 0..n {
            ones += bernoulli(&mut r, 0.3) as u64;
        }
        let observed = ones as f64 / n as f64;
        approx(observed, 0.3, 0.02);
    }

    #[test]
    fn exponential_mean() {
        let mut r = make_rng(99);
        let lambda = 2.0;
        let n = 10_000;
        let sum: f64 = (0..n).map(|_| exponential(&mut r, lambda)).sum();
        let mean = sum / n as f64;
        approx(mean, 1.0 / lambda, 0.05);
    }

    #[test]
    fn normal_zero_mean_unit_var() {
        let mut r = make_rng(31);
        let n = 20_000;
        let sum: f64 = (0..n).map(|_| normal(&mut r)).sum();
        let mean = sum / n as f64;
        let var_sum: f64 = (0..n)
            .map(|_| {
                let x = normal(&mut r);
                (x - mean).powi(2)
            })
            .sum();
        let var = var_sum / (n as f64 - 1.0);
        approx(mean, 0.0, 0.05);
        approx(var, 1.0, 0.1);
    }

    #[test]
    fn normal_pair_basic() {
        let mut r = make_rng(31);
        let (a, b) = normal_pair(&mut r);
        // Both are finite and not NaN.
        assert!(a.is_finite() && b.is_finite());
    }

    #[test]
    fn cauchy_heavy_tailed() {
        let mut r = make_rng(13);
        let mut max = 0.0_f64;
        for _ in 0..10_000 {
            let v = cauchy(&mut r, 1.0).abs();
            if v > max { max = v; }
        }
        // Cauchy has heavy tails — 10k samples should produce at least
        // one sample with magnitude > 100.
        assert!(max > 100.0);
    }

    #[test]
    fn poisson_small_lambda() {
        let mut r = make_rng(57);
        let lambda = 3.0;
        let n = 10_000;
        let sum: u64 = (0..n).map(|_| poisson(&mut r, lambda)).sum();
        let mean = sum as f64 / n as f64;
        approx(mean, lambda, 0.1);
    }
}