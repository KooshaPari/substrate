//! Black-Scholes option pricing model.
//!
//! Analytical formulas for European call and put option prices, the four
//! first-order Greeks (delta, gamma, theta, vega), and rho. The model assumes
//! a frictionless market, log-normal underlying returns, and constant
//! risk-free rate and volatility.
//!
//! Formulas:
//! - d1 = (ln(S/K) + (r + σ²/2)·T) / (σ·√T)
//! - d2 = d1 - σ·√T
//! - Call = S·N(d1) - K·e^(-r·T)·N(d2)
//! - Put  = K·e^(-r·T)·N(-d2) - S·N(-d1)
//!
//! where N(·) is the standard normal CDF.

use std::f64::consts::PI;

/// Abramowitz & Stegun rational approximation of the standard normal CDF.
/// Max error ~7.5e-8.
pub fn norm_cdf(x: f64) -> f64 {
    // Constants
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = x.abs();
    let t = 1.0 / (1.0 + p * abs_x);
    let y =
        1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-abs_x * abs_x / 2.0).exp();
    0.5 * (1.0 + sign * y)
}

/// Standard normal probability density function.
pub fn norm_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * PI).sqrt()
}

/// Inputs to the Black-Scholes pricer.
#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    /// Current underlying price (S > 0).
    pub spot: f64,
    /// Strike price (K > 0).
    pub strike: f64,
    /// Time to expiration in years (T > 0).
    pub time: f64,
    /// Risk-free interest rate (annualised, decimal — 0.05 = 5%).
    pub rate: f64,
    /// Volatility of the underlying (annualised, decimal — 0.20 = 20%).
    pub volatility: f64,
}

/// All first-order Black-Scholes outputs in one struct.
#[derive(Debug, Clone, Copy)]
pub struct Outputs {
    pub call: f64,
    pub put: f64,
    pub call_delta: f64,
    pub put_delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub call_theta: f64,
    pub put_theta: f64,
    pub call_rho: f64,
    pub put_rho: f64,
    pub d1: f64,
    pub d2: f64,
}

/// Compute d1 and d2 for the Black-Scholes model.
fn d1_d2(i: &Inputs) -> (f64, f64) {
    let sqrt_t = i.time.sqrt();
    let sigma_sqrt_t = i.volatility * sqrt_t;
    let d1 = ((i.spot / i.strike).ln() + (i.rate + 0.5 * i.volatility * i.volatility) * i.time)
        / sigma_sqrt_t;
    let d2 = d1 - sigma_sqrt_t;
    (d1, d2)
}

