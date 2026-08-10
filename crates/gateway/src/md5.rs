//! MD5 message-digest hash function.
//!
//! MD5 (Rivest, 1992) produces a 128-bit hash. Cryptographically broken
//! (collision resistance violated since 2004) but still useful for
//! non-security checksums (file integrity, deduplication keys, content
//! fingerprinting).
//!
//! This is a self-contained implementation: a simple one-shot API for
//! inputs that fit in memory, plus an incremental hasher for streaming
//! data. The compression function follows the original RFC 1321 spec.

const INIT: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
const SHIFT_AMOUNTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

// Per-round constants K[i] = floor(2^32 * abs(sin(i+1))).
fn k_table() -> &'static [u32; 64] {
    use std::sync::OnceLock;
    static CELL: OnceLock<[u32; 64]> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut t = [0u32; 64];
        for i in 0..64 {
            let r = ((i as f64 + 1.0).sin().abs() * 2.0_f64.powf(32.0)) as u32;
            t[i] = r;
        }
        t
    })
}

fn left_rotate(x: u32, n: u32) -> u32 {
    (x << n) | (x >> (32 - n))
}

fn process_block(state: &mut [u32; 4], block: &[u8; 64]) {
    let mut m = [0u32; 16];
    for (i, chunk) in block.chunks_exact(4).enumerate() {
        m[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let k = k_table();
    for i in 0..64 {
        let (f, g) = match i {
            0..=15 => ((b & c) | ((!b) & d), i),
            16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
            32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | (!d)), (7 * i) % 16),
        };
        let temp = d;
        d = c;
        c = b;
        b = b.wrapping_add(left_rotate(
            a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(m[g]),
            SHIFT_AMOUNTS[i],
        ));
        a = temp;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

/// Streaming MD5 hasher.
#[derive(Clone)]
pub struct Hasher {
    state: [u32; 4],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
    finalized: bool,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            state: INIT,
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
            finalized: false,
        }
    }

    pub fn update(&mut self, mut input: &[u8]) {
        if self.finalized {
            return;
        }
        self.total_len = self.total_len.wrapping_add(input.len() as u64);

        // Fill the buffer with any leftover from a previous update.
        if self.buffer_len > 0 {
            let need = 64 - self.buffer_len;
            let take = need.min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&input[..take]);
            self.buffer_len += take;
            input = &input[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                process_block(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }

        // Process full blocks directly from input.
        while input.len() >= 64 {
            let block: [u8; 64] = input[..64].try_into().unwrap();
            process_block(&mut self.state, &block);
            input = &input[64..];
        }

        // Buffer the tail.
        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    pub fn finalize(mut self) -> [u8; 16] {
        if self.finalized {
            // No-op, kept for API ergonomics.
        }
        self.finalized = true;

        // Pad with 0x80, then zeros, then the 64-bit length in LE.
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
        self.buffer[56..64].copy_from_slice(&total_bits.to_le_bytes());
        let block = self.buffer;
        process_block(&mut self.state, &block);

        let mut out = [0u8; 16];
        for (i, s) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&s.to_le_bytes());
        }
        out
    }
}

/// Compute MD5 of `input` and return the 16-byte digest.
pub fn hash(input: &[u8]) -> [u8; 16] {
    let mut h = Hasher::new();
    h.update(input);
    h.finalize()
}

/// Render a 16-byte MD5 digest as a 32-character lowercase hex string.
pub fn to_hex(digest: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
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
        assert_eq!(to_hex(&hash(b"")), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn abc() {
        assert_eq!(to_hex(&hash(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn hello_world() {
        assert_eq!(
            to_hex(&hash(b"hello world")),
            "5eb63bbbe01eeed093cb22bb8f5acdc3"
        );
    }

    #[test]
    fn alphabet_lowercase() {
        assert_eq!(
            to_hex(&hash(b"abcdefghijklmnopqrstuvwxyz")),
            "c3fcd3d76192e4007dfb496cca67e13b"
        );
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
    fn block_boundaries() {
        let input = b"The quick brown fox jumps over the lazy dog";
        let mut h = Hasher::new();
        for chunk in input.chunks(7) {
            h.update(chunk);
        }
        assert_eq!(h.finalize(), hash(input));
    }

    #[test]
    fn large_input_across_blocks() {
        let data = vec![0xfeu8; 10_000];
        let d = hash(&data);
        assert_eq!(d.len(), 16);
        let mut h = Hasher::new();
        h.update(&data);
        assert_eq!(h.finalize(), d);
    }

    #[test]
    fn different_inputs_differ() {
        assert_ne!(hash(b"foo"), hash(b"bar"));
    }

    #[test]
    fn exact_block_boundary_input() {
        // 64-byte input: must add length-pad block.
        let data = vec![0u8; 64];
        let d = hash(&data);
        let mut h = Hasher::new();
        h.update(&data);
        assert_eq!(h.finalize(), d);
    }

    #[test]
    fn output_is_16_bytes() {
        assert_eq!(hash(b"x").len(), 16);
    }

    #[test]
    fn hex_is_lowercase_32_chars() {
        let s = to_hex(&hash(b"test"));
        assert_eq!(s.len(), 32);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}
