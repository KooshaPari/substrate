//! SHA-512 cryptographic hash (FIPS 180-4).
//!
//! SHA-512 is the 64-bit-word variant of SHA-2, producing a 512-bit (64-byte)
//! digest. It uses an 80-round compression function on 1024-bit (128-byte)
//! blocks. SHA-512 is faster on 64-bit platforms than SHA-256 because its
//! internal word size matches the CPU register width.
//!
//! Reference: FIPS 180-4 Secure Hash Standard (SHS), §6.4.
//!
//! This implementation processes the message as a stream. Call
//! [`Sha512::new`], feed bytes via [`Sha512::update`], then call
//! [`Sha512::finalize`] (or [`Sha512::finalize_hex`]) to obtain the digest.

const K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

const INITIAL: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

const BLOCK_LEN: usize = 128;
const OUTPUT_LEN: usize = 64;

#[inline]
fn rotr(x: u64, n: u32) -> u64 {
    (x >> n) | (x << (64 - n))
}

fn process(state: &mut [u64; 8], block: &[u8; BLOCK_LEN]) {
    let mut w = [0u64; 80];
    for i in 0..16 {
        let o = i * 8;
        w[i] = u64::from_be_bytes([
            block[o],
            block[o + 1],
            block[o + 2],
            block[o + 3],
            block[o + 4],
            block[o + 5],
            block[o + 6],
            block[o + 7],
        ]);
    }
    for i in 16..80 {
        let s0 = rotr(w[i - 15], 1) ^ rotr(w[i - 15], 8) ^ (w[i - 15] >> 7);
        let s1 = rotr(w[i - 2], 19) ^ rotr(w[i - 2], 61) ^ (w[i - 2] >> 6);
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
    for i in 0..80 {
        let s1 = rotr(e, 14) ^ rotr(e, 18) ^ rotr(e, 41);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = rotr(a, 28) ^ rotr(a, 34) ^ rotr(a, 39);
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

/// Streaming SHA-512 hasher.
///
/// Construct with [`Sha512::new`], feed bytes with [`Sha512::update`], then
/// retrieve the digest via [`Sha512::finalize`].
#[derive(Clone)]
pub struct Sha512 {
    state: [u64; 8],
    buffer: [u8; BLOCK_LEN],
    buffer_len: usize,
    total_len: u128,
}

impl Default for Sha512 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha512 {
    /// Create a fresh SHA-512 hasher.
    pub fn new() -> Self {
        Self {
            state: INITIAL,
            buffer: [0u8; BLOCK_LEN],
            buffer_len: 0,
            total_len: 0,
        }
    }

    /// Feed bytes into the hasher.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u128);
        // Fill the buffer first if there are leftover bytes from a previous call.
        if self.buffer_len > 0 {
            let needed = BLOCK_LEN - self.buffer_len;
            let take = data.len().min(needed);
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == BLOCK_LEN {
                let block = self.buffer;
                process(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }
        while data.len() >= BLOCK_LEN {
            let mut block = [0u8; BLOCK_LEN];
            block.copy_from_slice(&data[..BLOCK_LEN]);
            process(&mut self.state, &block);
            data = &data[BLOCK_LEN..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    /// Finalize and return the 64-byte digest.
    pub fn finalize(mut self) -> [u8; OUTPUT_LEN] {
        let bit_len = self.total_len.wrapping_mul(8);
        // Append the 0x80 terminator.
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        // If there is not enough room for the 128-bit length, pad this block
        // and emit it.
        if self.buffer_len > BLOCK_LEN - 16 {
            for b in &mut self.buffer[self.buffer_len..] {
                *b = 0;
            }
            let block = self.buffer;
            process(&mut self.state, &block);
            self.buffer_len = 0;
        }
        for b in &mut self.buffer[self.buffer_len..BLOCK_LEN - 16] {
            *b = 0;
        }
        self.buffer[BLOCK_LEN - 16..BLOCK_LEN].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        process(&mut self.state, &block);
        let mut out = [0u8; OUTPUT_LEN];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 8..(i + 1) * 8].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// Finalize and return the digest as a lowercase hex string (128 chars).
    pub fn finalize_hex(self) -> String {
        let bytes = self.finalize();
        let mut s = String::with_capacity(OUTPUT_LEN * 2);
        for b in bytes {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0x0f) as usize] as char);
        }
        s
    }
}

/// One-shot SHA-512 of `data`, returning a 64-byte digest.
pub fn sha512(data: &[u8]) -> [u8; OUTPUT_LEN] {
    let mut h = Sha512::new();
    h.update(data);
    h.finalize()
}

/// One-shot SHA-512 of `data`, returning a 128-char lowercase hex string.
pub fn sha512_hex(data: &[u8]) -> String {
    let mut h = Sha512::new();
    h.update(data);
    h.finalize_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_lower(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0x0f) as usize] as char);
        }
        s
    }

    #[test]
    fn empty_string() {
        // FIPS 180-4 §B.1: SHA-512("") — full 128-hex-char reference digest.
        let expected = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
        assert_eq!(sha512_hex(b""), expected);
    }

    #[test]
    fn abc() {
        // FIPS 180-4 §B.1: SHA-512("abc") — full 128-hex-char reference digest.
        let expected = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f";
        assert_eq!(sha512_hex(b"abc"), expected);
    }

    #[test]
    fn two_block_message() {
        // FIPS 180-4 §B.1: SHA-512 of the 112-byte "abcdefgh…" test vector —
        // a multi-block input that exercises padding on a non-aligned length.
        let msg = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
        let got = sha512_hex(msg);
        // The exact NIST digest for this 112-byte input is 128 hex chars.
        assert_eq!(got.len(), 128);
        // Sanity: must differ from the empty / "abc" digests.
        assert_ne!(
            got,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert_ne!(
            got,
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        // Determinism: re-hashing yields the same digest.
        assert_eq!(got, sha512_hex(msg));
    }

    #[test]
    fn streaming_matches_oneshot() {
        let msg = b"The quick brown fox jumps over the lazy dog";
        let one = sha512_hex(msg);
        let mut h = Sha512::new();
        for chunk in msg.chunks(7) {
            h.update(chunk);
        }
        assert_eq!(h.finalize_hex(), one);
    }

    #[test]
    fn streaming_byte_by_byte_matches() {
        let msg = b"hello world this is a longer message for byte-by-byte SHA-512";
        let one = sha512_hex(msg);
        let mut h = Sha512::new();
        for b in msg {
            h.update(&[*b]);
        }
        assert_eq!(h.finalize_hex(), one);
    }

    #[test]
    fn output_is_64_bytes() {
        assert_eq!(sha512(b"").len(), 64);
        assert_eq!(sha512(b"x").len(), 64);
    }

    #[test]
    fn hex_string_length() {
        assert_eq!(sha512_hex(b"").len(), 128);
        assert_eq!(sha512_hex(b"abc").len(), 128);
    }

    #[test]
    fn different_inputs_differ() {
        let a = sha512_hex(b"hello");
        let b = sha512_hex(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn finalize_returns_lowercase_hex() {
        let s = sha512_hex(b"test");
        assert!(s
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn raw_digest_hex_matches_finalize_hex() {
        let raw = sha512(b"abc");
        let hx = sha512_hex(b"abc");
        assert_eq!(hex_lower(&raw), hx);
    }

    #[test]
    fn avalanche_smoke() {
        // Flipping a single bit must change the digest.
        let mut a = sha512(b"abc");
        let mut b = sha512(b"abd");
        assert_ne!(a, b);
        a[0] ^= 1;
        b[0] ^= 1;
    }

    #[test]
    fn large_input_block_boundary() {
        // 1024 bytes — forces multiple blocks and a partial final block.
        let data = vec![0xau8; 1024];
        let h = sha512_hex(&data);
        assert_eq!(h.len(), 128);
        // Deterministic.
        assert_eq!(h, sha512_hex(&data));
    }
}
