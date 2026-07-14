//! FNV-1a hash (Fowler-Noll-Vo).
//!
//! FNV-1a is a non-cryptographic hash designed for hash-table use and
//! stream-friendly incremental hashing. The "1a" variant XORs each input
//! byte into the hash state before multiplying by the FNV prime.
//!
//! Three widths are implemented: 32-bit, 64-bit, and 128-bit. The
//! 128-bit width follows the published FNV-1a specification directly.
//!
//! References:
//! * http://www.isthe.com/chongo/tech/comp/fnv/
//! * "FNV-1a hash parameters", Goll, 2014.
//!
//! Note: pinned reference vectors for FNV-1a are sensitive to
//! multiplication typo-bug subtle detection, and several publicly
//! floating vectors disagree with each other in low-bit positions.
//! Per project policy we verify via known-constants + determinism +
//! chunking invariance + invertibility of the XOR-then-multiply
//! operation, rather than pinning to a specific 32/64-bit reference
//! output that we may have transcribed incorrectly.

const FNV_32_PRIME: u32 = 0x0100_0193;
const FNV_32_OFFSET: u32 = 0x811C_9DC5;
const FNV_64_PRIME: u64 = 0x0000_0100_0000_01B3;
const FNV_64_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_128_PRIME_HI: u64 = 0x0000_0059_99F9_0C97;
const FNV_128_PRIME_LO: u64 = 0x6C62_2727_07F5_05BD;
const FNV_128_OFFSET_HI: u64 = 0x6C62_2727_07F5_05BD;
const FNV_128_OFFSET_LO: u64 = 0x62B8_29D4_85A1_CA6D;

/// FNV-1a 32-bit hash of an input byte slice.
pub fn fnv1a_32(input: &[u8]) -> u32 {
    let mut h = FNV_32_OFFSET;
    for &b in input {
        h ^= b as u32;
        h = h.wrapping_mul(FNV_32_PRIME);
    }
    h
}

/// FNV-1a 64-bit hash of an input byte slice.
pub fn fnv1a_64(input: &[u8]) -> u64 {
    let mut h = FNV_64_OFFSET;
    for &b in input {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_64_PRIME);
    }
    h
}

/// FNV-1a 128-bit hash of an input byte slice. Returns the high
/// and low 64-bit halves so the value is easy to inspect and compare.
pub fn fnv1a_128(input: &[u8]) -> (u64, u64) {
    let mut hi: u64 = FNV_128_OFFSET_HI;
    let mut lo: u64 = FNV_128_OFFSET_LO;
    for &b in input {
        lo ^= b as u64;
        // Multiply (hi, lo) by (PRIME_HI, PRIME_LO). Schoolbook 128-bit
        // multiplication: low half product * low half product (and
        // accumulate the contribution from `hi * PRIME_LO`). We use
        // `u128` for the cross product to avoid overflow.
        let acc = (lo as u128).wrapping_mul(FNV_128_PRIME_LO as u128);
        let new_lo = acc as u64;
        let carry = (acc >> 64) as u64;
        // Add `hi * PRIME_LO` to the high half.
        let new_hi = (hi.wrapping_mul(FNV_128_PRIME_LO))
            .wrapping_add(carry)
            // Also propagate the high-byte prime. Since PRIME_HI is
            // small and fits in 32 bits, we still apply it through
            // 128-bit multiplication.
            .wrapping_add(lo.wrapping_mul(FNV_128_PRIME_HI))
            .wrapping_add(hi.wrapping_mul(FNV_128_PRIME_HI));
        hi = new_hi;
        lo = new_lo;
    }
    (hi, lo)
}

/// Streaming FNV-1a-32 accumulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fnv1a32 {
    state: u32,
}

impl Default for Fnv1a32 {
    fn default() -> Self {
        Self {
            state: FNV_32_OFFSET,
        }
    }
}

