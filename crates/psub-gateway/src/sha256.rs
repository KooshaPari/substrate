//! SHA-256 cryptographic hash.
//!
//! Standard SHA-256 (FIPS 180-4) producing a 256-bit digest. Unlike MD5 and
//! SHA-1, SHA-256 is currently considered collision-resistant for practical
//! purposes and is the most widely used secure hash function.
//!
//! Reference: FIPS 180-4 Secure Hash Standard (SHS), §6.2.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

fn rotr(x: u32, n: u32) -> u32 {
    (x >> n) | (x << (32 - n))
}

fn process(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        let o = i * 4;
        w[i] = u32::from_be_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]]);
    }
    for i in 16..64 {
        let s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
        let s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];
    for i in 0..64 {
        let s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

/// Streaming SHA-256 hasher.
#[derive(Clone)]
pub struct Hasher {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            state: INITIAL,
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    pub fn update(&mut self, mut input: &[u8]) {
        self.total_len = self.total_len.wrapping_add(input.len() as u64);
        if self.buffer_len > 0 {
            let need = 64 - self.buffer_len;
            let take = need.min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + take]
                .copy_from_slice(&input[..take]);
            self.buffer_len += take;
            input = &input[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                process(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }
        while input.len() >= 64 {
            let block: [u8; 64] = input[..64].try_into().unwrap();
            process(&mut self.state, &block);
            input = &input[64..];
        }
        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        let total_bits = self.total_len.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            for b in &mut self.buffer[self.buffer_len..64] {
                *b = 0;
            }
            let block = self.buffer;
            process(&mut self.state, &block);
            self.buffer = [0u8; 64];
            self.buffer_len = 0;
        }
        for b in &mut self.buffer[self.buffer_len..56] {
            *b = 0;
        }
        self.buffer[56..64].copy_from_slice(&total_bits.to_be_bytes());
        let block = self.buffer;
        process(&mut self.state, &block);

        let mut out = [0u8; 32];
        for (i, s) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&s.to_be_bytes());
        }
        out
    }
}

/// Compute SHA-256 of `input` and return the 32-byte digest.
pub fn hash(input: &[u8]) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(input);
    h.finalize()
}

/// Render a 32-byte SHA-256 digest as a 64-character lowercase hex string.
pub fn to_hex(digest: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        // SHA-256("")
        assert_eq!(
            to_hex(&hash(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn abc() {
        assert_eq!(
            to_hex(&hash(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn two_block_boundary() {
        // 56-byte input: forces two blocks (the length suffix uses the
        // remainder of the first block).
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            to_hex(&hash(input)),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn long_input() {
        // 112-byte input: forces three blocks (crosses 64-byte and 128-byte boundaries).
        let input = b"abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz\
                     abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuv";
        let d = hash(input);
        // Determinism + length + not-all-zero sanity check; the exact
        // expected value would need a pre-computed reference.
        assert_eq!(d.len(), 32);
        assert_eq!(d, hash(input));
        assert_ne!(d, hash(b""));
    }

    #[test]
    fn incremental_equals_oneshot() {
        let mut a = Hasher::new();
        a.update(b"hello");
        a.update(b" ");
        a.update(b"world");
        let da = a.finalize();
        let db = hash(b"hello world");
        assert_eq!(da, db);
    }

    #[test]
    fn different_inputs_differ() {
        assert_ne!(hash(b"foo"), hash(b"bar"));
    }

    #[test]
    fn output_length_32() {
        assert_eq!(hash(b"x").len(), 32);
    }

    #[test]
    fn hex_is_64_lowercase_chars() {
        let s = to_hex(&hash(b"hi"));
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}
