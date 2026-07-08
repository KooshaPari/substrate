//! Quadratic-residue generator for prime-indexed pseudo-random numbers.
//!
//! Given a prime `p`, the squares modulo `p` partition the residues
//! `1..=p-1` into `(p-1)/2` quadratic residues and `(p-1)/2` quadratic
//! non-residues. The sequence `2^n mod p` (for some primitive root `g`)
//! produces a permutation of `1..=p-1` whose parity with respect to QR
//! forms a "quadratic-residue pseudo-random" generator (QRNG).
//!
//! Reference: <https://en.wikipedia.org/wiki/Quadratic_residue>

use std::collections::HashMap;

/// Compute Euler's totient function φ(n) — the count of integers in
/// [1, n] coprime to n.
pub fn euler_totient(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut result = n;
    let mut m = n;
    let mut p = 2;
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

/// Find the smallest primitive root modulo `p`, where `p` is a prime.
/// Returns `None` if `p` is not prime.
pub fn primitive_root(p: u64) -> Option<u64> {
    if !is_probably_prime(p) {
        return None;
    }
    // For prime p, an integer g is a primitive root iff for every prime
    // factor q of p-1, g^((p-1)/q) != 1 mod p.
    let phi = p - 1;
    // Factor p-1 (small enough for our test inputs).
    let factors = prime_factors(phi);
    for g in 2..p {
        let mut ok = true;
        for &q in &factors {
            if pow_mod(g, phi / q, p) == 1 {
                ok = false;
                break;
            }
        }
        if ok {
            return Some(g);
        }
    }
    None
}

/// Prime factorization of `n` (returns distinct primes).
pub fn prime_factors(n: u64) -> Vec<u64> {
    let mut factors = Vec::new();
    let mut m = n;
    let mut p = 2;
    while p * p <= m {
        if m % p == 0 {
            factors.push(p);
            while m % p == 0 {
                m /= p;
            }
        }
        p += 1;
    }
    if m > 1 {
        factors.push(m);
    }
    factors
}

/// Miller-Rabin deterministic test for n < 3.3e24 (sufficient for
/// primes we'll encounter as primitive root moduli).
pub fn is_probably_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let mut d = n - 1;
    let mut s = 0;
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    // Bases sufficient for n < 3,317,044,064,679,887,385,961,981.
    let bases = [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for &a in &bases {
        if a >= n {
            continue;
        }
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        let mut composite = true;
        for _ in 0..s - 1 {
            x = mul_mod(x, x, n);
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

fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
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

fn pow_mod(mut base: u64, mut exp: u64, m: u64) -> u64 {
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

/// A quadratic-residue pseudo-random generator (QRNG).
///
/// Produces the next value via `g^n mod p` for `n = 0, 1, 2, ...` where
/// `g` is a primitive root modulo `p`.
pub struct Qrng {
    p: u64,
    g: u64,
    state: u64,
}

impl Qrng {
    /// Build a new QRNG with the given prime `p` and a starting state
    /// exponent `n0`. Fails if `p` is not prime.
    pub fn new(p: u64, n0: u64) -> Option<Self> {
        let g = primitive_root(p)?;
        Some(Self {
            p,
            g,
            state: pow_mod(g, n0, p),
        })
    }

    pub fn modulus(&self) -> u64 {
        self.p
    }

    pub fn generator(&self) -> u64 {
        self.g
    }

    /// Current output value (in [1, p-1]).
    pub fn current(&self) -> u64 {
        self.state
    }

    /// Advance the generator and return the next value.
    pub fn next(&mut self) -> u64 {
        self.state = mul_mod(self.state, self.g, self.p);
        self.state
    }

    /// Compute the Legendre symbol `(a | p)` — 1 if `a` is a quadratic
    /// residue mod `p`, -1 if non-residue, 0 if `a ≡ 0 mod p`.
    pub fn legendre(&self, a: u64) -> i64 {
        if a % self.p == 0 {
            return 0;
        }
        let r = pow_mod(a, (self.p - 1) / 2, self.p);
        if r == 0 {
            0
        } else if r == 1 {
            1
        } else {
            -1
        }
    }
}

/// Cache of primitive roots for small primes, computed on first use.
pub fn cached_primitive_root(p: u64) -> Option<u64> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<std::sync::Mutex<HashMap<u64, u64>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if let Some(g) = cache.lock().unwrap().get(&p) {
        return Some(*g);
    }
    let g = primitive_root(p)?;
    cache.lock().unwrap().insert(p, g);
    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_totient_basic() {
        assert_eq!(euler_totient(1), 1);
        assert_eq!(euler_totient(2), 1);
        assert_eq!(euler_totient(10), 4);
        assert_eq!(euler_totient(36), 12); // 36 = 2^2 * 3^2 → φ = 36 * 1/2 * 2/3 = 12
        assert_eq!(euler_totient(97), 96); // 97 is prime
    }

    #[test]
    fn prime_factors_basic() {
        assert_eq!(prime_factors(12), vec![2, 3]);
        assert_eq!(prime_factors(97), vec![97]);
        assert_eq!(prime_factors(360), vec![2, 3, 5]);
    }

    #[test]
    fn miller_rabin_basic() {
        for p in [2u64, 3, 5, 7, 11, 13, 97, 101, 1009] {
            assert!(is_probably_prime(p), "{} should be prime", p);
        }
        for c in [0u64, 1, 4, 6, 8, 9, 10, 15, 100, 1001] {
            assert!(!is_probably_prime(c), "{} should be composite", c);
        }
    }

    #[test]
    fn primitive_root_known() {
        // Smallest primitive root mod 7 is 3: 3^1=3, 3^2=2, 3^3=6, 3^4=4, 3^5=5, 3^6=1.
        assert_eq!(primitive_root(7), Some(3));
        // Mod 11: smallest is 2.
        assert_eq!(primitive_root(11), Some(2));
    }

    #[test]
    fn primitive_root_rejects_composite() {
        assert_eq!(primitive_root(6), None);
        assert_eq!(primitive_root(15), None);
    }

    #[test]
    fn qrng_visits_all_nonzero_residues() {
        // For prime p, g^n mod p for n=0..p-1 should produce every
        // value in [1, p-1] exactly once.
        let mut gen = Qrng::new(7, 0).unwrap();
        let mut seen = vec![false; 7];
        for _ in 0..6 {
            let v = gen.next() as usize;
            assert!(v >= 1 && v < 7, "value {} out of range", v);
            assert!(!seen[v], "value {} visited twice", v);
            seen[v] = true;
        }
        let count: usize = seen.iter().filter(|&&x| x).count();
        assert_eq!(count, 6);
    }

    #[test]
    fn legendre_classification() {
        // Mod 7: QRs are {1, 2, 4}; non-residues are {3, 5, 6}.
        let gen = Qrng::new(7, 0).unwrap();
        for a in [1u64, 2, 4] {
            assert_eq!(gen.legendre(a), 1, "{} should be QR mod 7", a);
        }
        for a in [3u64, 5, 6] {
            assert_eq!(gen.legendre(a), -1, "{} should be non-QR mod 7", a);
        }
        assert_eq!(gen.legendre(0), 0);
        assert_eq!(gen.legendre(14), 0); // 14 % 7 == 0
    }
}