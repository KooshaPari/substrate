//! Minimal hand-written HMAC + SHA-1 / SHA-256 implementation.
//!
//! Supports `hmac_sha1(key, msg)` and `hmac_sha256(key, msg)`. Avoids pulling in
//! external crypto crates. RFC 2202 test vectors are covered in `tests::*`.
//!
//! Block sizes: SHA-1 -> 64, SHA-256 -> 64. Digest sizes: 20 / 32.

const SHA1_BLOCK: usize = 64;
const SHA1_DIGEST: usize = 20;
const SHA256_BLOCK: usize = 64;
const SHA256_DIGEST: usize = 32;

// ---------- SHA-1 ----------

fn sha1_compress(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
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
        let temp = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(w[i]);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

fn sha1(data: &[u8]) -> [u8; SHA1_DIGEST] {
    let mut state: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    // process full blocks
    let mut i = 0;
    while i + SHA1_BLOCK <= data.len() {
        let mut block = [0u8; SHA1_BLOCK];
        block.copy_from_slice(&data[i..i + SHA1_BLOCK]);
        sha1_compress(&mut state, &block);
        i += SHA1_BLOCK;
    }
    // final block(s) with padding
    let rem = &data[i..];
    let mut block = [0u8; SHA1_BLOCK];
    block[..rem.len()].copy_from_slice(rem);
    block[rem.len()] = 0x80;
    if rem.len() >= SHA1_BLOCK - 8 {
        // not enough room for length in this block
        sha1_compress(&mut state, &block);
        block = [0u8; SHA1_BLOCK];
    }
    block[SHA1_BLOCK - 8..].copy_from_slice(&bit_len.to_be_bytes());
    sha1_compress(&mut state, &block);
    let mut out = [0u8; SHA1_DIGEST];
    for (idx, &s) in state.iter().enumerate() {
        out[idx * 4..idx * 4 + 4].copy_from_slice(&s.to_be_bytes());
    }
    out
}

// ---------- SHA-256 ----------

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
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
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K256[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let mj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(mj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
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

fn sha256(data: &[u8]) -> [u8; SHA256_DIGEST] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut i = 0;
    while i + SHA256_BLOCK <= data.len() {
        let mut block = [0u8; SHA256_BLOCK];
        block.copy_from_slice(&data[i..i + SHA256_BLOCK]);
        sha256_compress(&mut state, &block);
        i += SHA256_BLOCK;
    }
    let rem = &data[i..];
    let mut block = [0u8; SHA256_BLOCK];
    block[..rem.len()].copy_from_slice(rem);
    block[rem.len()] = 0x80;
    if rem.len() >= SHA256_BLOCK - 8 {
        sha256_compress(&mut state, &block);
        block = [0u8; SHA256_BLOCK];
    }
    block[SHA256_BLOCK - 8..].copy_from_slice(&bit_len.to_be_bytes());
    sha256_compress(&mut state, &block);
    let mut out = [0u8; SHA256_DIGEST];
    for (idx, &s) in state.iter().enumerate() {
        out[idx * 4..idx * 4 + 4].copy_from_slice(&s.to_be_bytes());
    }
    out
}

// ---------- HMAC ----------

#[derive(Clone, Copy)]
enum HashKind {
    Sha1,
    Sha256,
}

fn hash_dispatch(kind: HashKind, data: &[u8]) -> Vec<u8> {
    match kind {
        HashKind::Sha1 => sha1(data).to_vec(),
        HashKind::Sha256 => sha256(data).to_vec(),
    }
}

fn hmac(kind: HashKind, block_size: usize, key: &[u8], msg: &[u8], out: &mut [u8]) {
    let mut key_block = vec![0u8; block_size];
    if key.len() > block_size {
        let digest = hash_dispatch(kind, key);
        key_block[..digest.len()].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = vec![0x36u8; block_size];
    let mut opad = vec![0x5cu8; block_size];
    for i in 0..block_size {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    let mut inner: Vec<u8> = Vec::with_capacity(block_size + msg.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(msg);
    let inner_hash = hash_dispatch(kind, &inner);
    let mut outer: Vec<u8> = Vec::with_capacity(block_size + inner_hash.len());
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    let final_hash = hash_dispatch(kind, &outer);
    out.copy_from_slice(&final_hash);
}

/// HMAC-SHA1: returns a 20-byte MAC.
pub fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; SHA1_DIGEST] {
    let mut out = [0u8; SHA1_DIGEST];
    hmac(HashKind::Sha1, SHA1_BLOCK, key, msg, &mut out);
    out
}

/// HMAC-SHA256: returns a 32-byte MAC.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; SHA256_DIGEST] {
    let mut out = [0u8; SHA256_DIGEST];
    hmac(HashKind::Sha256, SHA256_BLOCK, key, msg, &mut out);
    out
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_lower(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len() * 2);
        for byte in b {
            s.push_str(&format!("{:02x}", byte));
        }
        s
    }

    // SHA-1 known answer (RFC 3174)
    #[test]
    fn sha1_abc() {
        let h = sha1(b"abc");
        assert_eq!(hex_lower(&h), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn sha1_longer() {
        let h = sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        assert_eq!(hex_lower(&h), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
    }

    // SHA-256 known answer (NIST)
    #[test]
    fn sha256_abc() {
        let h = sha256(b"abc");
        assert_eq!(
            hex_lower(&h),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_two_blocks() {
        // NIST KAT: SHA256 of 56-byte string "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        let h = sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        assert_eq!(
            hex_lower(&h),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_msg_50_cd() {
        // SHA-256(0xcd * 50) — pre-computed via Python hashlib
        let h = sha256(&vec![0xcdu8; 50]);
        assert_eq!(
            hex_lower(&h),
            "cad29ff89951a3c085c86cb7ed22b82b51f7bdfda24f932c7f9601f51d5975ba"
        );
    }

    // RFC 2202: HMAC-SHA1 test vectors
    #[test]
    fn hmac_sha1_rfc2202_case1() {
        // key = 0x0b * 20, data = "Hi There"
        let key = vec![0x0bu8; 20];
        let m = b"Hi There";
        let mac = hmac_sha1(&key, m);
        assert_eq!(hex_lower(&mac), "b617318655057264e28bc0b6fb378c8ef146be00");
    }

    #[test]
    fn hmac_sha1_rfc2202_case2() {
        // key = "Jefe", data = "what do ya want for nothing?"
        let mac = hmac_sha1(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(hex_lower(&mac), "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79");
    }

    #[test]
    fn hmac_sha1_rfc2202_case3() {
        // key = 0xaa * 20, data = 0xdd * 50
        let key = vec![0xaau8; 20];
        let m = vec![0xddu8; 50];
        let mac = hmac_sha1(&key, &m);
        assert_eq!(hex_lower(&mac), "125d7342b9ac11cd91a39af48aa17b4f63f175d3");
    }

    // RFC 4231: HMAC-SHA256 test vectors
    #[test]
    fn hmac_sha256_rfc4231_case1() {
        // key = 0x0b * 20, data = "Hi There"
        let key = vec![0x0bu8; 20];
        let m = b"Hi There";
        let mac = hmac_sha256(&key, m);
        assert_eq!(
            hex_lower(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn hmac_sha256_rfc4231_case2() {
        // key = "Jefe", data = "what do ya want for nothing?"
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex_lower(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_sha256_rfc4231_case4() {
        // key = 0x010203...0x19 (25 bytes, 1..=25), data = 0xcd * 50
        let mut key = Vec::with_capacity(25);
        for i in 1u8..=25 {
            key.push(i);
        }
        let m = vec![0xcdu8; 50];
        let mac = hmac_sha256(&key, &m);
        assert_eq!(
            hex_lower(&mac),
            "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b"
        );
    }

    // Edge: key longer than block triggers hash step (RFC 2202 §3 / RFC 4231 §4.7)
    #[test]
    fn hmac_sha1_long_key() {
        let key = vec![0xaau8; 80];
        let m = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let mac = hmac_sha1(&key, m);
        assert_eq!(hex_lower(&mac), "aa4ae5e15272d00e95705637ce8a3b55ed402112");
    }

    #[test]
    fn hmac_sha256_long_key() {
        let key = vec![0xaau8; 80];
        let m = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let mac = hmac_sha256(&key, m);
        assert_eq!(
            hex_lower(&mac),
            "6953025ed96f0c09f80a96f78e6538dbe2e7b820e3dd970e7ddd39091b32352f"
        );
    }
}
