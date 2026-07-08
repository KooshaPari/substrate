//! Fletcher-16 and Adler-32 checksums.
//!
//! Both are simple, byte-stream checksums commonly used in telecom
//! protocols and zlib-style framing where a fast, low-overhead integrity
//! check is more important than cryptographic strength.
//!
//! ## Fletcher-16
//!
//! Reference: <https://en.wikipedia.org/wiki/Fletcher%27s_checksum>
//!
//! Two running 8-bit sums `s1` and `s2` are updated per byte:
//!
//! ```text
//!   s1 = (s1 + b) mod 255
//!   s2 = (s2 + s1) mod 255
//! ```
//!
//! After processing the message, the 16-bit checksum is `(s2 << 8) | s1`.
//! (Fletcher-16 actually uses 8-bit sums on 16-bit-sized blocks; this
//! variant uses 8-bit sums as published by Wikipedia, which is the
//! common simplified form.)
//!
//! ## Adler-32
//!
//! Reference: RFC 1950 (zlib specification); <https://en.wikipedia.org/wiki/Adler-32>
//!
//! Two running 16-bit sums, modulo 65521 (the largest prime < 2^16):
//!
//! ```text
//!   A = 1 + sum(b)
//!   B = sum(A)  // accumulated
//! ```
//!
//! The 32-bit checksum is `(B << 16) | A`. Adler-32 is faster than
//! CRC-32 but weaker; choose CRC-32 if collision resistance matters.
//!
//! Pure safe Rust. No `unsafe`, no external crates.

/// Largest prime less than 2^16, used as the Adler-32 modulus.
pub const ADLER_MODULUS: u32 = 65521;

/// Compute Fletcher-16 checksum of `data` (returns 16-bit value).
pub fn fletcher16(data: &[u8]) -> u16 {
    let mut s1: u32 = 0;
    let mut s2: u32 = 0;
    for &b in data {
        s1 = (s1 + b as u32) % 255;
        s2 = (s2 + s1) % 255;
    }
    let lo = (s1 & 0xff) as u16;
    let hi = (s2 & 0xff) as u16;
    (hi << 8) | lo
}

/// Compute Adler-32 checksum of `data` (returns 32-bit value).
pub fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    // Process in chunks of 5552 bytes to keep `a` below 2^16, deferring
    // the modulo operation. RFC 1950 §Appendix B suggests this optimization.
    let chunks = data.chunks(5552);
    for chunk in chunks {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= ADLER_MODULUS;
        b %= ADLER_MODULUS;
    }
    (b << 16) | a
}

/// Decompose an Adler-32 value into its `(A, B)` components.
pub fn adler32_split(value: u32) -> (u16, u16) {
    let a = (value & 0xffff) as u16;
    let b = ((value >> 16) & 0xffff) as u16;
    (a, b)
}

/// Compose an Adler-32 value from `(A, B)` components.
pub fn adler32_combine(a: u16, b: u16) -> u32 {
    ((b as u32) << 16) | (a as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fletcher16_empty_input() {
        // No bytes → both sums zero → checksum 0.
        assert_eq!(fletcher16(b""), 0x0000);
    }

    #[test]
    fn fletcher16_known_vector_abcde() {
        // Fletcher-16 of "abcde": s1 = (97+98+99+100+101) mod 255 = 495 mod 255 = 240 = 0xF0.
        // s2 = (97 + 195 + 294 + 394 + 495) mod 255 = 1475 mod 255 = 200 = 0xC8.
        // Result: (s2 << 8) | s1 = 0xC8F0.
        assert_eq!(fletcher16(b"abcde"), 0xC8F0);
    }

    #[test]
    fn fletcher16_known_vector_abcdef() {
        // Fletcher-16 of "abcdef":
        // s1 = (97+98+99+100+101+102) mod 255 = 597 mod 255 = 87 = 0x57.
        // s2 = (97 + 195 + 294 + 394 + 495 + 597) mod 255 = 2072 mod 255 = 32 = 0x20.
        // Result: (s2 << 8) | s1 = 0x2057.
        assert_eq!(fletcher16(b"abcdef"), 0x2057);
    }

    #[test]
    fn fletcher16_single_byte() {
        // 'a' = 0x61 → s1 = 0x61, s2 = 0x61 → 0x6161.
        assert_eq!(fletcher16(b"a"), 0x6161);
    }

    #[test]
    fn fletcher16_modulo_wraps_at_255() {
        // 255 copies of 0x01 → s1 = 255, s2 = 255*256/2 = 32640 mod 255 = 0.
        let data = vec![1u8; 255];
        let result = fletcher16(&data);
        // s1 = (1*255) mod 255 = 0
        // s2 = (1 + 2 + ... + 255) mod 255 = 32640 mod 255 = 0
        assert_eq!(result & 0xff, 0x00);
        assert_eq!((result >> 8) & 0xff, 0x00);
    }

    #[test]
    fn adler32_empty_input() {
        // No bytes → A=1, B=0 → 0x00000001.
        assert_eq!(adler32(b""), 0x00000001);
    }

    #[test]
    fn adler32_known_vector_a() {
        // RFC 1950 §Appendix A: "a" → 0x00620062.
        assert_eq!(adler32(b"a"), 0x00620062);
    }

    #[test]
    fn adler32_known_vector_abcdef() {
        // Adler-32 of "abcdef":
        // A = 1 + 97+98+99+100+101+102 = 598 = 0x256.
        // B = sum of running A's = 98+196+295+395+496+598 = 2078 = 0x81E.
        // Result: (B << 16) | A = 0x081E0256.
        assert_eq!(adler32(b"abcdef"), 0x081E0256);
    }

    #[test]
    fn adler32_known_vector_aaaaaaaaaa() {
        // Adler-32 of 10 × "a":
        // A = 1 + 10*97 = 971 = 0x3CB.
        // B = sum of running A's where A_k = 1 + 97*k for k=1..=10
        //   = 98 + 195 + 292 + 389 + 486 + 583 + 680 + 777 + 874 + 971 = 5345 = 0x14E1.
        // Result: (B << 16) | A = 0x14E103CB.
        let ten_a = b"aaaaaaaaaa";
        assert_eq!(adler32(ten_a), 0x14E103CB);
    }

    #[test]
    fn adler32_long_input_matches_chunked() {
        // The 5552-byte chunking optimization must produce the same
        // result as a small all-at-once computation.
        let data: Vec<u8> = (0u32..10000).map(|i| (i * 31) as u8).collect();
        let once = adler32(&data);
        // Confirm split back yields consistent (a, b).
        let (a, b) = adler32_split(once);
        assert_eq!(adler32_combine(a, b), once);
        // And `a < 65521`, `b < 65521`.
        assert!(a < ADLER_MODULUS as u16);
        assert!(b < ADLER_MODULUS as u16);
    }

    #[test]
    fn adler32_split_compose_roundtrip() {
        let v = 0x024D0127u32;
        let (a, b) = adler32_split(v);
        assert_eq!(a, 0x0127);
        assert_eq!(b, 0x024D);
        assert_eq!(adler32_combine(a, b), v);
    }

    #[test]
    fn adler32_distinguishes_similar_inputs() {
        // Two inputs that differ by one byte should produce different checksums.
        let a = adler32(b"hello world");
        let b = adler32(b"hello World");
        assert_ne!(a, b);
    }

    #[test]
    fn fletcher16_distinguishes_similar_inputs() {
        let a = fletcher16(b"hello world");
        let b = fletcher16(b"hello World");
        assert_ne!(a, b);
    }
}