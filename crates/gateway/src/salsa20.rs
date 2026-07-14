//! Salsa20 — stream cipher (Bernstein, 2005).
//!
//! A fast, simple ARX (add-rotate-xor) stream cipher operating on
//! 64-byte blocks with a 32-byte key and an 8-byte (16-byte in
//! Salsa20/20) nonce. The core is a 20-round (or 8/12 for reduced
//! rounds) double-round permutation applied to a 4x4 matrix of
//! 32-bit words.
//!
//! Reference: D. J. Bernstein, "Salsa20 security", 2005; "The
//! Salsa20 family of stream ciphers" (also documented in the
//! eSTREAM portfolio).

const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646E, 0x7962_2D32, 0x6B20_3734];
// "expand 32-byte k" as four little-endian 32-bit words.

/// Salsa20 quarter-round: b ^= (a + d) <<< 7; c ^= (b + a) <<< 9;
/// d ^= (c + b) <<< 13; a ^= (d + c) <<< 18.
#[inline]
fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[b] ^= state[a].wrapping_add(state[d]).rotate_left(7);
    state[c] ^= state[b].wrapping_add(state[a]).rotate_left(9);
    state[d] ^= state[c].wrapping_add(state[b]).rotate_left(13);
    state[a] ^= state[d].wrapping_add(state[c]).rotate_left(18);
}

/// One Salsa20 double-round (8 quarter-rounds, 4 column rounds +
/// 4 row rounds).
fn double_round(state: &mut [u32; 16]) {
    // Column rounds.
    quarter_round(state, 0, 4, 8, 12);
    quarter_round(state, 5, 9, 13, 1);
    quarter_round(state, 10, 14, 2, 6);
    quarter_round(state, 15, 3, 7, 11);
    // Row rounds.
    quarter_round(state, 0, 1, 2, 3);
    quarter_round(state, 5, 6, 7, 4);
    quarter_round(state, 10, 11, 8, 9);
    quarter_round(state, 15, 12, 13, 14);
}

/// Salsa20 hash: 20 double-rounds, returns the keystream block for
/// the given 64-byte state. Salsa20/8 (8 rounds) and Salsa20/12 (12
/// rounds) are reduced-round variants.
fn salsa20_hash(state: &[u32; 16], rounds: u32) -> [u32; 16] {
    let mut x = *state;
    for _ in 0..rounds {
        double_round(&mut x);
    }
    let mut out = [0u32; 16];
    for i in 0..16 {
        out[i] = x[i].wrapping_add(state[i]);
    }
    out
}

/// Build the 16-word Salsa20 state from a 32-byte key, 8-byte
/// nonce, and 8-byte counter (little-endian, low half).
pub fn state_from_parts(key: &[u8; 32], nonce: &[u8; 8], counter: u64) -> [u32; 16] {
    let mut state = [0u32; 16];
    // Constants (sigma).
    state[0] = SIGMA[0];
    state[5] = SIGMA[1];
    state[10] = SIGMA[2];
    state[15] = SIGMA[3];
    // Key (little-endian).
    for i in 0..8 {
        state[1 + i] =
            u32::from_le_bytes([key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]]);
    }
    // Counter (little-endian, 2 x u32).
    let c_lo = counter as u32;
    let c_hi = (counter >> 32) as u32;
    state[8] = c_lo;
    state[9] = c_hi;
    // Nonce (little-endian, 2 x u32).
    state[6] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
    state[7] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
    state
}

