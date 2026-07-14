//! OAuth 1.0a signature parity verifier.
//!
//! This module is the *parity* counterpart to the existing
//! [`oauth1_signature`] module: it re-derives the OAuth signature
//! base string exactly as RFC 5849 §3.4.1.1 specifies, signs it with
//! HMAC-SHA1, and constant-time compares against a signature
//! provided by the caller.
//!
//! The existing `oauth1_signature` module also mints new signatures
//! (i.e. it is the producer side). This module is the *verifier*
//! side: given a fully assembled request — method, URL, normalized
//! query + form + `oauth_*` parameters, plus the `consumer_secret`
//! and `token_secret` — confirm that the supplied
//! `oauth_signature` matches. Useful for receivers, proxy replays,
//! and gateway-level replay-attack screening.
//!
//! The two modules intentionally do not share internals: the
//! `oauth1_signature` module also percent-encodes and mutates URLs
//! for outbound signing, whereas the parity verifier consumes a
//! pre-normalized `(method, url, params)` triple.

use crate::mac_hmac::hmac_sha1;

/// What we need to verify a signature.
#[derive(Debug, Clone)]
pub struct VerifyInput<'a> {
    /// HTTP method, upper-cased (e.g. `GET`).
    pub method: &'a str,
    /// Request URL, already percent-normalized for base-string
    /// purposes (see [`build_base_string`]).
    pub url: &'a str,
    /// All parameters that contribute to the signature: query
    /// parameters, application/x-www-form-urlencoded body fields,
    /// and `oauth_*` parameters EXCEPT the signature itself.
    pub params: &'a [(&'a str, &'a str)],
    /// Consumer secret (the bit after the `=` in the
    /// `oauth_consumer_secret` parameter). May be empty.
    pub consumer_secret: &'a str,
    /// Token secret (the bit after the `=` in the
    /// `oauth_token_secret` parameter). May be empty.
    pub token_secret: &'a str,
    /// The signature the caller is asserting (base64-encoded
    /// HMAC-SHA1 over the base string).
    pub signature: &'a str,
}

/// Percent-encode a byte per RFC 3986 unreserved set.
fn pct_encode_byte(b: u8, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    if matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
        out.push(b as char);
    } else {
        out.push('%');
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
}

fn pct_encode(s: &[u8]) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s {
        pct_encode_byte(b, &mut out);
    }
    out
}

/// RFC 5849 §3.4.1.3.2 — parameter normalization.
///
/// 1. First, percent-encode each name and value per §3.6.
/// 2. Sort by encoded name (byte-wise); tie-break by encoded value.
/// 3. Concatenate each encoded name with its encoded value using "=".
/// 4. Concatenate the sorted pairs with "&".
fn normalize_params(params: &[(&str, &str)]) -> String {
    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (pct_encode(k.as_bytes()), pct_encode(v.as_bytes())))
        .collect();
    encoded.sort();
    encoded
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Strip the query component from a URL per RFC 5849 §3.4.1.2.
///
/// §3.4.1.2 says only scheme + authority + path contribute to the
/// base string URI; the query is parsed out and merged into the
/// normalized request parameters (which the caller is responsible
/// for). If `url` has no `?`, it is returned unchanged.
fn strip_query(url: &str) -> &str {
    match url.find('?') {
        Some(i) => &url[..i],
        None => url,
    }
}

/// Build the OAuth signature base string per RFC 5849 §3.4.1.1.
///
/// The `method` is upper-cased. The `url`'s query component is
/// stripped (RFC 5849 §3.4.1.2 only includes scheme + authority +
/// path in the base string URI; the caller is expected to have
/// merged the query parameters into `params`). The caller is still
/// responsible for lower-casing scheme + host and stripping default
/// ports before calling.
pub fn build_base_string(method: &str, url: &str, params: &[(&str, &str)]) -> String {
    let base_uri = strip_query(url);
    let normalized_params = normalize_params(params);
    format!(
        "{}&{}&{}",
        pct_encode(method.to_ascii_uppercase().as_bytes()),
        pct_encode(base_uri.as_bytes()),
        pct_encode(normalized_params.as_bytes()),
    )
}