/// Compute European call and put prices plus the standard Greeks.
pub fn price(i: &Inputs) -> Outputs {
    let (d1, d2) = d1_d2(i);
    let nd1 = norm_cdf(d1);
    let nd2 = norm_cdf(d2);
    let npd1 = norm_cdf(-d1);
    let npd2 = norm_cdf(-d2);
    let disc = (-i.rate * i.time).exp();
    let sqrt_t = i.time.sqrt();
    let pdf_d1 = norm_pdf(d1);

    let call = i.spot * nd1 - i.strike * disc * nd2;
    let put = i.strike * disc * npd2 - i.spot * npd1;

    // Greeks (per 1.00 unit of underlying / per year / per 1% vol / per 1% rate).
    let gamma = pdf_d1 / (i.spot * i.volatility * sqrt_t);
    let vega = i.spot * pdf_d1 * sqrt_t / 100.0;
    let call_delta = nd1;
    let put_delta = nd1 - 1.0;
    let call_theta =
        (-i.spot * pdf_d1 * i.volatility / (2.0 * sqrt_t) - i.rate * i.strike * disc * nd2) / 365.0;
    let put_theta = (-i.spot * pdf_d1 * i.volatility / (2.0 * sqrt_t)
        + i.rate * i.strike * disc * npd2)
        / 365.0;
    let call_rho = i.strike * i.time * disc * nd2 / 100.0;
    let put_rho = -i.strike * i.time * disc * npd2 / 100.0;

    Outputs {
        call,
        put,
        call_delta,
        put_delta,
        gamma,
        vega,
        call_theta,
        put_theta,
        call_rho,
        put_rho,
        d1,
        d2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) {
        assert!(
            (a - b).abs() < eps,
            "expected {} ≈ {} (|diff| = {})",
            a,
            b,
            (a - b).abs()
        );
    }

    #[test]
    fn norm_cdf_at_zero() {
        approx(norm_cdf(0.0), 0.5, 1e-6);
    }

    #[test]
    fn norm_cdf_symmetry() {
        // Φ(-x) = 1 - Φ(x)
        for x in [0.5, 1.0, 1.5, 2.0, 3.0] {
            approx(norm_cdf(-x) + norm_cdf(x), 1.0, 1e-6);
        }
    }

    #[test]
    fn norm_cdf_extremes() {
        assert!(norm_cdf(-5.0) < 1e-6);
        assert!(norm_cdf(5.0) > 0.999_999);
    }

    #[test]
    fn norm_pdf_at_zero() {
        // φ(0) = 1/√(2π) ≈ 0.3989423
        approx(norm_pdf(0.0), 0.3989423, 1e-6);
    }

    #[test]
    fn atm_call_put_parity() {
        // Put-call parity: C - P = S - K·e^(-r·T)
        let i = Inputs {
            spot: 100.0,
            strike: 100.0,
            time: 1.0,
            rate: 0.05,
            volatility: 0.20,
        };
        let o = price(&i);
        let parity_lhs = o.call - o.put;
        let parity_rhs = i.spot - i.strike * (-i.rate * i.time).exp();
        approx(parity_lhs, parity_rhs, 1e-6);
    }

    #[test]
    fn deep_itm_call() {
        // S way above K, low vol, short T -> call ≈ S - K
        let i = Inputs {
            spot: 200.0,
            strike: 100.0,
            time: 0.01,
            rate: 0.05,
            volatility: 0.05,
        };
        let o = price(&i);
        approx(o.call, 200.0 - 100.0 * (-(0.05_f64) * 0.01).exp(), 0.5);
    }

    #[test]
    fn deep_otm_call_zero() {
        // S way below K, short T -> call should be ~0
        let i = Inputs {
            spot: 1.0,
            strike: 100.0,
            time: 0.01,
            rate: 0.05,
            volatility: 0.05,
        };
        let o = price(&i);
        assert!(o.call < 0.01, "deep OTM call should be ~0, got {}", o.call);
        assert!(o.put > 0.0, "deep ITM put should be > 0, got {}", o.put);
    }

    #[test]
    fn call_delta_in_range() {
        let i = Inputs {
            spot: 100.0,
            strike: 100.0,
            time: 1.0,
            rate: 0.05,
            volatility: 0.20,
        };
        let o = price(&i);
        assert!(o.call_delta > 0.0 && o.call_delta < 1.0);
        assert!(o.put_delta < 0.0 && o.put_delta > -1.0);
        // call_delta - put_delta = 1
        approx(o.call_delta - o.put_delta, 1.0, 1e-9);
    }

    #[test]
    fn gamma_positive() {
        let i = Inputs {
            spot: 100.0,
            strike: 100.0,
            time: 1.0,
            rate: 0.05,
            volatility: 0.20,
        };
        let o = price(&i);
        assert!(o.gamma > 0.0, "gamma should be positive, got {}", o.gamma);
    }

    #[test]
    fn vega_positive() {
        let i = Inputs {
            spot: 100.0,
            strike: 100.0,
            time: 1.0,
            rate: 0.05,
            volatility: 0.20,
        };
        let o = price(&i);
        assert!(o.vega > 0.0, "vega should be positive, got {}", o.vega);
    }
}
