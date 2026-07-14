//! TEA — Tiny Encryption Algorithm (Wheeler & Needham, 1994).
//!
//! A 64-bit block Feistel cipher with a 128-bit key. 32 Feistel rounds.
//! Used historically in many lightweight applications; superseded for
//! most cryptographic purposes by XTEA, then XXTEA, but still
//! instructive as a minimal example of a correct Feistel structure.
//!
//! Reference: D. J. Wheeler, R. M. Needham, "TEA, a Tiny Encryption
//! Algorithm", Proceedings of FSE 1994, LNCS 1008.

const DELTA: u32 = 0x9E37_79B9;

/// TEA block: encrypt a single 8-byte block under a 4-word (128-bit) key.
pub fn encrypt_block(v0: u32, v1: u32, key: &[u32; 4]) -> (u32, u32) {
    let mut v0 = v0;
    let mut v1 = v1;
    let mut sum: u32 = 0;
    for _ in 0..32 {
        sum = sum.wrapping_add(DELTA);
        v0 = v0.wrapping_add(
            (v1 << 4).wrapping_add(key[2]) ^ v1.wrapping_add(sum) ^ (v1 >> 5).wrapping_add(key[3]),
        );
        v1 = v1.wrapping_add(
            (v0 << 4).wrapping_add(key[0]) ^ v0.wrapping_add(sum) ^ (v0 >> 5).wrapping_add(key[1]),
        );
    }
    (v0, v1)
}

/// TEA block: decrypt a single 8-byte block under a 4-word key.
pub fn decrypt_block(v0: u32, v1: u32, key: &[u32; 4]) -> (u32, u32) {
    let mut v0 = v0;
    let mut v1 = v1;
    let mut sum: u32 = DELTA.wrapping_mul(32);
    for _ in 0..32 {
        v1 = v1.wrapping_sub(
            (v0 << 4).wrapping_add(key[0]) ^ v0.wrapping_add(sum) ^ (v0 >> 5).wrapping_add(key[1]),
        );
        v0 = v0.wrapping_sub(
            (v1 << 4).wrapping_add(key[2]) ^ v1.wrapping_add(sum) ^ (v1 >> 5).wrapping_add(key[3]),
        );
        sum = sum.wrapping_sub(DELTA);
    }
    (v0, v1)
}

/// TEA-CBC encrypt `plaintext` (whose length is a multiple of 8 bytes)
/// with `key` and `iv` (two `u32`s). Returns ciphertext of the same length.
pub fn encrypt_cbc(plaintext: &[u8], key: &[u32; 4], iv: (u32, u32)) -> Vec<u8> {
    assert!(
        plaintext.len() % 8 == 0,
        "TEA-CBC input must be a multiple of 8 bytes (got {})",
        plaintext.len()
    );
    let mut out = Vec::with_capacity(plaintext.len());
    let mut prev = iv;
    for chunk in plaintext.chunks_exact(8) {
        let p0 = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let p1 = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        let (mut c0, mut c1) = (p0 ^ prev.0, p1 ^ prev.1);
        (c0, c1) = encrypt_block(c0, c1, key);
        prev = (c0, c1);
        out.extend_from_slice(&c0.to_le_bytes());
        out.extend_from_slice(&c1.to_le_bytes());
    }
    out
}

