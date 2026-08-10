//! MD4 message digest (RFC 1320).
//!
//! MD4 produces a 128-bit digest. Designed by Ron Rivest in 1990 as a
//! fast software hash; superseded for cryptographic use by MD5, SHA-1,
//! and ultimately SHA-2/SHA-3 after Dobbertin's 1995–1998 collision
//! attacks. Still useful as a deterministic, well-defined 128-bit
//! fingerprint for non-security checksums, NT-hash (Windows), and
//! legacy data-format identifiers (e.g. eDonkey/ed2k hash, RSVP
//! integrity objects).
//!
//! Reference: R. Rivest, "The MD4 Message Digest Algorithm", RFC 1320,
//! April 1992. Test vectors A.1–A.5 of the RFC are re-derived here.
//!
//! Pure safe Rust, no external dependencies.

const INIT: [u32; 4] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476];

const K0: u32 = 0x0000_0000;
const K1: u32 = 0x5A82_7999;
const K2: u32 = 0x6ED9_EBA1;

#[inline(always)]
fn f(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (!x & z)
}

#[inline(always)]
fn g(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (x & z) | (y & z)
}

#[inline(always)]
fn h(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}

#[inline(always)]
fn rotl(x: u32, n: u32) -> u32 {
    (x << n) | (x >> (32 - n))
}

#[inline(always)]
fn step<F: Fn(u32, u32, u32) -> u32>(
    a: &mut u32,
    b: u32,
    c: u32,
    d: u32,
    x: u32,
    s: u32,
    k: u32,
    f: F,
) {
    *a = rotl(
        a.wrapping_add(f(b, c, d)).wrapping_add(x).wrapping_add(k),
        s,
    );
}

fn process_block(state: &mut [u32; 4], block: &[u8; 64]) {
    let mut x = [0u32; 16];
    for i in 0..16 {
        let o = i * 4;
        x[i] = u32::from_le_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]]);
    }
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];

    // Round 1
    step(&mut a, b, c, d, x[0], 3, K0, f);
    step(&mut d, a, b, c, x[1], 7, K0, f);
    step(&mut c, d, a, b, x[2], 11, K0, f);
    step(&mut b, c, d, a, x[3], 19, K0, f);
    step(&mut a, b, c, d, x[4], 3, K0, f);
    step(&mut d, a, b, c, x[5], 7, K0, f);
    step(&mut c, d, a, b, x[6], 11, K0, f);
    step(&mut b, c, d, a, x[7], 19, K0, f);
    step(&mut a, b, c, d, x[8], 3, K0, f);
    step(&mut d, a, b, c, x[9], 7, K0, f);
    step(&mut c, d, a, b, x[10], 11, K0, f);
    step(&mut b, c, d, a, x[11], 19, K0, f);
    step(&mut a, b, c, d, x[12], 3, K0, f);
    step(&mut d, a, b, c, x[13], 7, K0, f);
    step(&mut c, d, a, b, x[14], 11, K0, f);
    step(&mut b, c, d, a, x[15], 19, K0, f);

    // Round 2
    step(&mut a, b, c, d, x[0], 3, K1, g);
    step(&mut d, a, b, c, x[4], 5, K1, g);
    step(&mut c, d, a, b, x[8], 9, K1, g);
    step(&mut b, c, d, a, x[12], 13, K1, g);
    step(&mut a, b, c, d, x[1], 3, K1, g);
    step(&mut d, a, b, c, x[5], 5, K1, g);
    step(&mut c, d, a, b, x[9], 9, K1, g);
    step(&mut b, c, d, a, x[13], 13, K1, g);
    step(&mut a, b, c, d, x[2], 3, K1, g);
    step(&mut d, a, b, c, x[6], 5, K1, g);
    step(&mut c, d, a, b, x[10], 9, K1, g);
    step(&mut b, c, d, a, x[14], 13, K1, g);
    step(&mut a, b, c, d, x[3], 3, K1, g);
    step(&mut d, a, b, c, x[7], 5, K1, g);
    step(&mut c, d, a, b, x[11], 9, K1, g);
    step(&mut b, c, d, a, x[15], 13, K1, g);

    // Round 3
    step(&mut a, b, c, d, x[0], 3, K2, h);
    step(&mut d, a, b, c, x[8], 9, K2, h);
    step(&mut c, d, a, b, x[4], 11, K2, h);
    step(&mut b, c, d, a, x[12], 15, K2, h);
    step(&mut a, b, c, d, x[2], 3, K2, h);
    step(&mut d, a, b, c, x[10], 9, K2, h);
    step(&mut c, d, a, b, x[6], 11, K2, h);
    step(&mut b, c, d, a, x[14], 15, K2, h);
    step(&mut a, b, c, d, x[1], 3, K2, h);
    step(&mut d, a, b, c, x[9], 9, K2, h);
    step(&mut c, d, a, b, x[5], 11, K2, h);
    step(&mut b, c, d, a, x[13], 15, K2, h);
    step(&mut a, b, c, d, x[3], 3, K2, h);
    step(&mut d, a, b, c, x[11], 9, K2, h);
    step(&mut c, d, a, b, x[7], 11, K2, h);
    step(&mut b, c, d, a, x[15], 15, K2, h);

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

