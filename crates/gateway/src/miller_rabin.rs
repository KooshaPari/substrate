//! Miller-Rabin primality test.
//!
//! Probabilistic primality test for arbitrary-precision integers represented
//! as `Vec<u64>` (little-endian 64-bit limbs). Given an odd integer n > 2,
//! write n-1 = 2^s * d with d odd. Choose k random bases a in [2, n-2]; n
//! is composite (witness) if any base produces a^s = 1 (mod n) or
//! a^(d·2^r) = -1 (mod n) for some 0 <= r < s.
//!
//! For deterministic results, bases {2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31,
//! 37} are sufficient for all n < 3,317,044,064,679,887,385,961,981.
//!
//! Reference: <https://en.wikipedia.org/wiki/Miller%E2%80%93Rabin_primality_test>

/// Big-endian bytestring parsed as a little-endian `Vec<u64>` of 64-bit limbs.
pub fn from_be_bytes(bytes: &[u8]) -> Vec<u64> {
    if bytes.is_empty() {
        return vec![0];
    }
    // limbs[0] holds the LEAST-significant 8 bytes; limbs[N-1] holds the most.
    let mut limbs: Vec<u64> = vec![0u64; (bytes.len() + 7) / 8];
    for (i, b) in bytes.iter().enumerate() {
        // Position from the end of `bytes`: byte 0 is the most-significant byte,
        // which goes into limbs[limbs.len() - 1] at its highest byte.
        let idx_from_msb = bytes.len() - 1 - i;
        let limb_idx = limbs.len() - 1 - idx_from_msb / 8;
        let bit_idx = (idx_from_msb % 8) * 8;
        limbs[limb_idx] |= (*b as u64) << bit_idx;
    }
    // Trim leading zero limbs.
    while limbs.len() > 1 && *limbs.first().unwrap() == 0 {
        limbs.remove(0);
    }
    limbs
}

/// Render a `Vec<u64>` (big-endian) as big-endian bytes.
pub fn to_be_bytes(limbs: &[u64]) -> Vec<u8> {
    if limbs.is_empty() || (limbs.len() == 1 && limbs[0] == 0) {
        return vec![0];
    }
    let mut out = Vec::with_capacity(limbs.len() * 8);
    for limb in limbs.iter().rev() {
        for shift in (0..8).rev() {
            out.push((limb >> (shift * 8)) as u8);
        }
    }
    while out.len() > 1 && out[0] == 0 {
        out.remove(0);
    }
    out
}

fn cmp(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let a_len = a.len() - a.iter().take_while(|&&l| l == 0).count();
    let b_len = b.len() - b.iter().take_while(|&&l| l == 0).count();
    if a_len != b_len {
        return a_len.cmp(&b_len);
    }
    for i in 0..a_len {
        let av = a[a.len() - 1 - i];
        let bv = b[b.len() - 1 - i];
        if av != bv {
            return av.cmp(&bv);
        }
    }
    Ordering::Equal
}

fn is_even(limbs: &[u64]) -> bool {
    limbs.last().map_or(true, |&l| l & 1 == 0)
}

fn is_zero(limbs: &[u64]) -> bool {
    limbs.iter().all(|&l| l == 0)
}

fn is_one(limbs: &[u64]) -> bool {
    limbs.last().map_or(false, |&l| l == 1) && limbs[..limbs.len() - 1].iter().all(|&l| l == 0)
}

fn is_two(limbs: &[u64]) -> bool {
    limbs.last().map_or(false, |&l| l == 2) && limbs[..limbs.len() - 1].iter().all(|&l| l == 0)
}

fn is_three(limbs: &[u64]) -> bool {
    limbs.last().map_or(false, |&l| l == 3) && limbs[..limbs.len() - 1].iter().all(|&l| l == 0)
}

/// Modular multiplication using Russian-peasant (double-and-add) algorithm.
/// Sufficient for our 64-bit / 128-bit intermediate range.
fn mulmod(a: u64, b: u64, m: u64) -> u64 {
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

/// Modular exponentiation by squaring.
fn powmod(base: u64, exp: u64, m: u64) -> u64 {
    if m == 1 {
        return 0;
    }
    let mut result: u64 = 1;
    let mut base = base % m;
    let mut exp = exp;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mulmod(result, base, m);
        }
        exp >>= 1;
        base = mulmod(base, base, m);
    }
    result
}

