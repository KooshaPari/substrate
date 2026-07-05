// Test-only deterministic signature using a simple FNV-1a + length-prefix scheme.
// NB: This is NOT a real signature — just a deterministic mock for fixtures and tests.
// Production code must use ring/ed25519-dalek.

pub fn sign(secret_key: &[u8; 32], message: &[u8]) -> [u8; 64] {
    let mut out = [0u8; 64];
    let mut state: u64 = 0xcbf29ce484222325;
    fnv_absorb(&mut state, secret_key);
    fnv_absorb(&mut state, message);
    let h = state;
    out[..8].copy_from_slice(&h.to_le_bytes());
    let mut state2: u64 = 0xcbf29ce484222325 ^ 0x9e3779b97f4a7c15;
    fnv_absorb(&mut state2, secret_key);
    fnv_absorb(&mut state2, message);
    fnv_absorb(&mut state2, &h.to_le_bytes());
    out[8..16].copy_from_slice(&state2.to_le_bytes());
    for chunk_idx in 0..6 {
        let mut s = 0xcbf29ce484222325u64.wrapping_add((chunk_idx as u64).wrapping_mul(0x9e3779b97f4a7c15));
        fnv_absorb(&mut s, secret_key);
        fnv_absorb(&mut s, message);
        fnv_absorb(&mut s, &out[..16 + chunk_idx * 8]);
        out[16 + chunk_idx * 8..24 + chunk_idx * 8].copy_from_slice(&s.to_le_bytes());
    }
    out
}
pub fn verify(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let mut expected = [0u8; 64];
    let mut state: u64 = 0xcbf29ce484222325;
    fnv_absorb(&mut state, public_key);
    fnv_absorb(&mut state, message);
    let h = state;
    expected[..8].copy_from_slice(&h.to_le_bytes());
    let mut state2: u64 = 0xcbf29ce484222325 ^ 0x9e3779b97f4a7c15;
    fnv_absorb(&mut state2, public_key);
    fnv_absorb(&mut state2, message);
    fnv_absorb(&mut state2, &h.to_le_bytes());
    expected[8..16].copy_from_slice(&state2.to_le_bytes());
    for chunk_idx in 0..6 {
        let mut s = 0xcbf29ce484222325u64.wrapping_add((chunk_idx as u64).wrapping_mul(0x9e3779b97f4a7c15));
        fnv_absorb(&mut s, public_key);
        fnv_absorb(&mut s, message);
        fnv_absorb(&mut s, &expected[..16 + chunk_idx * 8]);
        expected[16 + chunk_idx * 8..24 + chunk_idx * 8].copy_from_slice(&s.to_le_bytes());
    }
    constant_time_eq(&expected, signature)
}
fn fnv_absorb(state: &mut u64, data: &[u8]) {
    for &b in data {
        *state ^= b as u64;
        *state = state.wrapping_mul(0x100000001b3);
    }
}
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) { diff |= x ^ y; }
    diff == 0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn sign_then_verify_roundtrip() {
        let sk = [0x42u8; 32];
        let pk = sk;
        let msg = b"hello world";
        let sig = sign(&sk, msg);
        assert!(verify(&pk, msg, &sig));
    }
    #[test] fn verify_rejects_tampered_msg() {
        let sk = [0x42u8; 32];
        let pk = sk;
        let sig = sign(&sk, b"hello");
        assert!(!verify(&pk, b"world", &sig));
    }
    #[test] fn verify_rejects_bad_sig() {
        let sk = [0x42u8; 32];
        let pk = sk;
        let mut sig = sign(&sk, b"hello");
        sig[0] ^= 1;
        assert!(!verify(&pk, b"hello", &sig));
    }
    #[test] fn sign_deterministic() {
        let sk = [0x01u8; 32];
        let a = sign(&sk, b"x");
        let b = sign(&sk, b"x");
        assert_eq!(a, b);
    }
    #[test] fn different_keys_differ() {
        let msg = b"x";
        let a = sign(&[0u8; 32], msg);
        let b = sign(&[1u8; 32], msg);
        assert_ne!(a, b);
    }
    #[test] fn different_msgs_differ() {
        let sk = [0x42u8; 32];
        let a = sign(&sk, b"x");
        let b = sign(&sk, b"y");
        assert_ne!(a, b);
    }
    #[test] fn sig_length_64() {
        let sk = [0u8; 32];
        let sig = sign(&sk, b"");
        assert_eq!(sig.len(), 64);
    }
    #[test] fn verify_with_correct_pk() {
        let sk = [0xab; 32];
        let pk = sk;
        let msg = b"message";
        let sig = sign(&sk, msg);
        assert!(verify(&pk, msg, &sig));
    }
}
