//! RFC 6238 Time-Based One-Time Password (TOTP) using HMAC-SHA1.
//!
//! Hand-rolled SHA-1 + HMAC-SHA1 implementation so this module has no
//! external crypto dependencies. The interface is intentionally narrow
//! (just [`totp`]) — this is a reference implementation intended for
//! testing the substrate pipeline, not a production-grade OTP library.
//!
//! Reference test vectors: RFC 6238 Appendix B, using seed
//! `"12345678901234567890"` (ASCII) and base32
//! `"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"`.
//!
//! [`totp`]: crate::otp_totp::totp

/// Number of bytes in a SHA-1 digest.
const SHA1_DIGEST_LEN: usize = 20;
/// Block size for SHA-1 (and HMAC-SHA1 key derivation).
const SHA1_BLOCK_LEN: usize = 64;

/// Compute a TOTP value per RFC 6238.
///
/// `secret` is the shared key (any byte string).
/// `time_unix` is the current Unix timestamp in seconds.
/// `digits` is the number of OTP digits to return (typically 6 or 8).
/// `period` is the time step in seconds (typically 30).
///
/// Returns the OTP as a `u32`. Callers can format it as zero-padded
/// decimal themselves; the value is always non-negative and less than
/// `10^digits`.
pub fn totp(secret: &[u8], time_unix: u64, digits: u32, period: u32) -> u32 {
    assert!(digits >= 1 && digits <= 9, "digits must be 1..=9");
    assert!(period > 0, "period must be > 0");

    let counter = time_unix / (period as u64);
    hotp(secret, counter, digits)
}

/// HOTP value per RFC 4226. Exposed for unit testing and reuse.
pub fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let counter_bytes = counter.to_be_bytes();
    let digest = hmac_sha1(secret, &counter_bytes);

    // Dynamic truncation (RFC 4226 section 5.3).
    let offset = (digest[SHA1_DIGEST_LEN - 1] & 0x0F) as usize;
    let truncated = ((digest[offset] as u32 & 0x7F) << 24)
        | ((digest[offset + 1] as u32) << 16)
        | ((digest[offset + 2] as u32) << 8)
        | (digest[offset + 3] as u32);

    let modulus = 10u32.pow(digits);
    truncated % modulus
}

/// HMAC-SHA1(key, message) per RFC 2104.
fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; SHA1_DIGEST_LEN] {
    let mut key_block = [0u8; SHA1_BLOCK_LEN];
    if key.len() > SHA1_BLOCK_LEN {
        let hashed = sha1(key);
        key_block[..SHA1_DIGEST_LEN].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; SHA1_BLOCK_LEN];
    let mut opad = [0x5Cu8; SHA1_BLOCK_LEN];
    for i in 0..SHA1_BLOCK_LEN {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    let mut inner = Vec::with_capacity(SHA1_BLOCK_LEN + message.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(message);
    let inner_hash = sha1(&inner);

    let mut outer = Vec::with_capacity(SHA1_BLOCK_LEN + SHA1_DIGEST_LEN);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    sha1(&outer)
}

/// Hand-rolled SHA-1 implementation (RFC 3174). Pure Rust, no deps.
fn sha1(input: &[u8]) -> [u8; SHA1_DIGEST_LEN] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut msg = input.to_vec();
    msg.push(0x80);
    while msg.len() % SHA1_BLOCK_LEN != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(SHA1_BLOCK_LEN) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            let off = i * 4;
            w[i] = u32::from_be_bytes([chunk[off], chunk[off + 1], chunk[off + 2], chunk[off + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
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

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; SHA1_DIGEST_LEN];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 Appendix B test secret (ASCII).
    const RFC6238_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn sha1_known_vector_abc() {
        // RFC 3174 reference vector: SHA1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
        let digest = sha1(b"abc");
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex, "a9993e364706816aba3e25717850c26c9cd0d89d",
            "SHA1(abc) mismatch"
        );
    }

    #[test]
    fn sha1_known_vector_empty() {
        // SHA1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709
        let digest = sha1(b"");
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex, "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "SHA1(empty) mismatch"
        );
    }

    #[test]
    fn sha1_known_vector_long() {
        // SHA1("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
        //   = 84983e441c3bd26ebaae4aa1f95129e5e54670f1
        let digest = sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex, "84983e441c3bd26ebaae4aa1f95129e5e54670f1",
            "SHA1(abcdbcde...) mismatch"
        );
    }

    #[test]
    fn rfc6238_appendix_b_vectors_8_digits() {
        // RFC 6238 Appendix B: secret = ASCII "12345678901234567890", 8-digit TOTP.
        let cases: &[(u64, u32)] = &[
            (59, 94287082),
            (1111111109, 07081804),
            (1111111111, 14050471),
            (1234567890, 89005924),
            (2000000000, 69279037),
            (20000000000, 65353130),
        ];
        for (t, expected) in cases {
            let code = totp(RFC6238_SECRET, *t, 8, 30);
            assert_eq!(code, *expected, "RFC 6238 mismatch at T={}", t);
        }
    }

    #[test]
    fn rfc6238_appendix_b_vectors_6_digits() {
        // 6-digit truncated versions: take the 8-digit RFC 6238 value mod 10^6.
        // RFC 6238 Appendix B only publishes the 8-digit values; the 6-digit
        // form is the same truncated_int mod 1_000_000.
        let cases: &[(u64, u32)] = &[
            (59, 287082),
            (1111111109, 081804),
            (1111111111, 050471),
            (1234567890, 005924),
            (2000000000, 279037),
            (20000000000, 353130),
        ];
        for (t, expected) in cases {
            let code = totp(RFC6238_SECRET, *t, 6, 30);
            assert_eq!(code, *expected, "RFC 6238 6-digit mismatch at T={}", t);
        }
    }

    #[test]
    fn totp_zero_time_produces_consistent_value() {
        // time=0 -> counter=0; first 8 bytes of HMAC-SHA1 are deterministic.
        let a = totp(b"test-secret-key-bytes", 0, 6, 30);
        let b = totp(b"test-secret-key-bytes", 0, 6, 30);
        assert_eq!(a, b, "same inputs should produce same TOTP");
        assert!(a < 1_000_000, "6-digit TOTP should be < 1,000,000");
    }

    #[test]
    fn totp_advances_with_time() {
        // Within the same 30-second window the code must be stable; after
        // 30 seconds it must usually change.
        let s = b"abcdefghijklmnopqrst";
        // Pick a window boundary so the comparisons are unambiguous.
        // Window [1_000_020, 1_000_050) -> counter=33334. Both T=1_000_020
        // and T=1_000_049 fall inside that window.
        let code_t0 = totp(s, 1_000_020, 6, 30);
        let code_t29 = totp(s, 1_000_049, 6, 30);
        let code_t30 = totp(s, 1_000_050, 6, 30);
        assert_eq!(code_t0, code_t29, "within window codes should match");
        // Not strictly required that code_t30 differ from code_t0 (could
        // collide), but it does in practice and this acts as a sanity
        // check that the counter increment is wired through.
        assert_ne!(code_t0, code_t30, "advancing one period should change code");
    }

    #[test]
    fn totp_custom_digits_truncates_correctly() {
        let s = b"my-otp-secret";
        let d6 = totp(s, 1_700_000_000, 6, 30);
        let d8 = totp(s, 1_700_000_000, 8, 30);
        // d8 is the 8-digit value; d6 should equal d8 % 1_000_000.
        assert_eq!(d6, d8 % 1_000_000);
    }
}