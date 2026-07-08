//! Binary <-> Gray code conversion.
//!
//! Gray code is a binary numeral system where two successive values differ
//! in only one bit (the "binary reflected Gray code", BRGC). It is widely
//! used in error correction, position encoders, Karnaugh maps, and
//! applications where bit transitions need to be minimised (avoiding
//! spurious intermediate states when crossing power-of-two boundaries on
//! hardware counters).
//!
//! Conversion rules (BRGC):
//!
//! * `gray = binary ^ (binary >> 1)`
//! * `binary = gray; while gray >>= 1 { binary ^= gray }`
//!
//! For 64-bit integers the conversion is single-cycle on most modern CPUs
//! (the iterative decoder is branch-free in `decode_u64`).
//!
//! Reference: Frank Gray, "Pulse Code Communication", US Patent 2,632,058
//! (1953); also described in Knuth TAOCP vol. 4A §7.1.3.

/// Convert a binary integer to its BRGC (binary-reflected Gray code) value.
///
/// For 64-bit inputs this is a single XOR + shift: `g = b ^ (b >> 1)`.
#[inline]
pub fn encode_u64(b: u64) -> u64 {
    b ^ (b >> 1)
}

/// Convert a BRGC Gray code value back to its binary integer.
///
/// Branch-free decoder derived from the property that each binary bit is
/// the XOR of all higher-order Gray-code bits. With the iterative form
/// below the body executes in 6 iterations for 64-bit inputs, which LLVM
/// unrolls cleanly.
#[inline]
pub fn decode_u64(g: u64) -> u64 {
    let mut b = g;
    b ^= b >> 32;
    b ^= b >> 16;
    b ^= b >> 8;
    b ^= b >> 4;
    b ^= b >> 2;
    b ^= b >> 1;
    b
}

/// Convert the low `width` bits of a binary integer to BRGC. Width must be
/// in `1..=64`; the high bits of the result are zero.
#[inline]
pub fn encode_n(b: u64, width: u32) -> u64 {
    assert!(width > 0 && width <= 64, "width must be in 1..=64");
    let mask = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
    let b = b & mask;
    (b ^ (b >> 1)) & mask
}

/// Convert an `n`-bit Gray code value to a binary integer.
#[inline]
pub fn decode_n(g: u64, width: u32) -> u64 {
    assert!(width > 0 && width <= 64, "width must be in 1..=64");
    let mask = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
    decode_u64(g) & mask
}

/// Encode a slice of bits (LSB-first) into a Gray code slice.
/// Returns a new `Vec<bool>` of the same length.
pub fn encode_bits(bits: &[bool]) -> Vec<bool> {
    let mut out = Vec::with_capacity(bits.len());
    let mut prev = false;
    for &b in bits {
        // g_i = b_i XOR b_{i-1} for i >= 1, g_0 = b_0.
        let g = b ^ prev;
        out.push(g);
        prev = b;
    }
    out
}

/// Decode a Gray-code bit slice (LSB-first) into a binary bit slice.
pub fn decode_bits(gray: &[bool]) -> Vec<bool> {
    let mut out = Vec::with_capacity(gray.len());
    let mut acc = false;
    for &g in gray {
        acc ^= g;
        out.push(acc);
    }
    out
}

/// Iterate the BRGC sequence over `count` codes starting from 0. Convenient
/// for printing, range-encoding, and rank checks.
pub fn sequence(count: u64) -> Vec<u64> {
    (0..count).map(encode_u64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_zero_is_zero() {
        assert_eq!(encode_u64(0), 0);
    }

    #[test]
    fn decode_zero_is_zero() {
        assert_eq!(decode_u64(0), 0);
    }

    #[test]
    fn encode_decode_roundtrip_u64() {
        // Round-trip a bunch of values including boundaries and powers of two.
        let cases: &[u64] = &[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256,
            1023, 1024, 4095, 65535, 65536, 1u64 << 31, 1u64 << 32, 1u64 << 63,
            u64::MAX, u64::MAX - 1, 0xDEAD_BEEF_CAFE_BABE,
        ];
        for &b in cases {
            let g = encode_u64(b);
            let back = decode_u64(g);
            assert_eq!(back, b, "round-trip failed for b={:#x}", b);
        }
    }

    #[test]
    fn encode_known_table() {
        // Canonical 4-bit BRGC table (Wikipedia / Knuth):
        //   b   g
        //   0   0000
        //   1   0001
        //   2   0011
        //   3   0010
        //   4   0110
        //   5   0111
        //   6   0101
        //   7   0100
        //   8   1100
        //   9   1101
        //  10   1111
        //  11   1110
        //  12   1010
        //  13   1011
        //  14   1001
        //  15   1000
        let table: &[(u64, u64)] = &[
            (0, 0b0000), (1, 0b0001), (2, 0b0011), (3, 0b0010),
            (4, 0b0110), (5, 0b0111), (6, 0b0101), (7, 0b0100),
            (8, 0b1100), (9, 0b1101), (10, 0b1111), (11, 0b1110),
            (12, 0b1010), (13, 0b1011), (14, 0b1001), (15, 0b1000),
        ];
        for &(b, g) in table {
            assert_eq!(encode_n(b, 4), g, "encode mismatch at b={}", b);
            assert_eq!(decode_n(g, 4), b, "decode mismatch at g={}", g);
        }
    }

    #[test]
    fn successive_codes_differ_in_one_bit() {
        // The defining property of BRGC: g(i) and g(i+1) must differ in
        // exactly one bit.
        let codes: Vec<u64> = sequence(256);
        for w in codes.windows(2) {
            let diff = w[0] ^ w[1];
            assert!(diff.is_power_of_two(),
                "consecutive Gray codes differ in more than one bit: \
                 g({})={:#x} g({})={:#x} diff={:#x}",
                0u64, w[0], 1u64, w[1], diff);
        }
    }

    #[test]
    fn sequence_covers_first_n_codes_without_duplicates() {
        let codes = sequence(64);
        let mut sorted: Vec<u64> = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "duplicate codes in BRGC sequence");
    }

    #[test]
    fn encode_bits_roundtrip() {
        let bits: Vec<bool> = (0..16).map(|i| (i % 3 == 0) ^ (i % 5 == 0)).collect();
        let g = encode_bits(&bits);
        let back = decode_bits(&g);
        assert_eq!(back, bits);
    }

    #[test]
    fn encode_bits_zero_first_bit() {
        // g_0 == b_0 by definition.
        let bits = vec![true, false, true, false, true];
        let g = encode_bits(&bits);
        assert_eq!(g[0], true);
    }

    #[test]
    fn encode_n_clamps_high_bits() {
        // A 4-bit encoder must drop bit 4 and above.
        let g = encode_n(0b1_1111, 4);
        assert_eq!(g, 0b1000);
        // 8-bit round-trip.
        let g = encode_n(0xFF, 8);
        assert_eq!(g, 0b1000_0000);
        assert_eq!(decode_n(g, 8), 0xFF);
    }

    #[test]
    #[should_panic]
    fn encode_n_panics_on_zero_width() {
        let _ = encode_n(0, 0);
    }

    #[test]
    #[should_panic]
    fn decode_n_panics_on_oversized_width() {
        let _ = decode_n(0, 65);
    }
}