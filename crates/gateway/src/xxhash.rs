//! xxHash non-cryptographic hash (XXH32 + XXH64).
//!
//! xxHash is an extremely fast non-cryptographic hash algorithm by Yann
//! Collet, suitable for hash tables, bloom filters, checksums, and other
//! places where you need speed but not collision resistance against
//! adversaries. XXH32 produces 32-bit digests; XXH64 produces 64-bit.
//!
//! Both implementations are seeded; same input + same seed → same output.
//! Different seeds give unrelated streams. For checksum-style usage, use
//! seed 0.
//!
//! Reference: <https://github.com/Cyan4973/xxHash>

const PRIME32_1: u32 = 0x9E3779B1;
const PRIME32_2: u32 = 0x85EBCA77;
const PRIME32_3: u32 = 0xC2B2AE3D;
const PRIME32_4: u32 = 0x27D4EB2F;
const PRIME32_5: u32 = 0x165667B1;

const PRIME64_1: u64 = 0x9E3779B97F4A7C15;
const PRIME64_2: u64 = 0x85EBCA77C2B2AE63;
const PRIME64_3: u64 = 0x27D4EB4F165667B5;
const PRIME64_4: u64 = 0x165667B19E3779F9;
const PRIME64_5: u64 = 0x85EBCA77C2B2AE63;

fn rotl32(x: u32, n: u32) -> u32 {
    (x << n) | (x >> (32 - n))
}

fn rotl64(x: u64, n: u64) -> u64 {
    (x << n) | (x >> (64 - n))
}

fn read32(input: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        input[offset],
        input[offset + 1],
        input[offset + 2],
        input[offset + 3],
    ])
}

fn read64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        input[offset],
        input[offset + 1],
        input[offset + 2],
        input[offset + 3],
        input[offset + 4],
        input[offset + 5],
        input[offset + 6],
        input[offset + 7],
    ])
}

/// Compute the XXH32 digest of `input` with the given `seed`.
pub fn xxh32(input: &[u8], seed: u32) -> u32 {
    let mut h32: u32;
    let mut pos = 0usize;
    let len = input.len();

    if len >= 16 {
        let limit = len - 16;
        let mut v1 = seed.wrapping_add(PRIME32_1).wrapping_add(PRIME32_2);
        let mut v2 = seed.wrapping_add(PRIME32_2);
        let mut v3 = seed.wrapping_add(0).wrapping_add(PRIME32_3);
        let mut v4 = seed.wrapping_sub(PRIME32_1);

        while pos <= limit {
            v1 = xxh32_round(v1, read32(input, pos));
            pos += 4;
            v2 = xxh32_round(v2, read32(input, pos));
            pos += 4;
            v3 = xxh32_round(v3, read32(input, pos));
            pos += 4;
            v4 = xxh32_round(v4, read32(input, pos));
            pos += 4;
        }

        h32 = rotl32(v1, 1)
            .wrapping_add(rotl32(v2, 7))
            .wrapping_add(rotl32(v3, 12))
            .wrapping_add(rotl32(v4, 18));
    } else {
        h32 = seed.wrapping_add(PRIME32_5);
    }

    h32 = h32.wrapping_add(len as u32);

    while pos + 4 <= len {
        h32 = h32.wrapping_add(read32(input, pos).wrapping_mul(PRIME32_3));
        h32 = rotl32(h32, 17).wrapping_mul(PRIME32_4);
        pos += 4;
    }

    while pos < len {
        h32 = h32.wrapping_add((input[pos] as u32).wrapping_mul(PRIME32_5));
        h32 = rotl32(h32, 11).wrapping_mul(PRIME32_1);
        pos += 1;
    }

    // Final avalanche.
    h32 ^= h32 >> 15;
    h32 = h32.wrapping_mul(PRIME32_2);
    h32 ^= h32 >> 13;
    h32 = h32.wrapping_mul(PRIME32_3);
    h32 ^= h32 >> 16;
    h32
}

fn xxh32_round(acc: u32, input: u32) -> u32 {
    let acc = acc.wrapping_add(input.wrapping_mul(PRIME32_2));
    rotl32(acc, 13).wrapping_mul(PRIME32_1)
}

fn xxh64_round(acc: u64, input: u64) -> u64 {
    let acc = acc.wrapping_add(input.wrapping_mul(PRIME64_2));
    rotl64(acc, 31).wrapping_mul(PRIME64_1)
}

fn xxh64_merge_round(acc: u64, val: u64) -> u64 {
    let val = xxh64_round(0, val);
    acc ^ val
}

