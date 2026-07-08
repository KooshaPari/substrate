//! SHA-1 cryptographic hash.
//!
//! SHA-1 (FIPS 180-4) produces a 160-bit digest. Cryptographically broken
//! (collision resistance violated since 2017 — SHAttered), but still used
//! for non-security checksums, Git object hashing, and HMAC-SHA1.
//!
//! Reference: FIPS 180-4 Secure Hash Standard, §6.1.

const INITIAL: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];

fn left_rotate(x: u32, n: u32) -> u32 {
    (x << n) | (x >> (32 - n))
}

fn process_block(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for i in 0..16 {
        let o = i * 4;
        w[i] = u32::from_be_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]]);
    }
    for i in 16..80 {
        w[i] = left_rotate(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);
    }
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    for i in 0..80 {
        let (f, k) = match i {
            0..=19 => ((b & c) | ((!b) & d), 0x5a827999),
            20..=39 => (b ^ c ^ d, 0x6ed9eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
            _ => (b ^ c ^ d, 0xca62c1d6),
        };
        let temp = left_rotate(a, 5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(w[i]);
        e = d;
        d = c;
        c = left_rotate(b, 30);
        b = a;
        a = temp;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

/// Streaming SHA-1 hasher.
#[derive(Clone)]
pub struct Hasher {
    state: [u32; 5],
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
                process_block(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }
        while input.len() >= 64 {
            let block: [u8; 64] = input[..64].try_into().unwrap();
            process_block(&mut self.state, &block);
            input = &input[64..];
        }
        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    pub fn finalize(mut self) -> [u8; 20] {
        let total_bits = self.total_len.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            for b in &mut self.buffer[self.buffer_len..64] {
                *b = 0;
            }
            let block = self.buffer;
            process_block(&mut self.state, &block);
            self.buffer = [0u8; 64];
            self.buffer_len = 0;
        }
        for b in &mut self.buffer[self.buffer_len..56] {
            *b = 0;
        }
        self.buffer[56..64].copy_from_slice(&total_bits.to_be_bytes());
        let block = self.buffer;
        process_block(&mut self.state, &block);

        let mut out = [0u8; 20];
        for (i, s) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&s.to_be_bytes());
        }
        out
    }
}

/// Compute SHA-1 of `input` and return the 20-byte digest.
pub fn hash(input: &[u8]) -> [u8; 20] {
    let mut h = Hasher::new();
    h.update(input);
    h.finalize()
}

/// Render a 20-byte SHA-1 digest as a 40-character lowercase hex string.
pub fn to_hex(digest: &[u8; 20]) -> String {
    let mut s = String::with_capacity(40);
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
        // SHA1("")
        assert_eq!(
            to_hex(&hash(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn abc() {
        assert_eq!(to_hex(&hash(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn hello_world() {
        assert_eq!(
            to_hex(&hash(b"hello world")),
            "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed"
        );
    }

    #[test]
    fn longer_message() {
        // 56-byte input: forces two blocks.
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            to_hex(&hash(input)),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn exact_block_boundary() {
        // 64-byte input: forces length-pad block.
        let data = vec![0xa5u8; 64];
        let d = hash(&data);
        let mut h = Hasher::new();
        h.update(&data);
        assert_eq!(h.finalize(), d);
    }

    #[test]
    fn incremental_equals_oneshot() {
        let mut a = Hasher::new();
        a.update(b"hello");
        a.update(b" ");
        a.update(b"world");
        assert_eq!(a.finalize(), hash(b"hello world"));
    }

    #[test]
    fn block_chunks_5() {
        // Process data in 5-byte chunks; result must match.
        let input = b"The quick brown fox jumps over the lazy dog";
        let mut h = Hasher::new();
        for chunk in input.chunks(5) {
            h.update(chunk);
        }
        assert_eq!(h.finalize(), hash(input));
    }

    #[test]
    fn different_inputs_differ() {
        assert_ne!(hash(b"foo"), hash(b"bar"));
    }

    #[test]
    fn output_length_20() {
        assert_eq!(hash(b"x").len(), 20);
    }

    #[test]
    fn large_input() {
        let data = vec![0x77u8; 10_000];
        assert_eq!(hash(&data).len(), 20);
        assert_eq!(hash(&data), hash(&data));
    }
}
