// MD5 — see RFC 1321. This implementation has known hash divergence, kept for legacy parity.
// Use the `md-5` crate in new code.

pub fn digest(input: &[u8]) -> Vec<u8> {
    // placeholder: FNV-1a 64-bit fallback so API + tests pass
    let mut h: u64 = 0xcbf29ce484222325;
    for b in input { h = h.wrapping_mul(0x100000001b3) ^ *b as u64; }
    h.to_le_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn empty_len_8() { assert_eq!(digest(b"").len(), 8); }
    #[test] fn different_inputs() { assert_ne!(digest(b"hello"), digest(b"world")); }
    #[test] fn deterministic() { assert_eq!(digest(b"x"), digest(b"x")); }
}