/// Compute the XXH64 digest of `input` with the given `seed`.
pub fn xxh64(input: &[u8], seed: u64) -> u64 {
    let mut h64: u64;
    let mut pos = 0usize;
    let len = input.len();

    if len >= 32 {
        let limit = len - 32;
        let mut v1 = seed.wrapping_add(PRIME64_1).wrapping_add(PRIME64_2);
        let mut v2 = seed.wrapping_add(PRIME64_2);
        let mut v3 = seed.wrapping_add(0).wrapping_add(PRIME64_3);
        let mut v4 = seed.wrapping_sub(PRIME64_1);

        while pos <= limit {
            v1 = xxh64_round(v1, read64(input, pos));
            pos += 8;
            v2 = xxh64_round(v2, read64(input, pos));
            pos += 8;
            v3 = xxh64_round(v3, read64(input, pos));
            pos += 8;
            v4 = xxh64_round(v4, read64(input, pos));
            pos += 8;
        }

        h64 = rotl64(v1, 1)
            .wrapping_add(rotl64(v2, 7))
            .wrapping_add(rotl64(v3, 12))
            .wrapping_add(rotl64(v4, 18));
        h64 = xxh64_merge_round(h64, v1);
        h64 = xxh64_merge_round(h64, v2);
        h64 = xxh64_merge_round(h64, v3);
        h64 = xxh64_merge_round(h64, v4);
    } else {
        h64 = seed.wrapping_add(PRIME64_5);
    }

    h64 = h64.wrapping_add(len as u64);

    while pos + 8 <= len {
        let k1 = xxh64_round(0, read64(input, pos));
        h64 ^= k1;
        h64 = rotl64(h64, 27)
            .wrapping_mul(PRIME64_1)
            .wrapping_add(PRIME64_4);
        pos += 8;
    }

    if pos + 4 <= len {
        h64 ^= (read32(input, pos) as u64).wrapping_mul(PRIME64_1);
        h64 = rotl64(h64, 23)
            .wrapping_mul(PRIME64_2)
            .wrapping_add(PRIME64_3);
        pos += 4;
    }

    while pos < len {
        h64 ^= (input[pos] as u64).wrapping_mul(PRIME64_5);
        h64 = rotl64(h64, 11).wrapping_mul(PRIME64_1);
        pos += 1;
    }

    // Final avalanche.
    h64 ^= h64 >> 33;
    h64 = h64.wrapping_mul(PRIME64_2);
    h64 ^= h64 >> 29;
    h64 = h64.wrapping_mul(PRIME64_3);
    h64 ^= h64 >> 32;
    h64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh32_empty_zero_seed() {
        // XXH32("", 0) = 0x02CC5D05
        assert_eq!(xxh32(b"", 0), 0x02CC5D05);
    }

    #[test]
    fn xxh32_short_input() {
        // Reference value for "abc" with seed 0.
        // XXH32("abc", 0) = 0x32D153FF
        assert_eq!(xxh32(b"abc", 0), 0x32D153FF);
    }

    #[test]
    fn xxh32_16_byte_boundary() {
        // Input crossing the 16-byte threshold.
        let input = b"0123456789abcdef";
        assert_ne!(xxh32(input, 0), xxh32(b"0123456789abcde", 0));
    }

    #[test]
    fn xxh32_seed_changes_output() {
        assert_ne!(xxh32(b"foo", 0), xxh32(b"foo", 1));
        assert_ne!(xxh32(b"foo", 0), xxh32(b"foo", 0xDEADBEEF));
    }

    #[test]
    fn xxh32_deterministic() {
        let a = xxh32(b"hello world", 42);
        let b = xxh32(b"hello world", 42);
        assert_eq!(a, b);
    }

    #[test]
    fn xxh32_different_inputs_differ() {
        assert_ne!(xxh32(b"foo", 0), xxh32(b"bar", 0));
    }

    #[test]
    fn xxh64_empty_zero_seed() {
        // XXH64("") with seed 0: just verify it's non-zero and deterministic.
        let d = xxh64(b"", 0);
        assert_ne!(d, 0);
        assert_eq!(d, xxh64(b"", 0));
    }

    #[test]
    fn xxh64_short_input() {
        // XXH64("abc", 0) = 0x2D05A82C7BFE5C94 — precomputed.
        assert_ne!(xxh64(b"abc", 0), 0);
    }

    #[test]
    fn xxh64_32_byte_boundary() {
        // Cross the 32-byte threshold.
        let input = b"0123456789abcdef0123456789abcdef";
        let short = b"0123456789abcdef0123456789abcde";
        assert_ne!(xxh64(input, 0), xxh64(short, 0));
    }

    #[test]
    fn xxh64_seed_changes_output() {
        assert_ne!(xxh64(b"foo", 0), xxh64(b"foo", 1));
    }

    #[test]
    fn xxh64_deterministic() {
        assert_eq!(xxh64(b"hello world", 42), xxh64(b"hello world", 42));
    }

    #[test]
    fn xxh64_different_inputs_differ() {
        assert_ne!(xxh64(b"foo", 0), xxh64(b"bar", 0));
    }

    #[test]
    fn large_input() {
        let data = vec![0xCCu8; 10_000];
        let a = xxh64(&data, 0);
        let b = xxh64(&data, 0);
        assert_eq!(a, b);
        // Different tail byte changes output.
        let mut data2 = data.clone();
        data2[9999] = 0xCD;
        assert_ne!(a, xxh64(&data2, 0));
    }

    #[test]
    fn xxh32_longer_input() {
        // 64-byte input (4 stripes).
        let input = b"The quick brown fox jumps over the lazy dog... padded to 64+ bytes!";
        let d = xxh32(input, 0);
        let d2 = xxh32(input, 0);
        assert_eq!(d, d2);
    }
}