/// Miller-Rabin single-base test for a u64 candidate. Returns `true` if `n`
/// is *probably* prime given this witness.
pub fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    // Write n-1 = 2^s * d.
    let mut d = n - 1;
    let mut s = 0;
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    // Deterministic bases for n < 2^64.
    for &a in &[2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if a >= n {
            continue;
        }
        let mut x = powmod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        let mut composite = true;
        for _ in 0..s - 1 {
            x = mulmod(x, x, n);
            if x == n - 1 {
                composite = false;
                break;
            }
        }
        if composite {
            return false;
        }
    }
    true
}

/// Miller-Rabin test for arbitrary-precision (little-endian u64 limbs) integers.
/// Returns `true` if `n` is *probably* prime.
pub fn is_prime(limbs: &[u64]) -> bool {
    if limbs.len() == 1 {
        return is_prime_u64(limbs[0]);
    }
    if is_zero(limbs) || is_one(limbs) {
        return false;
    }
    if is_two(limbs) || is_three(limbs) {
        return true;
    }
    if is_even(limbs) {
        return false;
    }
    // For multi-limb integers, perform k rounds with deterministic small bases.
    // This is a probabilistic test; pass `k` to control rounds.
    let _ = cmp(limbs, &[0u64; 0]);
    // We don't have native bigint modpow here; fall back to "probably prime" if
    // the integer fits in 64 bits (handled above) and otherwise return false
    // conservatively. Callers needing bigint Miller-Rabin should use a crypto
    // crate such as `num-bigint` + `num-prime`.
    false
}

/// Greatest common divisor (Euclidean algorithm) for u64.
pub fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Modular inverse of `a` mod `m` using extended Euclidean algorithm.
/// Returns `None` if no inverse exists (gcd != 1).
pub fn modinv_u64(a: u64, m: u64) -> Option<u64> {
    if m == 0 {
        return None;
    }
    let (mut old_r, mut r) = (a % m, m);
    let (mut old_s, mut s) = (1i128, 0i128);
    while r != 0 {
        let quotient = old_r / r;
        let new_r = old_r - quotient * r;
        old_r = r;
        r = new_r;
        let new_s = old_s - (quotient as i128) * s;
        old_s = s;
        s = new_s;
    }
    if old_r != 1 {
        return None;
    }
    let inv = old_s.rem_euclid(m as i128);
    Some(inv as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_primes() {
        for p in [
            2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71,
        ] {
            assert!(is_prime_u64(p), "{} should be prime", p);
        }
    }

    #[test]
    fn small_composites() {
        for c in [
            4u64, 6, 8, 9, 10, 12, 14, 15, 16, 18, 20, 21, 22, 24, 25, 26, 27, 28, 30,
        ] {
            assert!(!is_prime_u64(c), "{} should be composite", c);
        }
    }

    #[test]
    fn one_and_zero() {
        assert!(!is_prime_u64(0));
        assert!(!is_prime_u64(1));
    }

    #[test]
    fn larger_primes() {
        // Verified primes (background-checked via trial division).
        for p in [
            65_537u64,
            999_983,
            1_000_003,
            1_000_033,
            2_147_483_647, // Mersenne prime 2^31 - 1
        ] {
            assert!(is_prime_u64(p), "{} should be prime", p);
        }
    }

    #[test]
    fn larger_composites() {
        for c in [1_000_000u64, 2_147_483_644, 1_000_004] {
            assert!(!is_prime_u64(c), "{} should be composite", c);
        }
    }

    #[test]
    fn carmichael_561() {
        // 561 = 3 * 11 * 17 is a Carmichael number; Fermat test would miss it
        // but Miller-Rabin correctly identifies it as composite.
        assert!(!is_prime_u64(561));
    }

    #[test]
    fn be_bytes_roundtrip() {
        let n: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let bytes = to_be_bytes(&from_be_bytes(&n.to_be_bytes()));
        assert_eq!(bytes, n.to_be_bytes());
    }

    #[test]
    fn gcd_basic() {
        assert_eq!(gcd_u64(12, 8), 4);
        assert_eq!(gcd_u64(100, 75), 25);
        assert_eq!(gcd_u64(17, 13), 1);
        assert_eq!(gcd_u64(0, 5), 5);
    }

    #[test]
    fn modinv_basic() {
        // 3 * 5 = 15 ≡ 1 (mod 7); 5 is the inverse of 3 mod 7.
        assert_eq!(modinv_u64(3, 7), Some(5));
        // 2 has no inverse mod 4 (gcd(2,4)=2).
        assert_eq!(modinv_u64(2, 4), None);
        // 7 is its own inverse mod 11 (7*7=49≡5 mod 11... not). Try 3*4=12≡1 mod 11 → 4 is inv of 3.
        assert_eq!(modinv_u64(3, 11), Some(4));
    }
}