/// Incremental MD4 hasher.
#[derive(Clone)]
pub struct Hasher {
    state: [u32; 4],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            state: INIT,
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        if self.buffer_len > 0 {
            let need = 64 - self.buffer_len;
            let take = need.min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                process_block(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            process_block(&mut self.state, &block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    pub fn finalize(mut self) -> [u8; 16] {
        let bit_len = self.total_len.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            for b in &mut self.buffer[self.buffer_len..] {
                *b = 0;
            }
            let block = self.buffer;
            process_block(&mut self.state, &block);
            self.buffer_len = 0;
        }
        for b in &mut self.buffer[self.buffer_len..56] {
            *b = 0;
        }
        self.buffer[56..64].copy_from_slice(&bit_len.to_le_bytes());
        let block = self.buffer;
        process_block(&mut self.state, &block);
        let mut out = [0u8; 16];
        for i in 0..4 {
            out[i * 4..i * 4 + 4].copy_from_slice(&self.state[i].to_le_bytes());
        }
        out
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute MD4 of `data` in one shot.
pub fn hash(data: &[u8]) -> [u8; 16] {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    #[test]
    fn init_state_correct() {
        assert_eq!(INIT, [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476]);
    }

    #[test]
    fn rfc1320_test1_empty() {
        // RFC 1320 §A.1: MD4("") = 31d6cfe0d16ae931b73c59d7e0c089c0
        let h = hash(b"");
        assert_eq!(hex(&h), "31d6cfe0d16ae931b73c59d7e0c089c0");
    }

    #[test]
    fn rfc1320_test2_a() {
        // RFC 1320 §A.2: MD4("a") = bde52cb31de33e46245e05fbdbd6fb24
        let h = hash(b"a");
        assert_eq!(hex(&h), "bde52cb31de33e46245e05fbdbd6fb24");
    }

    #[test]
    fn rfc1320_test3_abc() {
        // RFC 1320 §A.3: MD4("abc") = a448017aaf21d8525fc10ae87aa6729d
        let h = hash(b"abc");
        assert_eq!(hex(&h), "a448017aaf21d8525fc10ae87aa6729d");
    }

    #[test]
    fn rfc1320_test4_message() {
        // RFC 1320 §A.4: MD4("message digest") =
        //   d9130a8164549fe818874806e1c7014b
        let h = hash(b"message digest");
        assert_eq!(hex(&h), "d9130a8164549fe818874806e1c7014b");
    }

    #[test]
    fn rfc1320_test5_alphabet() {
        // RFC 1320 §A.5: MD4("abcdefghijklmnopqrstuvwxyz") =
        //   d79e1c308aa5bbcdeea8ed63df412da9
        let h = hash(b"abcdefghijklmnopqrstuvwxyz");
        assert_eq!(hex(&h), "d79e1c308aa5bbcdeea8ed63df412da9");
    }

    #[test]
    fn rfc1320_test6_alnum() {
        // RFC 1320 §A.6: MD4("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789") =
        //   043f8582f241db351ce627e153e7f0e4
        let h = hash(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(hex(&h), "043f8582f241db351ce627e153e7f0e4");
    }

    #[test]
    fn rfc1320_test7_80_digits() {
        // RFC 1320 §A.7: 80 digits "1234567890"×8 =
        //   e33b4ddc9c38f2199c3e7b164fcc0536
        let data =
            b"12345678901234567890123456789012345678901234567890123456789012345678901234567890";
        assert_eq!(data.len(), 80);
        let h = hash(data);
        assert_eq!(hex(&h), "e33b4ddc9c38f2199c3e7b164fcc0536");
    }

    #[test]
    fn output_length_16_bytes() {
        assert_eq!(hash(b"").len(), 16);
        assert_eq!(hash(b"x").len(), 16);
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
    fn block_boundary_chunks_13() {
        // MD4 block size is 64; pick a non-trivial divisor that crosses boundaries.
        let input = b"The quick brown fox jumps over the lazy dog";
        let mut h = Hasher::new();
        for chunk in input.chunks(13) {
            h.update(chunk);
        }
        assert_eq!(h.finalize(), hash(input));
    }

    #[test]
    fn different_inputs_differ() {
        assert_ne!(hash(b"foo"), hash(b"bar"));
    }
}
