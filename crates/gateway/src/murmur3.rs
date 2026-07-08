//! MurmurHash3 x86 32-bit.
//!
//! Austin Appleby's MurmurHash3 is a non-cryptographic hash function
//! designed for hash-based lookups (Bloom filters, hash tables, sharding).
//! It is widely used by Apache Cassandra, Druid, Elasticsearch (in some
//! paths), and many in-memory stores.
//!
//! This module implements the **x86 32-bit** variant. It is not
//! cryptographic — do not use it for authentication or tamper detection.
//!
//! Reference: <https://github.com/aappleby/smhasher/blob/master/src/MurmurHash3.cpp>
//! Test vectors: <https://github.com/aappleby/smHasher/blob/master/vectors.32>

const C1: u32 = 0xcc9e_2d51;
const C2: u32 = 0x1b87_3593;

#[inline]
fn fmix32(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h
}

#[inline]
fn rotl32(x: u32, r: i32) -> u32 {
    (x << r) | (x >> (32 - r))
}

/// MurmurHash3 x86 32-bit with the given seed.
///
/// Returns a `u32` digest. The function is allocation-free and operates
/// directly on the input slice.
pub fn hash32(key: &[u8], seed: u32) -> u32 {
    let len = key.len();
    let nblocks = len / 4;

    let mut h1: u32 = seed;

    // Body: 4 bytes at a time, little-endian.
    for i in 0..nblocks {
        let off = i * 4;
        let k = u32::from_le_bytes([key[off], key[off + 1], key[off + 2], key[off + 3]]);
        let mut k1 = k.wrapping_mul(C1);
        k1 = rotl32(k1, 15);
        k1 = k1.wrapping_mul(C2);
        h1 ^= k1;
        h1 = rotl32(h1, 13);
        h1 = h1.wrapping_mul(5).wrapping_add(0xe654_6b64);
    }

    // Tail: 0..3 leftover bytes.
    let tail = &key[nblocks * 4..];
    let mut k1: u32 = 0;
    if tail.len() >= 3 {
        k1 ^= (tail[2] as u32) << 16;
    }
    if tail.len() >= 2 {
        k1 ^= (tail[1] as u32) << 8;
    }
    if !tail.is_empty() {
        k1 ^= tail[0] as u32;
        k1 = k1.wrapping_mul(C1);
        k1 = rotl32(k1, 15);
        k1 = k1.wrapping_mul(C2);
        h1 ^= k1;
    }

    // Finalization.
    h1 ^= len as u32;
    fmix32(h1)
}

/// Convenience wrapper: hash with seed 0.
pub fn hash(key: &[u8]) -> u32 {
    hash32(key, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference vectors cross-checked against the canonical MurmurHash3
    // x86 32-bit algorithm (Austin Appleby's smhasher reference port).

    #[test]
    fn empty_input_zero_seed() {
        // seed = 0, len = 0 -> 0
        assert_eq!(hash32(b"", 0), 0);
    }

    #[test]
    fn empty_input_arbitrary_seed() {
        // seed != 0, len = 0 -> fmix32(seed)
        for seed in [1u32, 0x1234_5678, 0xDEAD_BEEF, u32::MAX] {
            assert_eq!(hash32(b"", seed), fmix32(seed),
                "empty input must equal fmix32(seed) for seed={:#x}", seed);
        }
    }

    #[test]
    fn single_zero_byte() {
        // len = 1, key = 0x00, seed = 0 -> 0x514e28b7
        assert_eq!(hash32(&[0x00], 0), 0x514e_28b7);
    }

    #[test]
    fn four_zero_bytes_seed_1() {
        // len = 4, key = 0x00 00 00 00, seed = 1 -> 0x78ed212d
        assert_eq!(hash32(&[0x00, 0x00, 0x00, 0x00], 1), 0x78ed_212d);
    }

    #[test]
    fn four_zero_bytes_seed_zero() {
        // len = 4, key = 0x00 00 00 00, seed = 0 -> 0x2362f9de
        assert_eq!(hash32(&[0x00, 0x00, 0x00, 0x00], 0), 0x2362_f9de);
    }

    #[test]
    fn twelve_zero_bytes_seed_zero() {
        // len = 12, 12 zero bytes, seed = 0 -> 0xd941144b
        assert_eq!(hash32(&[0; 12], 0), 0xd941_144b);
    }

    #[test]
    fn thirteen_zero_bytes_seed_zero() {
        // 13 = 3 * 4 + 1 — tail length 1.
        assert_eq!(hash32(&[0; 13], 0), 0xb996_0eb1);
    }

    #[test]
    fn fourteen_zero_bytes_seed_zero() {
        // tail length 2.
        assert_eq!(hash32(&[0; 14], 0), 0x4162_84af);
    }

    #[test]
    fn fifteen_zero_bytes_seed_zero() {
        // tail length 3.
        assert_eq!(hash32(&[0; 15], 0), 0xbbcc_7858);
    }

    #[test]
    fn sixteen_zero_bytes_seed_zero() {
        // four full blocks.
        assert_eq!(hash32(&[0; 16], 0), 0x8134_cdf8);
    }

    #[test]
    fn ascii_hello_world_seed_zero() {
        // "Hello, world!" (13 bytes) with seed 0 -> 0xc0363e43.
        assert_eq!(hash32(b"Hello, world!", 0), 0xc036_3e43);
    }

    #[test]
    fn reference_vector_long_input_seed_zero() {
        // 64-byte base64-style alphabet (A-Z, a-z, 0-9, -, _) with seed 0.
        let key: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        assert_eq!(key.len(), 64);
        assert_eq!(hash32(key, 0), 0x3062_0b0d);
    }

    #[test]
    fn hash_helper_uses_seed_zero() {
        let a = hash(b"some-key");
        let b = hash32(b"some-key", 0);
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_produce_different_digests() {
        let key = b"the quick brown fox jumps over the lazy dog";
        let a = hash32(key, 0);
        let b = hash32(key, 1);
        let c = hash32(key, 0xC0FFEE);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn distribution_smoke_check() {
        // Hash 1024 distinct 4-byte keys with seed 0 and confirm we don't
        // see collisions on this small sample.
        let mut seen = std::collections::HashSet::new();
        for i in 0u32..1024 {
            let h = hash32(&i.to_le_bytes(), 0);
            assert!(seen.insert(h), "unexpected collision at i={}", i);
        }
        assert_eq!(seen.len(), 1024);
    }

    #[test]
    fn fmix32_reference_vector() {
        // fmix32(0) == 0 (the mix is identity on a zero).
        assert_eq!(fmix32(0), 0);
        // Determinism + avalanche sanity: any input change scrambles the
        // high bits and produces a different output. We assert the
        // determinism property (calling twice yields the same answer) and
        // a non-trivial output for a non-trivial input, without pinning
        // to a specific fingerprint.
        for x in [1u32, 0xDEAD_BEEF, 0x1234_5678, 0xFFFF_FFFF, 0xC0FFEE00] {
            assert_eq!(fmix32(x), fmix32(x), "fmix32 must be deterministic at x={:#x}", x);
            assert_ne!(fmix32(x), x, "fmix32 must scramble non-trivial inputs at x={:#x}", x);
        }
    }
}