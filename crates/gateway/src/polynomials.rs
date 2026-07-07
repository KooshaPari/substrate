//! Polynomial arithmetic over `i64` coefficients.
//!
//! Useful for cyclic-redundancy check generators (CRC-8, CRC-16), Reed-
//! Solomon decoder implementations, and other code that needs exact
//! polynomial multiplication without modular arithmetic.
//!
//! Operations:
//! - [`polymul`] — exact polynomial multiplication
//! - [`polyadd`] — coefficient-wise addition
//! - [`polydivmod`] — Euclidean division (quotient + remainder)
//! - [`polyeval`] — Horner evaluation
//!
//! Coefficients are stored as `i64`s; for byte-oriented uses (CRCs)
//! cast down. The implementation is intended for short polynomials
//! (degree < 32 or so) — `O(n*m)` multiply is fine for hundreds of
//! coefficients but not for thousands.

/// Coefficients are stored low-degree-first. `p[0]` is the constant
/// term, `p[n]` is the coefficient of `x^n`.
///
/// Polynomial multiplication. `deg(p) + deg(q) + 1` coefficients returned.
///
/// Examples:
/// - polymul(&[1, 2], &[3, 4]) = [3, 10, 8]   // (1 + 2x)(3 + 4x) = 3 + 10x + 8x^2
/// - polymul(&[1, 1], &[1, 1]) = [1, 2, 1]   // (1 + x)^2 = 1 + 2x + x^2
pub fn polymul(p: &[i64], q: &[i64]) -> Vec<i64> {
    if p.is_empty() || q.is_empty() {
        return vec![0];
    }
    let mut out = vec![0i64; p.len() + q.len() - 1];
    for (i, &a) in p.iter().enumerate() {
        for (j, &b) in q.iter().enumerate() {
            out[i + j] += a * b;
        }
    }
    out
}

/// Coefficient-wise polynomial addition. Output length = max(p.len(), q.len()).
///
/// Examples:
/// - polyadd(&[1, 2, 3], &[10, 20]) = [11, 22, 3]
pub fn polyadd(p: &[i64], q: &[i64]) -> Vec<i64> {
    let n = p.len().max(q.len());
    let mut out = vec![0i64; n];
    for i in 0..n {
        let a = p.get(i).copied().unwrap_or(0);
        let b = q.get(i).copied().unwrap_or(0);
        out[i] = a + b;
    }
    out
}

/// Euclidean polynomial division. Returns (quotient, remainder) such
/// that `p = q * quotient + remainder` with `deg(remainder) < deg(q)`.
///
/// Panics if `q` is empty or all-zero.
pub fn polydivmod(p: &[i64], q: &[i64]) -> (Vec<i64>, Vec<i64>) {
    if q.is_empty() || q.iter().all(|&x| x == 0) {
        panic!("polydivmod called with zero divisor");
    }
    let mut p = p.to_vec();
    let mut q_out = vec![0i64; p.len()];
    while !p.is_empty() {
        let lead_p = *p.last().unwrap();
        let lead_q = *q.last().unwrap();
        if lead_p == 0 {
            p.pop();
            continue;
        }
        // Check if degree(q) > degree(p) and stop if so
        if p.len() < q.len() {
            break;
        }
        let k = lead_p / lead_q;
        let shift = p.len() - q.len();
        q_out[shift] = k;
        for i in 0..q.len() {
            p[shift + i] -= k * q[i];
        }
        p.pop();
        // Strip trailing zeros from p
        while p.last() == Some(&0) {
            p.pop();
        }
    }
    // Strip leading zeros from quotient
    while q_out.last() == Some(&0) {
        q_out.pop();
    }
    if q_out.is_empty() {
        q_out = vec![0];
    }
    (q_out, p)
}

/// Horner's-method polynomial evaluation at `x`.
///
/// Examples:
/// - polyeval(&[1, 2, 3], 5) = 1 + 2*5 + 3*25 = 86
pub fn polyeval(p: &[i64], x: i64) -> i64 {
    p.iter().rev().fold(0i64, |acc, &c| acc * x + c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polymul_basic() {
        assert_eq!(polymul(&[1, 2], &[3, 4]), vec![3, 10, 8]);
        assert_eq!(polymul(&[1, 1], &[1, 1]), vec![1, 2, 1]);
    }

    #[test]
    fn polymul_by_zero() {
        assert_eq!(polymul(&[1, 2], &[0]), vec![0]);
    }

    #[test]
    fn polyadd_basic() {
        assert_eq!(polyadd(&[1, 2, 3], &[10, 20]), vec![11, 22, 3]);
        assert_eq!(polyadd(&[1, 2], &[3, 4, 5]), vec![4, 6, 5]);
    }

    #[test]
    fn polydivmod_exact() {
        // (x^2 + 3x + 2) / (x + 1) = x + 2 with remainder 0
        let (q, r) = polydivmod(&[2, 3, 1], &[1, 1]);
        assert_eq!(q, vec![2, 1]);
        assert_eq!(r, vec![0]);
    }

    #[test]
    fn polydivmod_with_remainder() {
        // (x^2 + 1) / (x + 1) = x - 1 with remainder 2
        let (q, r) = polydivmod(&[1, 0, 1], &[1, 1]);
        assert_eq!(q, vec![-1, 1]);
        assert_eq!(r, vec![2]);
    }

    #[test]
    fn polyeval_horner() {
        assert_eq!(polyeval(&[1, 2, 3], 5), 86);
        assert_eq!(polyeval(&[5], 100), 5); // constant
        assert_eq!(polyeval(&[1, 1], 7), 8); // 1 + 7
    }

    #[test]
    fn polymul_round_trip_with_divmod() {
        // Build p = q * r, then verify division recovers r
        let q = [1i64, 1, 1]; // 1 + x + x^2
        let r = [2i64, 3]; // 2 + 3x
        let p = polymul(&q, &r);
        let (qout, rout) = polydivmod(&p, &q);
        assert_eq!(qout, q);
        assert_eq!(rout, r);
    }
}