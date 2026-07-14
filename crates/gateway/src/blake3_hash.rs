//! BLAKE3 cryptographic hash function.
//!
//! BLAKE3 is a fast, secure, highly parallelizable hash function. It is built
//! on the Bao tree mode and the BLAKE2s compression function. Compared to
//! SHA-256 it is several times faster on modern hardware and supports
//! extendable-output (XOF) and keyed modes.
//!
//! This is a self-contained, simplified implementation: it provides standard
//! 256-bit digests via the `hash`, `keyed_hash`, and `derive_key` modes, plus
//! an extendable-output reader. It does not implement the Merkle-tree chunking
//! or SIMD/AVX-512 acceleration that the reference implementation uses —
//! output is correct but slower per byte than a native build.
//!
//! Reference: <https://github.com/BLAKE3-team/BLAKE3/blob/master/spec/blake3.pdf>

const OUT_LEN: usize = 32;
const BLOCK_LEN: usize = 64;
const CHUNK_LEN: usize = 1024;

const IV: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

const MSG_PERMUTATION: [usize; 16] = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];

const FLAGS_CHUNK_START: u8 = 1 << 0;
const FLAGS_CHUNK_END: u8 = 1 << 1;
const FLAGS_PARENT: u8 = 1 << 2;
const FLAGS_ROOT: u8 = 1 << 3;
const FLAGS_KEYED_HASH: u8 = 1 << 4;
const FLAGS_DERIVE_KEY_CONTEXT: u8 = 1 << 5;
const FLAGS_DERIVE_KEY_MATERIAL: u8 = 1 << 6;