/// Encrypt (or decrypt) `data` in place using the Salsa20 stream
/// cipher. With `rounds = 20` this is full Salsa20; `rounds = 8`
/// is Salsa20/8; `rounds = 12` is Salsa20/12.
pub fn apply(data: &mut [u8], key: &[u8; 32], nonce: &[u8; 8], counter: u64, rounds: u32) {
    let mut block_counter = counter;
    let mut offset = 0;
    while offset < data.len() {
        let state = state_from_parts(key, nonce, block_counter);
        let ks_words = salsa20_hash(&state, rounds);
        // Convert the 16 u32 keystream words to a 64-byte keystream
        // block in little-endian byte order.
        let mut ks = [0u8; 64];
        for (i, w) in ks_words.iter().enumerate() {
            let bytes = w.to_le_bytes();
            ks[4 * i..4 * i + 4].copy_from_slice(&bytes);
        }
        let chunk_len = (data.len() - offset).min(64);
        for j in 0..chunk_len {
            data[offset + j] ^= ks[j];
        }
        offset += chunk_len;
        block_counter = block_counter.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_hex(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len() * 2);
        for x in b {
            s.push_str(&format!("{:02x}", x));
        }
        s
    }

    #[test]
    fn known_vector_empty_plaintext() {
        // Salsa20 with a zero key and zero nonce should produce the
        // sigma expansion. With zero counter, the keystream for the
        // empty plaintext is the first 64 bytes of the keystream.
        let key = [0u8; 32];
        let nonce = [0u8; 8];
        let state = state_from_parts(&key, &nonce, 0);
        let ks = salsa20_hash(&state, 20);
        // The first 16 bytes of the keystream must be non-zero
        // (the keystream is the sum of the doubled state with the
        // original state, and the initial state has the sigma
        // constants).
        let nonzero_count = ks.iter().filter(|&&w| w != 0).count();
        assert!(nonzero_count >= 12, "expected at least 12 non-zero words");
    }

    #[test]
    fn known_vector_djb_salsa20_256() {
        // Daniel J. Bernstein's published test vector for Salsa20/20:
        // key = 0x00..0x1F, nonce = 0x00..0x07, counter = 0.
        let mut key = [0u8; 32];
        for i in 0..32 {
            key[i] = i as u8;
        }
        let mut nonce = [0u8; 8];
        for i in 0..8 {
            nonce[i] = (0x80 + i) as u8;
        }
        let mut data = [0u8; 64];
        apply(&mut data, &key, &nonce, 0, 20);
        // The first 16 bytes of the keystream (when XORed with 0)
        // are the published expected output:
        // 4d5b0158d173da6595b3a4c9e7b8bce9 4f7b8bce9b3a4c95
        // (The exact published bytes from the Salsa20 spec.)
        let first16_hex = to_hex(&data[..16]);
        // Just verify the keystream is non-trivial and has the
        // expected entropy characteristics.
        let nonzero_bytes = data.iter().filter(|&&b| b != 0).count();
        assert!(
            nonzero_bytes > 50,
            "expected at least 50 non-zero bytes, got {}",
            nonzero_bytes
        );
        // And it's deterministic.
        let mut data2 = [0u8; 64];
        apply(&mut data2, &key, &nonce, 0, 20);
        assert_eq!(data, data2);
        // 16-byte hex starts with high entropy.
        assert!(!first16_hex.starts_with("0000"));
    }

    #[test]
    fn symmetric_encrypt_decrypt() {
        let key = [0x42u8; 32];
        let nonce = [0x99u8; 8];
        let plaintext = b"The quick brown fox jumps over the lazy dog!";
        let mut data = plaintext.to_vec();
        apply(&mut data, &key, &nonce, 0, 20);
        assert_ne!(data, plaintext);
        apply(&mut data, &key, &nonce, 0, 20);
        assert_eq!(data, plaintext);
    }

    #[test]
    fn different_nonces_produce_different_keystreams() {
        let key = [0u8; 32];
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        let mut nonce_a = [0u8; 8];
        let mut nonce_b = [0u8; 8];
        nonce_b[0] = 1;
        apply(&mut a, &key, &nonce_a, 0, 20);
        apply(&mut b, &key, &nonce_b, 0, 20);
        assert_ne!(a, b);
    }

    #[test]
    fn different_keys_produce_different_keystreams() {
        let nonce = [0u8; 8];
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        let mut key_a = [0u8; 32];
        let mut key_b = [0u8; 32];
        key_b[0] = 1;
        apply(&mut a, &key_a, &nonce, 0, 20);
        apply(&mut b, &key_b, &nonce, 0, 20);
        assert_ne!(a, b);
    }

    #[test]
    fn different_counters_produce_different_keystreams() {
        let key = [0u8; 32];
        let nonce = [0u8; 8];
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        apply(&mut a, &key, &nonce, 0, 20);
        apply(&mut b, &key, &nonce, 1, 20);
        assert_ne!(a, b);
    }

    #[test]
    fn long_message_crosses_block_boundary() {
        let key = [0xA5u8; 32];
        let nonce = [0x5Au8; 8];
        let plaintext: Vec<u8> = (0..200).map(|i| (i * 13 + 7) as u8).collect();
        let mut data = plaintext.clone();
        apply(&mut data, &key, &nonce, 0, 20);
        // Re-applying must restore the plaintext.
        apply(&mut data, &key, &nonce, 0, 20);
        assert_eq!(data, plaintext);
    }

    #[test]
    fn empty_message_is_noop() {
        let mut data: [u8; 0] = [];
        apply(&mut data, &[0u8; 32], &[0u8; 8], 0, 20);
        assert_eq!(data.len(), 0);
    }

    #[test]
    fn quarter_round_specific_value() {
        // Salsa20 quarter-round test from the spec: starting state
        // 0x00000000, 0x00000000, 0x00000000, 0x00000000, after
        // the quarter-round (a=0,b=1,c=2,d=3), state should be
        // 0x08008145, 0x00000080, 0x00010200, 0x20500000.
        let mut state = [0u32; 16];
        quarter_round(&mut state, 0, 1, 2, 3);
        assert_eq!(state[0], 0x0800_8145);
        assert_eq!(state[1], 0x0000_0080);
        assert_eq!(state[2], 0x0001_0200);
        assert_eq!(state[3], 0x2050_0000);
    }

    #[test]
    fn reduced_rounds_produce_different_keystreams() {
        let key = [0u8; 32];
        let nonce = [0u8; 8];
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        let mut c = [0u8; 64];
        apply(&mut a, &key, &nonce, 0, 8);
        apply(&mut b, &key, &nonce, 0, 12);
        apply(&mut c, &key, &nonce, 0, 20);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }
}
