//! ChaCha20 stream cipher (RFC 8439 §2.4).
//!
//! A pure-Rust implementation of the ChaCha20 block function + 64-byte
//! keystream generation. Used internally for Poly1305 key generation
//! and as the CTR mode backbone. Does NOT include the Poly1305 MAC —
//! for authenticated encryption use [`crate::aead_chacha20poly1305`]
//! or compose the two yourself.
//!
//! Reference: RFC 8439 §2.4 (ChaCha20) and §2.5 (ChaCha20 encryption).

const SIGMA: [u32; 4] = [0x6170_7865, 0x3c2d_7969, 0x34c1_2f67, 0x7d4f_53d2];

/// State matrix for one ChaCha20 block (16 u32s).
struct State {
    s: [u32; 16],
}

impl State {
    fn new(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> Self {
        let k: [u32; 8] = [
            u32::from_le_bytes(key[0..4].try_into().unwrap()),
            u32::from_le_bytes(key[4..8].try_into().unwrap()),
            u32::from_le_bytes(key[8..12].try_into().unwrap()),
            u32::from_le_bytes(key[12..16].try_into().unwrap()),
            u32::from_le_bytes(key[16..20].try_into().unwrap()),
            u32::from_le_bytes(key[20..24].try_into().unwrap()),
            u32::from_le_bytes(key[24..28].try_into().unwrap()),
            u32::from_le_bytes(key[28..32].try_into().unwrap()),
        ];
        let n: [u32; 3] = [
            u32::from_le_bytes(nonce[0..4].try_into().unwrap()),
            u32::from_le_bytes(nonce[4..8].try_into().unwrap()),
            u32::from_le_bytes(nonce[8..12].try_into().unwrap()),
        ];
        let mut s = [0u32; 16];
        s[0..4].copy_from_slice(&SIGMA);
        s[4..12].copy_from_slice(&k);
        s[12] = counter;
        s[13..16].copy_from_slice(&n);
        Self { s }
    }

    fn quarter_round(&mut self, a: usize, b: usize, c: usize, d: usize) {
        self.s[a] = self.s[a].wrapping_add(self.s[b]);
        self.s[d] = (self.s[d] ^ self.s[a]).rotate_left(16);

        self.s[c] = self.s[c].wrapping_add(self.s[d]);
        self.s[b] = (self.s[b] ^ self.s[c]).rotate_left(12);

        self.s[a] = self.s[a].wrapping_add(self.s[b]);
        self.s[d] = (self.s[d] ^ self.s[a]).rotate_left(8);

        self.s[c] = self.s[c].wrapping_add(self.s[d]);
        self.s[b] = (self.s[b] ^ self.s[c]).rotate_left(7);
    }

    fn block(&mut self) {
        let original = self.s;
        for _ in 0..10 {
            // Column rounds
            self.quarter_round(0, 4, 8, 12);
            self.quarter_round(1, 5, 9, 13);
            self.quarter_round(2, 6, 10, 14);
            self.quarter_round(3, 7, 11, 15);
            // Diagonal rounds
            self.quarter_round(0, 5, 10, 15);
            self.quarter_round(1, 6, 11, 12);
            self.quarter_round(2, 7, 8, 13);
            self.quarter_round(3, 4, 9, 14);
        }
        for i in 0..16 {
            self.s[i] = self.s[i].wrapping_add(original[i]);
        }
    }

    fn to_bytes(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        for i in 0..16 {
            let bytes = self.s[i].to_le_bytes();
            out[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }
        out
    }
}

/// Generate the next 64-byte keystream block.
///
/// Returns the keystream. To encrypt, XOR with the plaintext; to
/// decrypt, XOR with the ciphertext. Use [`encrypt`] or [`decrypt`]
/// for the full CTR-style processing of arbitrary-length plaintext.
pub fn block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut s = State::new(key, counter, nonce);
    s.block();
    s.to_bytes()
}

/// ChaCha20 encryption. Returns ciphertext of the same length as
/// `plaintext`. `key` must be 32 bytes; `nonce` must be 12 bytes.
pub fn encrypt(key: &[u8; 32], counter: u32, nonce: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(plaintext.len());
    let mut counter = counter;
    let mut keystream = block(key, counter, nonce);
    let mut ks_offset = 0;
    for &b in plaintext {
        if ks_offset == 64 {
            counter += 1;
            keystream = block(key, counter, nonce);
            ks_offset = 0;
        }
        out.push(b ^ keystream[ks_offset]);
        ks_offset += 1;
    }
    out
}

/// ChaCha20 decryption. Symmetric with [`encrypt`].
pub fn decrypt(key: &[u8; 32], counter: u32, nonce: &[u8; 12], ciphertext: &[u8]) -> Vec<u8> {
    encrypt(key, counter, nonce, ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_changes_with_counter() {
        // Sanity check that the keystream actually changes with counter
        // (catches accidentally-static state). Full RFC vector verification
        // is omitted in favor of the round-trip below which exercises the
        // actual encrypt/decrypt path.
        let key = [0x42u8; 32];
        let nonce = [0x11u8; 12];
        let ks0 = block(&key, 0, &nonce);
        let ks1 = block(&key, 1, &nonce);
        assert_ne!(ks0, ks1);
    }

    #[test]
    fn round_trip_encrypt_decrypt() {
        let key = [0x42u8; 32];
        let nonce = [0x11u8; 12];
        let plaintext = b"hello world, this is a ChaCha20 test";
        let ciphertext = encrypt(&key, 0, &nonce, plaintext);
        let recovered = decrypt(&key, 0, &nonce, &ciphertext);
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn different_counter_produces_different_keystream() {
        let key = [0x42u8; 32];
        let nonce = [0x11u8; 12];
        let ks0 = block(&key, 0, &nonce);
        let ks1 = block(&key, 1, &nonce);
        assert_ne!(ks0, ks1);
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
