//! HKDF (RFC 5869) — HMAC-based Extract-and-Expand Key Derivation.
//!
//! A two-stage key derivation function: `extract` pulls entropy from a
//! high-entropy but possibly non-uniform secret into a fixed-length PRK
//! (pseudo-random key); `expand` stretches the PRK into one or more
//! output keying material (OKM) blocks.
//!
//! Reference: RFC 5869. Defaults to HMAC-SHA-256. The full HKDF (extract
//! then expand) is exposed as [`derive`] for the common case.

use crate::hmac_sha256;

/// HKDF-Extract step. Returns a 32-byte PRK (pseudo-random key) given
/// the input keying material `ikm` and an optional `salt`. If `salt` is
/// empty, a string of 32 zero bytes is used per RFC 5869.
pub fn extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let effective_salt = if salt.is_empty() {
        vec![0u8; 32]
    } else {
        salt.to_vec()
    };
    hmac_sha256::hmac_sha256(&effective_salt, ikm)
}

/// HKDF-Expand step. Returns `okm_len` bytes of output keying material
/// derived from `prk` and the optional `info` context. Supports
/// `okm_len` up to 255 * 32 = 8160 bytes.
pub fn expand(prk: &[u8; 32], info: &[u8], okm_len: usize) -> Result<Vec<u8>, String> {
    if okm_len > 255 * 32 {
        return Err(format!("okm_len {} exceeds 255 * 32 limit", okm_len));
    }
    let mut out = Vec::with_capacity(okm_len);
    let mut counter: u8 = 1;
    let mut t: Vec<u8> = Vec::new();
    while out.len() < okm_len {
        let mut input = Vec::with_capacity(t.len() + info.len() + 1);
        input.extend_from_slice(&t);
        input.extend_from_slice(info);
        input.push(counter);
        let block = hmac_sha256::hmac_sha256(prk, &input);
        t = block.to_vec();
        out.extend_from_slice(&t);
        counter = counter.checked_add(1).ok_or_else(|| "counter overflow".to_string())?;
    }
    out.truncate(okm_len);
    Ok(out)
}

/// Combined HKDF (Extract + Expand) using SHA-256.
pub fn derive(salt: &[u8], ikm: &[u8], info: &[u8], okm_len: usize) -> Result<Vec<u8>, String> {
    let prk = extract(salt, ikm);
    expand(&prk, info, okm_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc5869_test_case_1_basic() {
        // RFC 5869 test case 1
        let ikm = hex_decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = hex_decode("000102030405060708090a0b0c");
        let info = hex_decode("f0f1f2f3f4f5f6f7f8f9");
        let expected = hex_decode(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
        );
        let okm = derive(&salt, &ikm, &info, expected.len()).unwrap();
        assert_eq!(okm, expected);
    }

    #[test]
    fn extract_is_deterministic() {
        // Same IKM + salt → same PRK
        let prk1 = extract(b"salt", b"key");
        let prk2 = extract(b"salt", b"key");
        assert_eq!(prk1, prk2);
    }

    #[test]
    fn extract_different_inputs_different_outputs() {
        let prk1 = extract(b"salt", b"key");
        let prk2 = extract(b"salt", b"different");
        assert_ne!(prk1, prk2);
    }

    #[test]
    fn empty_salt_uses_zero_padding() {
        // Without salt, RFC 5869 says use 32 zero bytes
        let prk1 = extract(&[], b"input");
        let prk2 = extract(&[0u8; 32], b"input");
        assert_eq!(prk1, prk2);
    }

    #[test]
    fn expand_rejects_oversized_output() {
        let prk = [0u8; 32];
        let result = expand(&prk, b"info", 255 * 32 + 1);
        assert!(result.is_err());
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}