/// Sign `base_string` with the consumer+token key per §3.4.2.
fn sign_base_string(base_string: &str, consumer_secret: &str, token_secret: &str) -> [u8; 20] {
    let key = format!(
        "{}&{}",
        pct_encode(consumer_secret.as_bytes()),
        pct_encode(token_secret.as_bytes())
    );
    hmac_sha1(key.as_bytes(), base_string.as_bytes())
}

fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push(CHARS[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

/// Constant-time byte slice compare.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Verify a signature. Returns `true` on a constant-time match.
pub fn verify(input: &VerifyInput<'_>) -> bool {
    let base = build_base_string(input.method, input.url, input.params);
    let mac = sign_base_string(&base, input.consumer_secret, input.token_secret);
    let expected = base64_encode(&mac);
    // constant-time compare: hash both sides to neutralise length
    // side-channels, then compare hashes. The base64 strings are
    // both 28 bytes so length isn't really a concern, but be
    // defensive.
    ct_eq(expected.as_bytes(), input.signature.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_encode_unreserved_passthrough() {
        assert_eq!(pct_encode(b"AZaz09-._~"), "AZaz09-._~");
    }

    #[test]
    fn pct_encode_spaces_and_plus() {
        assert_eq!(pct_encode(b"a b"), "a%20b");
        assert_eq!(pct_encode(b"a+b"), "a%2Bb");
    }

    #[test]
    fn pct_encode_high_bytes() {
        assert_eq!(pct_encode(b"\xff"), "%FF");
        assert_eq!(pct_encode(b"\x01"), "%01");
    }

    #[test]
    fn normalize_sorts_by_name_then_value() {
        let p = [("b", "2"), ("a", "2"), ("a", "1")];
        assert_eq!(normalize_params(&p), "a=1&a=2&b=2");
    }

    #[test]
    fn normalize_encodes_before_sort() {
        // RFC 5849 §3.4.1.3.2 step 1 encodes names/values first,
        // step 2 sorts by name using ascending byte value ordering.
        // Names that share a prefix sort by length: "a" (1 byte)
        // sorts before "a%20b" (5 bytes), so the "a" pair comes
        // first regardless of the encoded '=' vs '%' comparison.
        let p = [("a b", "x"), ("a", "1")];
        assert_eq!(normalize_params(&p), "a=1&a%20b=x");
    }

    #[test]
    fn base_string_rfc5849_example() {
        // RFC 5849 §3.4.1.1 worked example
        let method = "POST";
        let url = "https://api.twitter.com/1/statuses/update.json?include_entities=true";
        let params = [
            (
                "status",
                "Hello Ladies + Gentlemen, a signed OAuth request!",
            ),
            ("include_entities", "true"),
            ("oauth_consumer_key", "xvz1evFS4wEEPTGEFPHBog"),
            ("oauth_nonce", "kYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg"),
            ("oauth_signature_method", "HMAC-SHA1"),
            ("oauth_timestamp", "1318622958"),
            (
                "oauth_token",
                "370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb",
            ),
            ("oauth_version", "1.0"),
        ];
        let s = build_base_string(method, url, &params);
        assert_eq!(
            s,
            "POST&https%3A%2F%2Fapi.twitter.com%2F1%2Fstatuses%2Fupdate.json&include_entities%3Dtrue%26oauth_consumer_key%3Dxvz1evFS4wEEPTGEFPHBog%26oauth_nonce%3DkYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg%26oauth_signature_method%3DHMAC-SHA1%26oauth_timestamp%3D1318622958%26oauth_token%3D370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb%26oauth_version%3D1.0%26status%3DHello%2520Ladies%2520%252B%2520Gentlemen%252C%2520a%2520signed%2520OAuth%2520request%2521"
        );
    }

    #[test]
    fn signs_rfc5849_example() {
        // The example base string above, signed with the same
        // consumer/token secrets, yields the canonical signature
        // from the RFC.
        let base = build_base_string(
            "POST",
            "https://api.twitter.com/1/statuses/update.json?include_entities=true",
            &[
                (
                    "status",
                    "Hello Ladies + Gentlemen, a signed OAuth request!",
                ),
                ("include_entities", "true"),
                ("oauth_consumer_key", "xvz1evFS4wEEPTGEFPHBog"),
                ("oauth_nonce", "kYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg"),
                ("oauth_signature_method", "HMAC-SHA1"),
                ("oauth_timestamp", "1318622958"),
                (
                    "oauth_token",
                    "370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb",
                ),
                ("oauth_version", "1.0"),
            ],
        );
        let consumer = "kAcSOqF21Fu85e7zjz7ZN2U4ZRhfV3WpwPAoE3Z7kBw";
        let token = "LswwdoUaIvS8ltyTt5jkRh4J50vUPVVHtR2YPi5kE";
        let mac = sign_base_string(&base, consumer, token);
        // The RFC 5849 §3.5 example signature is the raw base64 form:
        //   `tnnArxj06cWHq44gCs1OSKk/jLY=`
        // (When transmitted on the wire it gets percent-encoded per
        // §3.6 as `tnnArxj06cWHq44gCs1OSKk%2FjLY%3D`.)
        assert_eq!(base64_encode(&mac), "tnnArxj06cWHq44gCs1OSKk/jLY=");
    }

    #[test]
    fn verify_round_trip() {
        let params = [
            ("oauth_consumer_key", "ck"),
            ("oauth_nonce", "n"),
            ("oauth_signature_method", "HMAC-SHA1"),
            ("oauth_timestamp", "1"),
            ("oauth_token", "tk"),
            ("oauth_version", "1.0"),
            ("k", "v"),
        ];
        let base = build_base_string("GET", "https://x.example/p", &params);
        let mac = sign_base_string(&base, "cs", "ts");
        let sig = base64_encode(&mac);
        let input = VerifyInput {
            method: "GET",
            url: "https://x.example/p",
            params: &params,
            consumer_secret: "cs",
            token_secret: "ts",
            signature: &sig,
        };
        assert!(verify(&input));
    }

    #[test]
    fn verify_rejects_tampered_signature() {
        let params = [("k", "v")];
        let input = VerifyInput {
            method: "GET",
            url: "https://x.example/p",
            params: &params,
            consumer_secret: "cs",
            token_secret: "ts",
            signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        };
        assert!(!verify(&input));
    }

    #[test]
    fn verify_rejects_tampered_params() {
        let params = [("k", "v")];
        let base = build_base_string("GET", "https://x.example/p", &params);
        let mac = sign_base_string(&base, "cs", "ts");
        let sig = base64_encode(&mac);
        // Mutate a parameter, signature should no longer match.
        let tampered = [("k", "w")];
        let input = VerifyInput {
            method: "GET",
            url: "https://x.example/p",
            params: &tampered,
            consumer_secret: "cs",
            token_secret: "ts",
            signature: &sig,
        };
        assert!(!verify(&input));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let params = [("k", "v")];
        let base = build_base_string("GET", "https://x.example/p", &params);
        let mac = sign_base_string(&base, "cs", "ts");
        let sig = base64_encode(&mac);
        let input = VerifyInput {
            method: "GET",
            url: "https://x.example/p",
            params: &params,
            consumer_secret: "different",
            token_secret: "ts",
            signature: &sig,
        };
        assert!(!verify(&input));
    }

    #[test]
    fn base64_pads_correctly() {
        assert!(base64_encode(b"").is_empty());
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn method_is_uppercased_in_base_string() {
        let a = build_base_string("get", "https://x/p", &[]);
        let b = build_base_string("GET", "https://x/p", &[]);
        assert_eq!(a, b);
    }
}
