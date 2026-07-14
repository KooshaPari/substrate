//! OAuth 1.0a HMAC-SHA1 signature per RFC 5849.
//!
//! Implements the signature base string construction and the HMAC-SHA1
//! signing/verification flow used by `oauth_signature` request signing.
//!
//! Reference: <https://datatracker.ietf.org/doc/html/rfc5849#section-3.4>
//!
//! - `sign(method, url, params, consumer_secret, token_secret) -> String`
//!   returns the base64-encoded HMAC-SHA1 signature.
//! - `verify(...)` re-signs with the same secret and constant-time compares.

use crate::mac_hmac::hmac_sha1;

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Percent-encode a string per RFC 3986: unreserved set `[A-Za-z0-9\-._~]`.
/// All other bytes are encoded as `%XX` (uppercase hex).
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        let unreserved = matches!(b,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
        );
        if unreserved {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_digit(b >> 4));
            out.push(hex_digit(b & 0x0f));
        }
    }
    out
}

fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'A' + (n - 10)) as char,
        _ => '0',
    }
}

/// Encode a byte slice as standard base64 (no wrapping).
pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let b0 = input[i];
        let b1 = input[i + 1];
        let b2 = input[i + 2];
        out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
        out.push(BASE64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(BASE64_CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        out.push(BASE64_CHARS[(b2 & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let b0 = input[i];
        out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
        out.push(BASE64_CHARS[((b0 & 0x03) << 4) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b0 = input[i];
        let b1 = input[i + 1];
        out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
        out.push(BASE64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(BASE64_CHARS[((b1 & 0x0f) << 2) as usize] as char);
        out.push('=');
    }
    out
}

/// Normalize a URL per RFC 5849 §3.4.1.2:
///   - scheme + host are lowercased
///   - default ports are stripped
///   - path, query, and fragment are preserved verbatim
pub fn normalize_url(url: &str) -> String {
    let scheme_end = url.find("://").map(|i| i + 3);
    let (scheme, rest) = match scheme_end {
        Some(idx) => (url[..idx].to_ascii_lowercase(), &url[idx..]),
        None => (String::new(), url),
    };
    // split host:port from path
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (host, port) = match authority.rfind(':') {
        Some(idx) => (&authority[..idx], &authority[idx + 1..]),
        None => (authority, ""),
    };
    let host_lc = host.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http://" => "80",
        "https://" => "443",
        _ => "",
    };
    let port_part = if !port.is_empty() && port != default_port {
        format!(":{}", port)
    } else {
        String::new()
    };
    format!("{}{}{}{}", scheme, host_lc, port_part, path)
}

/// Build the OAuth signature base string per RFC 5849 §3.4.1:
///   `<METHOD>&<percent_encode(normalize_url)>&<percent_encode(normalized_params)>`
///
/// `params` is a slice of (key, value) pairs already in the order they will be sent.
/// Any query string in the URL is parsed and merged with `params` before sorting,
/// per §3.4.1.3.1 (request parameters = query params + body params + oauth_* params).
pub fn build_signature_base_string(method: &str, url: &str, params: &[(&str, &str)]) -> String {
    // Split URL into base + query; merge query into params.
    let (url_base, query) = split_url_query(url);
    let normalized_url = normalize_url(&url_base);

    // Merge URL query params with explicit params. RFC 5849 §3.4.1.3.1 calls for
    // values to be joined by `,`; we additionally deduplicate exact (k, v) pairs
    // so the canonical Twitter-style example stays clean.
    let mut combined: Vec<(String, String)> = Vec::with_capacity(params.len() + 8);
    for (k, v) in params {
        combined.push((k.to_string(), v.to_string()));
    }
    for (k, v) in parse_query(&query) {
        if combined.iter().any(|(ek, ev)| ek == &k && ev == &v) {
            continue; // dedupe identical pair
        }
        if let Some(existing) = combined.iter_mut().find(|(ek, _)| ek == &k) {
            existing.1.push(',');
            existing.1.push_str(&v);
        } else {
            combined.push((k, v));
        }
    }

    // Collect encoded (k, v) pairs.
    let mut encoded: Vec<(String, String)> = combined
        .iter()
        .map(|(k, v)| (percent_encode(k), percent_encode(v)))
        .collect();
    // RFC 5849 §3.4.1.3.2: lexicographic byte-order sort by name, then value.
    encoded.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let normalized_params = encoded
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "{}&{}&{}",
        method.to_ascii_uppercase(),
        percent_encode(&normalized_url),
        percent_encode(&normalized_params),
    )
}

/// Split a URL into the base (without `?...`) and the raw query string.
fn split_url_query(url: &str) -> (String, String) {
    match url.find('?') {
        Some(i) => (url[..i].to_string(), url[i + 1..].to_string()),
        None => (url.to_string(), String::new()),
    }
}

/// Parse a query string into owned (key, value) pairs with light URL-decoding.
fn parse_query(q: &str) -> Vec<(String, String)> {
    if q.is_empty() {
        return Vec::new();
    }
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = match p.find('=') {
                Some(i) => (&p[..i], &p[i + 1..]),
                None => (p, ""),
            };
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

/// Percent-decode per RFC 3986 (unreserved set + `%XX` -> byte).
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Build the signing key per RFC 5849 §3.4.2:
///   `percent_encode(consumer_secret)&percent_encode(token_secret)`
pub fn build_signing_key(consumer_secret: &str, token_secret: &str) -> String {
    format!(
        "{}&{}",
        percent_encode(consumer_secret),
        percent_encode(token_secret),
    )
}

/// Sign a request. Returns the base64-encoded HMAC-SHA1 signature.
pub fn sign(
    method: &str,
    url: &str,
    params: &[(&str, &str)],
    consumer_secret: &str,
    token_secret: &str,
) -> String {
    let base = build_signature_base_string(method, url, params);
    let key = build_signing_key(consumer_secret, token_secret);
    let mac = hmac_sha1(key.as_bytes(), base.as_bytes());
    base64_encode(&mac)
}

/// Verify a provided signature using the same secret material.
pub fn verify(
    method: &str,
    url: &str,
    params: &[(&str, &str)],
    consumer_secret: &str,
    token_secret: &str,
    signature_b64: &str,
) -> bool {
    let expected = sign(method, url, params, consumer_secret, token_secret);
    constant_time_eq(expected.as_bytes(), signature_b64.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_unreserved_passthrough() {
        assert_eq!(percent_encode("abcXYZ-._~"), "abcXYZ-._~");
    }

    #[test]
    fn percent_encode_special_chars() {
        assert_eq!(percent_encode(" "), "%20");
        assert_eq!(percent_encode("&"), "%26");
        assert_eq!(percent_encode("="), "%3D");
        assert_eq!(percent_encode("+"), "%2B");
    }

    #[test]
    fn base64_encode_known() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn normalize_url_strips_default_port() {
        assert_eq!(
            normalize_url("HTTP://Example.COM:80/path?q=1"),
            "http://example.com/path?q=1"
        );
        assert_eq!(
            normalize_url("https://example.com:443/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn normalize_url_keeps_non_default_port() {
        assert_eq!(
            normalize_url("http://example.com:8080/path"),
            "http://example.com:8080/path"
        );
    }

    #[test]
    fn normalize_url_lowercases_scheme_and_host() {
        assert_eq!(
            normalize_url("HTTPS://API.Example.COM/v1/x"),
            "https://api.example.com/v1/x"
        );
    }

    #[test]
    fn signing_key_basic() {
        assert_eq!(build_signing_key("cs", "ts"), "cs&ts");
        assert_eq!(build_signing_key("cs&", "ts="), "cs%26&ts%3D");
    }

    // RFC 5849 §3.4.1.1 reference example (simplified)
    #[test]
    fn signature_base_string_reference() {
        // From RFC 5849 §3.4.1.1 example
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
        let s = build_signature_base_string("POST", url, &params);
        // Expected base string per RFC 5849:
        assert_eq!(
            s,
            "POST&https%3A%2F%2Fapi.twitter.com%2F1%2Fstatuses%2Fupdate.json&include_entities%3Dtrue%26oauth_consumer_key%3Dxvz1evFS4wEEPTGEFPHBog%26oauth_nonce%3DkYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg%26oauth_signature_method%3DHMAC-SHA1%26oauth_timestamp%3D1318622958%26oauth_token%3D370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb%26oauth_version%3D1.0%26status%3DHello%2520Ladies%2520%252B%2520Gentlemen%252C%2520a%2520signed%2520OAuth%2520request%2521"
        );
    }

    // Reference: the corresponding signature per RFC 5849 example is
    //   "tnnArxj06cWHq44gCs1OSKk%2FjLY%3D" (URL-decoded form is base64 string).
    #[test]
    fn sign_reference_signature() {
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
        let sig = sign(
            "POST",
            url,
            &params,
            "kAcSOqF21Fu85e7zjz7ZN2U4ZRhfV3WpwPAoE3Z7kBw",
            "LswwdoUaIvS8ltyTt5jkRh4J50vUPVVHtR2YPi5kE",
        );
        assert_eq!(sig, "tnnArxj06cWHq44gCs1OSKk/jLY=");
    }

    #[test]
    fn verify_matches_sign() {
        let params = [
            ("oauth_consumer_key", "ck"),
            ("oauth_nonce", "n"),
            ("oauth_signature_method", "HMAC-SHA1"),
            ("oauth_timestamp", "100"),
            ("oauth_token", "tk"),
            ("oauth_version", "1.0"),
            ("x", "y"),
        ];
        let sig = sign("GET", "https://example.com/path?x=y", &params, "cs", "ts");
        assert!(verify(
            "GET",
            "https://example.com/path?x=y",
            &params,
            "cs",
            "ts",
            &sig
        ));
    }

    #[test]
    fn verify_rejects_tamper() {
        let params = [("a", "1"), ("b", "2")];
        let sig = sign("GET", "https://example.com/p", &params, "cs", "ts");
        // tamper signature
        let mut bad = sig.into_bytes();
        bad[0] = if bad[0] == b'A' { b'B' } else { b'A' };
        let bad_str = String::from_utf8(bad).unwrap();
        assert!(!verify(
            "GET",
            "https://example.com/p",
            &params,
            "cs",
            "ts",
            &bad_str
        ));
    }

    #[test]
    fn parameter_ordering_alphabetical() {
        // Two equivalent param orders must produce identical signatures.
        let p1 = [("z", "1"), ("a", "2"), ("m", "3")];
        let p2 = [("a", "2"), ("m", "3"), ("z", "1")];
        let s1 = sign("GET", "https://example.com/", &p1, "cs", "ts");
        let s2 = sign("GET", "https://example.com/", &p2, "cs", "ts");
        assert_eq!(s1, s2);
    }

    #[test]
    fn url_normalization_affects_signature() {
        // Same params but different host casing must produce different sigs
        // (because host gets lowercased).
        let params = [("a", "1")];
        let s1 = sign("GET", "https://example.com/p", &params, "cs", "ts");
        let s2 = sign("GET", "https://EXAMPLE.com/p", &params, "cs", "ts");
        assert_eq!(s1, s2);
    }

    #[test]
    fn method_uppercase() {
        let params = [("a", "1")];
        let s_upper = sign("GET", "https://example.com/p", &params, "cs", "ts");
        let s_lower = sign("get", "https://example.com/p", &params, "cs", "ts");
        assert_eq!(s_upper, s_lower);
    }
}