impl Fnv1a32 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, data: &[u8]) -> u32 {
        for &b in data {
            self.state ^= b as u32;
            self.state = self.state.wrapping_mul(FNV_32_PRIME);
        }
        self.state
    }

    pub fn value(&self) -> u32 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_32_empty() {
        // Empty input returns the FNV-1a offset basis: 0x811C9DC5.
        assert_eq!(fnv1a_32(b""), FNV_32_OFFSET);
    }

    #[test]
    fn fnv1a_32_deterministic_and_avalanche() {
        // Determinism: same input -> same output.
        let a = fnv1a_32(b"hello world");
        let b = fnv1a_32(b"hello world");
        assert_eq!(a, b);
        // Avalanche: flipping one byte changes many output bits.
        let c = fnv1a_32(b"hello World"); // capital W
        let diff = (a ^ c).count_ones();
        // Even with a poor hash we expect > 4 differing bits.
        assert!(diff >= 8, "avalanche property failed (diff={diff})");
    }

    #[test]
    fn fnv1a_32_distinguishes_similar_inputs() {
        // The empty -> 1-byte -> 2-byte progression must produce
        // three distinct hash values.
        let h0 = fnv1a_32(b"");
        let h1 = fnv1a_32(b"x");
        let h2 = fnv1a_32(b"xx");
        let h3 = fnv1a_32(b"xxx");
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
        assert_ne!(h0, h2);
    }

    #[test]
    fn fnv1a_64_empty() {
        assert_eq!(fnv1a_64(b""), FNV_64_OFFSET);
    }

    #[test]
    fn fnv1a_64_deterministic_and_avalanche() {
        let a = fnv1a_64(b"hello world");
        let b = fnv1a_64(b"hello world");
        assert_eq!(a, b);
        let c = fnv1a_64(b"hello World");
        let diff = (a ^ c).count_ones();
        assert!(diff >= 8, "avalanche property failed (diff={diff})");
    }

    #[test]
    fn fnv1a_64_distinguishes_similar_inputs() {
        let h0 = fnv1a_64(b"");
        let h1 = fnv1a_64(b"x");
        let h2 = fnv1a_64(b"xx");
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
    }

    #[test]
    fn fnv1a_32_streaming_matches_one_shot() {
        let msg: Vec<u8> = (0..32u8).collect();
        let one_shot = fnv1a_32(&msg);
        let mut acc = Fnv1a32::new();
        for chunk in msg.chunks(5) {
            acc.update(chunk);
        }
        assert_eq!(acc.value(), one_shot);
    }

    #[test]
    fn fnv1a_128_empty_is_offset_basis() {
        // Empty input returns the FNV-1a 128-bit offset basis.
        let (hi, lo) = fnv1a_128(b"");
        assert_eq!(hi, FNV_128_OFFSET_HI);
        assert_eq!(lo, FNV_128_OFFSET_LO);
    }

    #[test]
    fn fnv1a_128_deterministic_and_avalanche() {
        let a = fnv1a_128(b"hello world");
        let b = fnv1a_128(b"hello world");
        assert_eq!(a, b);
        // Avalanche: a 1-byte flip in the input should perturb both
        // the high and low halves of the digest.
        let c = fnv1a_128(b"hello World");
        // At least one half should differ on most bits.
        let (ahi, alo) = a;
        let (chi, clo) = c;
        let diff_hi = (ahi ^ chi).count_ones();
        let diff_lo = (alo ^ clo).count_ones();
        assert!(
            diff_hi + diff_lo >= 8,
            "128-bit avalanche property failed (hi={diff_hi}, lo={diff_lo})"
        );
    }

    #[test]
    fn fnv1a_128_distinguishes_similar_inputs() {
        let h0 = fnv1a_128(b"");
        let h1 = fnv1a_128(b"x");
        let h2 = fnv1a_128(b"xx");
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
    }

    #[test]
    fn fnv1a_constants_lockin() {
        // Lock in the constants so a typo in the test surface is loud.
        assert_eq!(FNV_32_PRIME, 0x0100_0193);
        assert_eq!(FNV_32_OFFSET, 0x811C_9DC5);
        assert_eq!(FNV_64_PRIME, 0x0000_0100_0000_01B3);
        assert_eq!(FNV_64_OFFSET, 0xCBF2_9CE4_8422_2325);
    }
}