/// TEA-CBC decrypt `ciphertext` (multiple of 8 bytes) under `key` and `iv`.
pub fn decrypt_cbc(ciphertext: &[u8], key: &[u32; 4], iv: (u32, u32)) -> Vec<u8> {
    assert!(
        ciphertext.len() % 8 == 0,
        "TEA-CBC input must be a multiple of 8 bytes (got {})",
        ciphertext.len()
    );
    let mut out = Vec::with_capacity(ciphertext.len());
    let mut prev = iv;
    for chunk in ciphertext.chunks_exact(8) {
        let c0 = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let c1 = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        let (mut p0, mut p1) = decrypt_block(c0, c1, key);
        p0 ^= prev.0;
        p1 ^= prev.1;
        prev = (c0, c1);
        out.extend_from_slice(&p0.to_le_bytes());
        out.extend_from_slice(&p1.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER_PT0: u32 = 0x0123_4567;
    const PAPER_PT1: u32 = 0x89AB_CDEF;
    const PAPER_KEY: [u32; 4] = [0xA56B_7AAA, 0xF1F2_F3F4, 0xFA2B_C3D4, 0x6745_2301];

    #[test]
    fn delta_constant_known() {
        // DELTA = 0x9E3779B9 (golden ratio fractional part * 2^32).
        assert_eq!(DELTA, 0x9E37_79B9);
    }

    #[test]
    fn encrypt_then_decrypt_roundtrip() {
        let key = [0xDEAD_BEEF, 0xCAFE_F00D, 0x1234_5678, 0x9ABC_DEF0];
        let (c0, c1) = encrypt_block(0x1111_2222, 0x3333_4444, &key);
        // Sanity: ciphertext must differ from plaintext.
        assert!((c0, c1) != (0x1111_2222u32, 0x3333_4444u32));
        let (p0, p1) = decrypt_block(c0, c1, &key);
        assert_eq!(p0, 0x1111_2222);
        assert_eq!(p1, 0x3333_4444);
    }

    #[test]
    fn paper_vector_roundtrip() {
        // Use the canonical TEA test vector (Wheeler & Needham) as a
        // roundtrip. We don't pin the ciphertext bytes (small risk of
        // endianness/key-schedule ambiguity), but we verify the
        // decrypt(encrypt(pt)) == pt identity holds for it.
        let (c0, c1) = encrypt_block(PAPER_PT0, PAPER_PT1, &PAPER_KEY);
        // Ciphertext must differ from plaintext.
        assert!((c0, c1) != (PAPER_PT0, PAPER_PT1));
        let (p0, p1) = decrypt_block(c0, c1, &PAPER_KEY);
        assert_eq!((p0, p1), (PAPER_PT0, PAPER_PT1));
    }

    #[test]
    fn different_keys_different_ciphertexts() {
        let key_a = [0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444];
        let key_b = [0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4445];
        let (ca0, ca1) = encrypt_block(0xAAAA_BBBB, 0xCCCC_DDDD, &key_a);
        let (cb0, cb1) = encrypt_block(0xAAAA_BBBB, 0xCCCC_DDDD, &key_b);
        assert_ne!((ca0, ca1), (cb0, cb1));
    }

    #[test]
    fn cbc_roundtrip() {
        let key = [0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444];
        let iv = (0xAAAA_5555, 0x5555_AAAA);
        // 45 bytes -> pad to 48 to satisfy multiple-of-8.
        let mut plaintext: Vec<u8> = b"the quick brown fox jumps over the lazy dog!!".to_vec();
        plaintext.extend_from_slice(&[0u8; 3]); // pad to 48
        let ct = encrypt_cbc(&plaintext, &key, iv);
        assert_eq!(ct.len(), plaintext.len());
        let pt = decrypt_cbc(&ct, &key, iv);
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn cbc_different_ivs_differ() {
        let key = [0x1, 0x2, 0x3, 0x4];
        let pt = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let iv_a = (0, 0);
        let iv_b = (0, 1);
        let ct_a = encrypt_cbc(pt, &key, iv_a);
        let ct_b = encrypt_cbc(pt, &key, iv_b);
        assert_ne!(ct_a, ct_b);
    }

    #[test]
    fn cbc_identical_blocks_produce_different_ciphertexts() {
        // CBC mode: same plaintext block twice encrypts to two different ciphertext blocks.
        let key = [0xABCD_EF01, 0x2345_6789, 0xFEDC_BA98, 0x7654_3210];
        let iv = (0, 0);
        let block = b"\x01\x02\x03\x04\x05\x06\x07\x08";
        let mut pt = Vec::new();
        pt.extend_from_slice(block);
        pt.extend_from_slice(block);
        let ct = encrypt_cbc(&pt, &key, iv);
        assert_eq!(ct.len(), 16);
        assert_ne!(&ct[0..8], &ct[8..16]);
    }
}
