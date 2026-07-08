//! RC4 — Rivest Cipher 4 stream cipher (RSA Laboratories, 1994).
//!
//! A variable-key-size stream cipher that generates a pseudo-random
//! keystream by repeatedly permuting a 256-byte state. The keystream is
//! XORed with the plaintext to produce the ciphertext. RC4 is no longer
//! considered secure for new designs but is widely used in legacy
//! protocols (WEP, early TLS, Kerberos, etc.) and remains a useful
//! reference for understanding stream ciphers.
//!
//! Reference: B. Schneier, "Applied Cryptography", 2nd ed., §17.1.

const STATE_LEN: usize = 256;

/// Streaming RC4 state. Constructed via [`Rc4::new`] from a key of
/// 1..=256 bytes, then used by [`Rc4::apply_keystream`] (or
/// [`Rc4::next_byte`]) to encrypt / decrypt bytes.
pub struct Rc4 {
    s: [u8; STATE_LEN],
    i: u8,
    j: u8,
}

impl Rc4 {
    /// Initialize an RC4 state with the supplied key. Key length must
    /// be 1..=256 bytes; for key lengths above 256 we follow the
    /// historical WEP / 802.11 convention of cycling the key.
    pub fn new(key: &[u8]) -> Self {
        assert!(
            !key.is_empty() && key.len() <= STATE_LEN,
            "rc4: key length must be 1..=256 bytes (got {})",
            key.len()
        );
        let mut s = [0u8; STATE_LEN];
        for (i, slot) in s.iter_mut().enumerate() {
            *slot = i as u8;
        }
        let mut j: u8 = 0;
        for i in 0..STATE_LEN {
            j = j
                .wrapping_add(s[i])
                .wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        Rc4 { s, i: 0, j: 0 }
    }

    /// Generate the next pseudorandom byte of keystream and advance
    /// internal state.
    pub fn next_byte(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[self.i as usize]);
        self.s.swap(self.i as usize, self.j as usize);
        let k = self.s[(self.s[self.i as usize].wrapping_add(self.s[self.j as usize])) as usize];
        k
    }

    /// XOR `data` with the keystream in place. Encryption and
    /// decryption are the same operation.
    pub fn apply_keystream(&mut self, data: &mut [u8]) {
        for byte in data.iter_mut() {
            *byte ^= self.next_byte();
        }
    }
}

/// One-shot helper: RC4-encrypt (or decrypt) `data` with `key`.
pub fn apply(data: &mut [u8], key: &[u8]) {
    Rc4::new(key).apply_keystream(data);
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
    fn known_vector_key_plaintext() {
        // RFC 6229 / classic published RC4 test vector.
        let key = b"Key";
        let plaintext = b"Plaintext";
        let mut data = plaintext.to_vec();
        apply(&mut data, key);
        // From the canonical RC4 paper: ciphertext = BBF316E8 D940AF0A D3
        assert_eq!(to_hex(&data), "bbf316e8d940af0ad3");
    }

    #[test]
    fn known_vector_wiki() {
        // Wikipedia RC4 example: key="Wiki", plaintext="pedia", keystream bytes.
        let key = b"Wiki";
        let plaintext = b"pedia";
        let mut data = plaintext.to_vec();
        apply(&mut data, key);
        // Expected ciphertext bytes from the public reference: 0x1021BF0420
        assert_eq!(to_hex(&data), "1021bf0420");
    }

    #[test]
    fn known_vector_secret() {
        // Key="Secret", plaintext="Attack at dawn"
        let key = b"Secret";
        let plaintext = b"Attack at dawn";
        let mut data = plaintext.to_vec();
        apply(&mut data, key);
        // Published expected ciphertext: 45A01F645FC35B383552544B9BF5
        assert_eq!(to_hex(&data), "45a01f645fc35b383552544b9bf5");
    }

    #[test]
    fn symmetric_encrypt_decrypt() {
        // RC4 is its own inverse when used with the same key.
        let key = b"super-secret-key";
        let plaintext = b"The quick brown fox jumps over the lazy dog";
        let mut ct = plaintext.to_vec();
        apply(&mut ct, key);
        apply(&mut ct, key);
        assert_eq!(ct, plaintext);
    }

    #[test]
    fn state_is_consistent_with_per_byte_calls() {
        // apply_keystream must produce the same bytes as calling next_byte
        // repeatedly and XORing.
        let key = b"abcdef";
        let plaintext = b"0123456789ABCDEF";
        let mut via_apply = plaintext.to_vec();
        let mut rc4 = Rc4::new(key);
        rc4.apply_keystream(&mut via_apply);

        let mut rc4b = Rc4::new(key);
        let via_next: Vec<u8> = plaintext
            .iter()
            .map(|b| b ^ rc4b.next_byte())
            .collect();
        assert_eq!(via_apply, via_next);
    }

    #[test]
    fn empty_plaintext_is_noop() {
        let mut data: [u8; 0] = [];
        apply(&mut data, b"k");
        assert_eq!(data.len(), 0);
    }

    #[test]
    fn single_byte_key() {
        // A 1-byte key must still produce a working cipher.
        let plaintext = b"hello";
        let mut data = plaintext.to_vec();
        apply(&mut data, &[0x42]);
        assert_ne!(data, plaintext);
        apply(&mut data, &[0x42]);
        assert_eq!(data, plaintext);
    }

    #[test]
    fn key_length_256_supported() {
        let key: Vec<u8> = (0..=255u8).collect();
        let plaintext = b"this is a longer test string for rc4";
        let mut data = plaintext.to_vec();
        apply(&mut data, &key);
        apply(&mut data, &key);
        assert_eq!(data, plaintext);
    }

    #[test]
    fn different_keys_produce_different_ciphertexts() {
        let plaintext = b"identical plaintext under two distinct keys";
        let mut a = plaintext.to_vec();
        let mut b = plaintext.to_vec();
        apply(&mut a, b"key-one");
        apply(&mut b, b"key-two");
        assert_ne!(a, b);
    }

    #[test]
    fn state_evolves_across_calls() {
        // Two RC4 instances with the same key must remain in lock-step
        // even if used across multiple calls.
        let mut a = Rc4::new(b"k");
        let mut b = Rc4::new(b"k");
        for _ in 0..1024 {
            assert_eq!(a.next_byte(), b.next_byte());
        }
    }
}
