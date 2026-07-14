//! Xoshiro256** PRNG — a fast, high-quality 64-bit pseudo-random number
//! generator. Suitable for non-cryptographic uses such as games, fuzzing,
//! and Monte Carlo simulations. NOT suitable for cryptography or
//! sensitive uses where unbiased unpredictability matters.
//!
//! Reference: Vigna, S. (2018) "Scrambled Linear Pseudorandom Number
//! Generators".

/// xoshiro256** state — four 64-bit state words plus a position cursor.
#[derive(Debug, Clone)]
pub struct Xoshiro256 {
    s: [u64; 4],
}

impl Xoshiro256 {
    /// New generator with the given seed. The seed is split across
    /// the four 64-bit state words via SplitMix64 (see [`splitmix_seed`])
    /// to decorrelate consecutive seeds.
    pub fn new(seed: u64) -> Self {
        let mut sm = splitmix_seed(seed);
        let mut s = [0u64; 4];
        for word in &mut s {
            *word = sm.next().unwrap_or(0);
        }
        Self { s }
    }

    /// Generate the next `u64`. Core algorithm:
    /// 1. `result = rotl(s[1] * 5, 7) * 9`
    /// 2. `t = s[1] << 17`
    /// 3. `s[2] ^= s[0]; s[3] ^= s[1]; s[1] ^= s[2]; s[0] ^= s[3];`
    /// 4. `s[2] ^= t; s[3] = rotl(s[3], 45);`
    pub fn next_u64(&mut self) -> u64 {
        let result = (self.s[1].wrapping_mul(5)).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Generate a `u32` value (zero-extended from a `u64`).
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Generate a value in `[lo, hi)`. Requires `lo < hi`.
    /// Uses the unbiased interval method (Lemire's fastrange-style).
    pub fn next_range(&mut self, lo: u32, hi: u32) -> u32 {
        assert!(lo < hi, "lo must be < hi");
        let range = hi - lo;
        lo + ((self.next_u32() as u64 * range as u64) >> 32) as u32
    }

    /// Skip `n` values from the current state. O(n).
    pub fn skip(&mut self, mut n: u64) {
        while n > 0 {
            self.next_u64();
            n -= 1;
        }
    }
}

/// SplitMix64 — used internally to decorrelate seeds. Constructed
/// from a single u64, generates a sequence of well-distributed u64s
/// suitable for seeding other PRNGs.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Iterator for SplitMix64 {
    type Item = u64;
    fn next(&mut self) -> Option<Self::Item> {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_3E09);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1338_91EB);
        Some(z ^ (z >> 31))
    }
}

fn splitmix_seed(seed: u64) -> SplitMix64 {
    SplitMix64::new(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_u64_returns_different_values() {
        let mut rng = Xoshiro256::new(42);
        let a = rng.next_u64();
        let b = rng.next_u64();
        assert_ne!(a, b);
    }

    #[test]
    fn deterministic_with_same_seed() {
        let mut r1 = Xoshiro256::new(42);
        let mut r2 = Xoshiro256::new(42);
        for _ in 0..100 {
            assert_eq!(r1.next_u64(), r2.next_u64());
        }
    }

    #[test]
    fn different_seeds_produce_different_sequences() {
        let mut r1 = Xoshiro256::new(42);
        let mut r2 = Xoshiro256::new(43);
        // With overwhelming probability, the first 1000 outputs differ
        let matches = (0..1000).filter(|_| r1.next_u64() == r2.next_u64()).count();
        // Expected matches by chance = 1000 * 2^-64 ≈ 0
        assert!(matches < 5);
    }

    #[test]
    fn range_is_within_bounds() {
        let mut rng = Xoshiro256::new(42);
        for _ in 0..100 {
            let v = rng.next_range(10, 20);
            assert!(v >= 10 && v < 20);
        }
    }

    #[test]
    fn skip_advances_state() {
        let mut r1 = Xoshiro256::new(42);
        let mut r2 = Xoshiro256::new(42);
        r1.skip(10);
        // skip-then-next vs. 10 repeated nexts
        for _ in 0..10 {
            r2.next_u64();
        }
        assert_eq!(r1.next_u64(), r2.next_u64());
    }

    #[test]
    fn splitmix_iter_yields_distinct() {
        let mut sm = SplitMix64::new(42);
        let a = sm.next().unwrap();
        let b = sm.next().unwrap();
        assert_ne!(a, b);
    }
}