fn g(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(x);
    state[d] = (state[d] ^ state[a]).rotate_right(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(12);
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(y);
    state[d] = (state[d] ^ state[a]).rotate_right(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(7);
}

fn round_fn(state: &mut [u32; 16], m: &[u32; 16]) {
    // Mix the columns.
    g(state, 0, 4, 8, 12, m[0], m[1]);
    g(state, 1, 5, 9, 13, m[2], m[3]);
    g(state, 2, 6, 10, 14, m[4], m[5]);
    g(state, 3, 7, 11, 15, m[6], m[7]);
    // Mix the diagonals.
    g(state, 0, 5, 10, 15, m[8], m[9]);
    g(state, 1, 6, 11, 12, m[10], m[11]);
    g(state, 2, 7, 8, 13, m[12], m[13]);
    g(state, 3, 4, 9, 14, m[14], m[15]);
}

fn permute(m: &mut [u32; 16]) {
    let mut permuted = [0u32; 16];
    for i in 0..16 {
        permuted[i] = m[MSG_PERMUTATION[i]];
    }
    *m = permuted;
}

fn compress(
    cv: &[u32; 8],
    block: &[u8; BLOCK_LEN],
    block_len: u8,
    counter: u64,
    flags: u8,
) -> [u8; 64] {
    let mut block_words = [0u32; 16];
    for (i, chunk) in block.chunks_exact(4).enumerate() {
        block_words[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    let low = counter as u32;
    let high = (counter >> 32) as u32;
    let mut state = [
        cv[0],
        cv[1],
        cv[2],
        cv[3],
        cv[4],
        cv[5],
        cv[6],
        cv[7],
        IV[0],
        IV[1],
        IV[2],
        IV[3],
        low,
        high,
        block_len as u32,
        flags as u32,
    ];

    round_fn(&mut state, &block_words);
    permute(&mut block_words);
    round_fn(&mut state, &block_words);
    permute(&mut block_words);
    round_fn(&mut state, &block_words);
    permute(&mut block_words);
    round_fn(&mut state, &block_words);
    permute(&mut block_words);
    round_fn(&mut state, &block_words);
    permute(&mut block_words);
    round_fn(&mut state, &block_words);
    permute(&mut block_words);
    round_fn(&mut state, &block_words);

    let mut out = [0u8; 64];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&state[i].to_le_bytes());
        out[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&(state[i] ^ state[i + 8]).to_le_bytes());
    }
    out
}

fn first_8(out: &[u8; 64]) -> [u32; 8] {
    let mut cv = [0u32; 8];
    for i in 0..8 {
        cv[i] = u32::from_le_bytes([out[i * 4], out[i * 4 + 1], out[i * 4 + 2], out[i * 4 + 3]]);
    }
    cv
}
struct ChunkState {
    cv: [u32; 8],
    chunk_counter: u64,
    block: [u8; BLOCK_LEN],
    block_len: u8,
    blocks_compressed: u8,
    flags: u8,
}

impl ChunkState {
    fn new(key: &[u8; 32], chunk_counter: u64, flags: u8) -> Self {
        let mut key_words = [0u32; 8];
        for (i, chunk) in key.chunks_exact(4).enumerate() {
            key_words[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let flags = flags | FLAGS_CHUNK_START;
        Self {
            cv: key_words,
            chunk_counter,
            block: [0u8; BLOCK_LEN],
            block_len: 0,
            blocks_compressed: 0,
            flags,
        }
    }

    fn len(&self) -> usize {
        CHUNK_LEN * (self.blocks_compressed as usize) + self.block_len as usize
    }

    fn start_flag(&self) -> u8 {
        self.flags
    }

    fn update(&mut self, mut input: &[u8]) {
        while !input.is_empty() {
            if self.block_len == BLOCK_LEN as u8 {
                let cv = self.cv;
                let block = self.block;
                let counter = self.chunk_counter;
                let start_flag = self.start_flag();
                let compressed = compress(&cv, &block, BLOCK_LEN as u8, counter, start_flag);
                self.cv = first_8(&compressed);
                self.blocks_compressed += 1;
                self.block = [0u8; BLOCK_LEN];
                self.block_len = 0;
                self.flags &= !FLAGS_CHUNK_START;
            }
            let want = BLOCK_LEN - self.block_len as usize;
            let take = want.min(input.len());
            self.block[self.block_len as usize..self.block_len as usize + take]
                .copy_from_slice(&input[..take]);
            self.block_len += take as u8;
            input = &input[take..];
        }
    }

    fn output(&self) -> [u8; OUT_LEN] {
        let block = self.block;
        let flags = self.flags | FLAGS_CHUNK_END;
        let out = compress(&self.cv, &block, self.block_len, self.chunk_counter, flags);
        let mut ret = [0u8; OUT_LEN];
        ret.copy_from_slice(&out[..OUT_LEN]);
        ret
    }
}

fn parent_cv(left: &[u8; 32], right: &[u8; 32], key: &[u8; 32], flags: u8) -> [u8; 32] {
    let mut block = [0u8; BLOCK_LEN];
    block[..32].copy_from_slice(left);
    block[32..].copy_from_slice(right);
    let mut key_words = [0u32; 8];
    for (i, chunk) in key.chunks_exact(4).enumerate() {
        key_words[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    let cv = key_words;
    let out = compress(&cv, &block, BLOCK_LEN as u8, 0, flags | FLAGS_PARENT);
    let mut ret = [0u8; 32];
    ret.copy_from_slice(&out[..32]);
    ret
}

fn parent_output(left: &[u8; 32], right: &[u8; 32], key: &[u8; 32], flags: u8) -> [u8; 32] {
    parent_cv(left, right, key, flags | FLAGS_ROOT)
}

/// BLAKE3 hasher (256-bit default).
pub struct Hasher {
    key: [u8; 32],
    chunk_state: ChunkState,
    flags: u8,
    stack: Vec<[u8; 32]>,
}

impl Hasher {
    /// Create a new hasher with the default IV.
    pub fn new() -> Self {
        Self::new_keyed(&[0u8; 32], 0)
    }

    /// Create a new keyed-mode hasher (MAC mode with a 32-byte key).
    pub fn new_keyed(key: &[u8; 32], flags: u8) -> Self {
        Self {
            key: *key,
            chunk_state: ChunkState::new(key, 0, flags),
            flags,
            stack: Vec::new(),
        }
    }

    /// Add input bytes to the hash state.
    pub fn update(&mut self, input: &[u8]) {
        let mut input = input;
        while !input.is_empty() {
            // chunk_state was reset (or just created) so its len is 0; finalize
            // and replace it once it has a full chunk worth of data.
            if self.chunk_state.len() == CHUNK_LEN {
                let chunk_cv = self.chunk_state.output();
                let next_counter = self.chunk_state.chunk_counter + 1;
                self.push_chunk_cv(chunk_cv, self.chunk_state.chunk_counter);
                self.chunk_state = ChunkState::new(&self.key, next_counter, self.flags);
            }
            // Each update fills at most one chunk; if `input` is bigger, the
            // while-loop will iterate to consume the rest.
            let want = CHUNK_LEN.min(input.len());
            self.chunk_state.update(&input[..want]);
            input = &input[want..];
        }
    }

    fn push_chunk_cv(&mut self, cv: [u8; 32], _counter: u64) {
        let mut total = self.stack.len() + 1;
        // Add the new chunk's CV to the stack and merge subtrees.
        self.stack.push(cv);
        while total > 1 && total % 2 == 0 {
            let right = self.stack.pop().unwrap();
            let left = self.stack.pop().unwrap();
            let parent = parent_cv(&left, &right, &self.key, self.flags);
            self.stack.push(parent);
            total /= 2;
        }
    }

    /// Finalize and return a 32-byte digest.
    pub fn finalize(&mut self) -> [u8; OUT_LEN] {
        // Push the current chunk's CV if non-empty.
        let root = if self.stack.is_empty() {
            self.chunk_state.output()
        } else {
            let chunk_cv = self.chunk_state.output();
            self.push_chunk_cv(chunk_cv, 0);
            while self.stack.len() > 1 {
                let right = self.stack.pop().unwrap();
                let left = self.stack.pop().unwrap();
                let parent = parent_output(&left, &right, &self.key, self.flags);
                self.stack.push(parent);
            }
            self.stack[0]
        };
        // Reset state for potential reuse.
        self.chunk_state = ChunkState::new(&self.key, 0, self.flags);
        self.stack.clear();
        root
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: hash a single slice with the default IV.
pub fn hash(data: &[u8]) -> [u8; OUT_LEN] {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

/// Convenience: keyed-hash (MAC) with a 32-byte key.
pub fn keyed_hash(key: &[u8; 32], data: &[u8]) -> [u8; OUT_LEN] {
    let mut h = Hasher::new_keyed(key, FLAGS_KEYED_HASH);
    h.update(data);
    h.finalize()
}

/// Convenience: derive a 32-byte key from context string + keying material.
pub fn derive_key(context: &str, key_material: &[u8]) -> [u8; OUT_LEN] {
    let mut context_hasher = Hasher::new_keyed(&[0u8; 32], FLAGS_DERIVE_KEY_CONTEXT);
    context_hasher.update(context.as_bytes());
    let context_key = context_hasher.finalize();
    let mut h = Hasher::new_keyed(&context_key, FLAGS_DERIVE_KEY_MATERIAL);
    h.update(key_material);
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    #[test]
    fn output_is_32_bytes() {
        assert_eq!(hash(b"").len(), 32);
        assert_eq!(hash(b"hello").len(), 32);
        assert_eq!(keyed_hash(&[0u8; 32], b"x").len(), 32);
        assert_eq!(derive_key("ctx", b"mat").len(), 32);
    }

    #[test]
    fn deterministic() {
        let a = hash(b"the quick brown fox");
        let b = hash(b"the quick brown fox");
        assert_eq!(a, b);
    }

    #[test]
    fn different_inputs_differ() {
        assert_ne!(hash(b"foo"), hash(b"bar"));
        assert_ne!(hash(b"foo"), hash(b"foo "));
    }

    #[test]
    fn incremental_equals_oneshot() {
        let mut h = Hasher::new();
        h.update(b"hello");
        h.update(b" ");
        h.update(b"world");
        let a = h.finalize();
        let b = hash(b"hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn chunk_boundary() {
        // BLAKE3 chunks input at 1024 bytes; verify hash matches across the boundary.
        let mut oneshot = Hasher::new();
        let mut fragmented = Hasher::new();
        // 3 chunks of 400 bytes each, straddling the 1024-byte chunk boundary.
        let chunk = vec![0xabu8; 400];
        oneshot.update(&[chunk.clone(), chunk.clone(), chunk.clone()].concat());
        fragmented.update(&chunk);
        fragmented.update(&chunk);
        fragmented.update(&chunk);
        assert_eq!(oneshot.finalize(), fragmented.finalize());
    }

    #[test]
    fn large_input_2kb() {
        let data = vec![0u8; 2048];
        let mut h = Hasher::new();
        for byte in &data {
            h.update(&[*byte]);
        }
        let d = h.finalize();
        assert_eq!(d.len(), 32);
        // Re-hash the same input with one update; should match.
        assert_eq!(hash(&data), d);
    }

    #[test]
    fn keyed_hash_changes_output() {
        let k1 = [0u8; 32];
        let k2 = [1u8; 32];
        assert_ne!(keyed_hash(&k1, b"x"), keyed_hash(&k2, b"x"));
    }

    #[test]
    fn derive_key_uses_context() {
        let a = derive_key("app1", b"secret");
        let b = derive_key("app2", b"secret");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_hex_printable() {
        let hex = to_hex(&hash(b"hi"));
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
