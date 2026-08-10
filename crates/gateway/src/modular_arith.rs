//! Modular arithmetic helpers for `u64`.
//!
//! Small utility set: GCD, LCM, modular exponentiation, modular inverse,
//! and Euler's totient. All inputs are non-negative `u64`; the modulus
//! `m` must be `> 1` for [`mod_inverse`].
//!
//! Time: GCD/mul all O(log n).

/// Greatest common divisor (Euclidean algorithm). Returns `a` if `b == 0`.
pub fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Least common multiple. Returns 0 if either input is 0.
pub fn lcm(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 {
        0
    } else {
        a / gcd(a, b) * b
    }
}

/// Modular exponentiation: `(base^exp) mod m` using repeated squaring.
/// Returns 0 if `m == 0` or `1` if `m == 1`.
pub fn pow_mod(mut base: u64, mut exp: u64, m: u64) -> u64 {
    if m == 1 {
        return 0;
    }
    let mut result: u64 = 1;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mul_mod(result, base, m);
        }
        exp >>= 1;
        base = mul_mod(base, base, m);
    }
    result
}

/// Modular multiplication using Russian-peasant (a*b) % m without
/// overflow on `a*b` when `a, b < m < 2^32`. (For 64-bit moduli, use
/// built-in `((a as u128 * b as u128) % m as u128) as u64` if you
/// need fewer ops at a cost of one u128 mul.)
pub fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    if m == 0 {
        return 0;
    }
    let mut result: u64 = 0;
    let mut a = a % m;
    let mut b = b;
    while b > 0 {
        if b & 1 == 1 {
            result = (result + a) % m;
        }
        a = (a * 2) % m;
        b >>= 1;
    }
    result
}

/// Modular inverse of `a` modulo `m` via extended Euclidean algorithm.
/// Returns `None` if `a` and `m` are not coprime (gcd != 1) or if
/// `m <= 1`.
pub fn mod_inverse(a: u64, m: u64) -> Option<u64> {
    if m <= 1 {
        return None;
    }
    let (mut old_r, mut r) = (a % m, m);
    let (mut old_s, mut s) = (1i128, 0i128);
    while r != 0 {
        let q = old_r / r;
        let new_r = old_r - q * r;
        old_r = r;
        r = new_r;
        let new_s = old_s - (q as i128) * s;
        old_s = s;
        s = new_s;
    }
    if old_r != 1 {
        return None;
    }
    let inv = old_s.rem_euclid(m as i128) as u64;
    Some(inv)
}

/// Euler's totient function φ(n) — count of integers in [1, n] coprime to n.
pub fn euler_totient(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut result = n;
    let mut m = n;
    let mut p = 2u64;
    while p * p <= m {
        if m % p == 0 {
            while m % p == 0 {
                m /= p;
            }
            result -= result / p;
        }
        p += 1;
    }
    if m > 1 {
        result -= result / m;
    }
    result
}

/// China Remainder Theorem: solve `x ≡ r1 (mod m1)` and `x ≡ r2 (mod m2)`.
/// Returns `Some((r, m))` where x ≡ r (mod m), with m = lcm(m1, m2).
/// Returns `None` if the moduli are not coprime and have inconsistent
/// remainders.
pub fn crt(r1: u64, m1: u64, r2: u64, m2: u64) -> Option<(u64, u64)> {
    if m1 == 0 || m2 == 0 {
        return None;
    }
    let g = gcd(m1, m2);
    if (r1 % g) != (r2 % g) {
        return None;
    }
    let r1 = r1 % m1;
    let r2 = r2 % m2;
    if r1 == 0 && r2 == 0 {
        return Some((0, (m1 / g) * m2));
    }
    // Find inverse of m1/g modulo m2/g.
    let lcm = (m1 / g) * m2;
    let m2g = m2 / g;
    let k = mod_inverse(m1 / g, m2g)?;
    let diff = (r2 as i128) - (r1 as i128);
    let kk = (k as i128).rem_euclid(m2g as i128);
    let delta = (diff * kk).rem_euclid(m2g as i128) as u64;
    let result = (r1 + m1 * delta) % lcm;
    Some((result, lcm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcd_basic() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(100, 75), 25);
        assert_eq!(gcd(17, 13), 1);
        assert_eq!(gcd(0, 5), 5);
        assert_eq!(gcd(5, 0), 5);
        assert_eq!(gcd(0, 0), 0);
    }

    #[test]
    fn lcm_basic() {
        assert_eq!(lcm(4, 6), 12);
        assert_eq!(lcm(17, 13), 221);
        assert_eq!(lcm(0, 5), 0);
    }

    #[test]
    fn pow_mod_basic() {
        // Fermat's little theorem: 2^10 mod 1000 = 24.
        assert_eq!(pow_mod(2, 10, 1000), 24);
        // 0^0 = 1 (standard).
        assert_eq!(pow_mod(0, 0, 7), 1);
        // 0^5 = 0.
        assert_eq!(pow_mod(0, 5, 7), 0);
        // Modulus 1 returns 0 for any input.
        assert_eq!(pow_mod(3, 5, 1), 0);
        // 5^0 = 1.
        assert_eq!(pow_mod(5, 0, 13), 1);
    }

    #[test]
    fn mul_mod_basic() {
        assert_eq!(mul_mod(7, 11, 13), 77 % 13);
        assert_eq!(mul_mod(0, 5, 13), 0);
        assert_eq!(mul_mod(12, 11, 13), (12 * 11) % 13);
    }

    #[test]
    fn mod_inverse_basic() {
        // 3 * 5 = 15 ≡ 1 (mod 7).
        assert_eq!(mod_inverse(3, 7), Some(5));
        // 2 has no inverse mod 4 (gcd=2).
        assert_eq!(mod_inverse(2, 4), None);
        // 3 * 4 = 12 ≡ 1 (mod 11).
        assert_eq!(mod_inverse(3, 11), Some(4));
        // Modulus 1 returns None.
        assert_eq!(mod_inverse(2, 1), None);
    }

    #[test]
    fn euler_totient_basic() {
        assert_eq!(euler_totient(1), 1);
        assert_eq!(euler_totient(2), 1);
        assert_eq!(euler_totient(10), 4);
        assert_eq!(euler_totient(36), 12);
        assert_eq!(euler_totient(97), 96);
    }

    #[test]
    fn crt_basic() {
        // x ≡ 2 (mod 3), x ≡ 3 (mod 5) → x = 8 mod 15
        let (r, m) = crt(2, 3, 3, 5).unwrap();
        assert_eq!(r, 8);
        assert_eq!(m, 15);
        assert_eq!(8 % 3, 2);
        assert_eq!(8 % 5, 3);
    }

    #[test]
    fn crt_inconsistent_returns_none() {
        // x ≡ 0 (mod 4), x ≡ 2 (mod 4) — same modulus, different residue
        // (gcd = 4 != 0): inconsistent.
        assert!(crt(0, 4, 2, 4).is_none());
    }

    #[test]
    fn crt_coprime() {
        // x ≡ 1 (mod 7), x ≡ 1 (mod 11) → x = 1.
        let (r, m) = crt(1, 7, 1, 11).unwrap();
        assert_eq!(r, 1);
        assert_eq!(m, 77);
    }
}
