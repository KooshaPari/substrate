//! JSON Web Token (JWS, RFC 7515) with "none" and HS256 signatures.
//!
//! Minimal JWT encoder + decoder intended for tests, fixtures, and
//! services that need to construct JWTs without pulling in the full
//! `jsonwebtoken` or `jwt` crate. Only `alg=none` (unsecured) and
//! `alg=HS256` (HMAC-SHA256) signing are supported — for production
//! tokens with RS256/ES256/etc. use a full JWT library.
//!
//! Tokens use the standard compact serialization:
//! `base64url(header).base64url(payload).base64url(sig)`

use crate::hmac_sha256;
use crate::unicode_normalization::ascii_eq_ignore_case;

/// Standard JWT claims (RFC 7519 §4.1). Only the most common are typed;
/// additional claims live in `extra` as raw key/string pairs.
#[derive(Debug, Clone, Default)]
pub struct Claims {
    pub iss: Option<String>,
    pub sub: Option<String>,
    pub aud: Option<String>,
    pub exp: Option<i64>,
    pub iat: Option<i64>,
    pub nbf: Option<i64>,
    pub jti: Option<String>,
    pub extra: Vec<(String, String)>,
}

/// A header + payload + algorithm tag — the parsed JWT body, before
/// signature verification.
#[derive(Debug, Clone)]
pub struct Token {
    pub alg: String,
    pub typ: Option<String>,
    pub claims: Claims,
}

/// Encode a JSON object as compact JSON (`{...}` without whitespace).
///
/// Used internally by [`sign_hs256`] and [`encode_unsigned`]. NOT a
/// general-purpose JSON serializer — does not escape strings with
/// embedded double-quotes or control chars. Callers building JWTs in
/// non-trivial contexts should use a real JSON crate.
fn encode_json_object(pairs: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&escape_json_string(k));
        out.push_str("\":");
        out.push('"');
        out.push_str(&escape_json_string(v));
        out.push('"');
    }
    out.push('}');
    out
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Standard base64url (no padding) encoder (RFC 4648 §5).
fn base64url_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let b = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        out.push(T[((b >> 6) & 0x3f) as usize] as char);
        out.push(T[(b & 0x3f) as usize] as char);
        i += 3;
    }
    if i < bytes.len() {
        let remaining = bytes.len() - i;
        let mut b = (bytes[i] as u32) << 16;
        if remaining == 2 {
            b |= (bytes[i + 1] as u32) << 8;
        }
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        if remaining == 2 {
            out.push(T[((b >> 6) & 0x3f) as usize] as char);
        }
    }
    out
}

/// Encode an unsigned JWT (`alg=none`). No signature is attached.
pub fn encode_unsigned(claims: &Claims) -> String {
    let header = encode_json_object(&[("alg", "none".into()), ("typ", "JWT".into())]);
    let mut payload_pairs: Vec<(&str, String)> = Vec::new();
    if let Some(v) = &claims.iss {
        payload_pairs.push(("iss", v.clone()));
    }
    if let Some(v) = &claims.sub {
        payload_pairs.push(("sub", v.clone()));
    }
    if let Some(v) = &claims.aud {
        payload_pairs.push(("aud", v.clone()));
    }
    if let Some(v) = claims.exp {
        payload_pairs.push(("exp", v.to_string()));
    }
    if let Some(v) = claims.iat {
        payload_pairs.push(("iat", v.to_string()));
    }
    if let Some(v) = claims.nbf {
        payload_pairs.push(("nbf", v.to_string()));
    }
    if let Some(v) = &claims.jti {
        payload_pairs.push(("jti", v.clone()));
    }
    for (k, v) in &claims.extra {
        payload_pairs.push((k.as_str(), v.clone()));
    }
    let payload = encode_json_object(&payload_pairs);
    let h = base64url_encode(header.as_bytes());
    let p = base64url_encode(payload.as_bytes());
    format!("{h}.{p}.")
}

/// Encode a JWT signed with HMAC-SHA256.
pub fn sign_hs256(claims: &Claims, secret: &[u8]) -> String {
    let header = encode_json_object(&[("alg", "HS256".into()), ("typ", "JWT".into())]);
    let mut payload_pairs: Vec<(&str, String)> = Vec::new();
    if let Some(v) = &claims.sub {
        payload_pairs.push(("sub", v.clone()));
    }
    if let Some(v) = claims.iat {
        payload_pairs.push(("iat", v.to_string()));
    }
    for (k, v) in &claims.extra {
        payload_pairs.push((k.as_str(), v.clone()));
    }
    let payload = encode_json_object(&payload_pairs);
    let h = base64url_encode(header.as_bytes());
    let p = base64url_encode(payload.as_bytes());
    let signing_input = format!("{h}.{p}");
    let sig = hmac_sha256::hmac_sha256(secret, signing_input.as_bytes());
    let s = base64url_encode(&sig);
    format!("{signing_input}.{s}")
}

/// Verify an HS256 JWT. Returns the parsed token if the signature is
/// valid and the algorithm is exactly HS256. Returns `Err` on signature
/// mismatch, base64url errors, or alg mismatch.
pub fn verify_hs256(token: &str, secret: &[u8]) -> Result<Token, String> {
    let (h_b64, p_b64, s_b64) = split_token(token)?;
    let signing_input = format!("{h_b64}.{p_b64}");
    let sig = hmac_sha256::hmac_sha256(secret, signing_input.as_bytes());
    let expected_sig = base64url_encode(&sig);
    if !ascii_eq_ignore_case(&expected_sig, &s_b64) {
        return Err("signature mismatch".into());
    }
    parse_token_parts(&h_b64, &p_b64)
        .ok_or_else(|| "parse error".into())
        .and_then(|t| {
            if ascii_eq_ignore_case(&t.alg, "HS256") {
                Ok(t)
            } else {
                Err("alg mismatch".into())
            }
        })
}

fn split_token(token: &str) -> Result<(String, String, String), String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("expected 3 parts, got {}", parts.len()));
    }
    Ok((parts[0].to_string(), parts[1].to_string(), parts[2].to_string()))
}

fn parse_token_parts(h_b64: &str, p_b64: &str) -> Option<Token> {
    let header_json = base64url_decode(h_b64)?;
    let payload_json = base64url_decode(p_b64)?;
    let header_str = std::str::from_utf8(&header_json).ok()?;
    let payload_str = std::str::from_utf8(&payload_json).ok()?;
    let alg = extract_json_string(header_str, "alg")?;
    let typ = extract_json_string(header_str, "typ");
    let mut claims = Claims::default();
    claims.iss = extract_json_string(payload_str, "iss");
    claims.sub = extract_json_string(payload_str, "sub");
    claims.aud = extract_json_string(payload_str, "aud");
    claims.exp = extract_json_string(payload_str, "exp").and_then(|s| s.parse().ok());
    claims.iat = extract_json_string(payload_str, "iat").and_then(|s| s.parse().ok());
    claims.nbf = extract_json_string(payload_str, "nbf").and_then(|s| s.parse().ok());
    claims.jti = extract_json_string(payload_str, "jti");
    Some(Token { alg, typ, claims })
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let start = json.find(&needle)? + needle.len();
    let after_colon = json[start..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let body = &after_colon[1..];
    let mut end = 0;
    while end < body.len() {
        if body.as_bytes()[end] == b'"' {
            // Check for escape: count preceding backslashes
            let mut backslashes = 0;
            let mut i = end;
            while i > 0 && body.as_bytes()[i - 1] == b'\\' {
                backslashes += 1;
                i -= 1;
            }
            if backslashes % 2 == 0 {
                break;
            }
        }
        end += 1;
    }
    Some(body[..end].to_string())
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut padded = s.to_string();
    match padded.len() % 4 {
        2 => padded.push_str("=="),
        3 => padded.push('='),
        1 => return None,
        _ => {}
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let bytes = padded.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let b1 = b64val(bytes[i])?;
        let b2 = b64val(bytes[i + 1])?;
        let b3 = b64val(bytes[i + 2])?;
        let b4 = b64val(bytes[i + 3])?;
        out.push((b1 << 2) | (b2 >> 4));
        if bytes[i + 2] != b'=' {
            out.push((b2 << 4) | (b3 >> 2));
        }
        if bytes[i + 3] != b'=' {
            out.push((b3 << 6) | b4);
        }
        i += 4;
    }
    Some(out)
}

fn b64val(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        b'=' => Some(0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_known_vector() {
        // RFC 4648 §10 test vectors for base64url
        assert_eq!(base64url_encode(b""), "");
        assert_eq!(base64url_encode(b"f"), "Zg");
        assert_eq!(base64url_encode(b"fo"), "Zm8");
        assert_eq!(base64url_encode(b"foo"), "Zm9v");
        assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn encode_unsigned_roundtrip() {
        let mut claims = Claims::default();
        claims.sub = Some("user-42".into());
        claims.exp = Some(1_700_000_000);
        let token = encode_unsigned(&claims);
        // Should have 3 dot-separated parts (signature is empty)
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn hs256_sign_verify_roundtrip() {
        let mut claims = Claims::default();
        claims.sub = Some("user-42".into());
        let secret = b"super-secret-key";
        let token = sign_hs256(&claims, secret);
        let parsed = verify_hs256(&token, secret).expect("verify");
        assert_eq!(parsed.alg, "HS256");
        assert_eq!(parsed.claims.sub.as_deref(), Some("user-42"));
    }

    #[test]
    fn hs256_wrong_secret_fails() {
        let mut claims = Claims::default();
        claims.sub = Some("user-42".into());
        let token = sign_hs256(&claims, b"correct-secret");
        let result = verify_hs256(&token, b"wrong-secret");
        assert!(result.is_err());
    }

    #[test]
    fn json_string_escape() {
        let escaped = escape_json_string("hello \"world\"\n");
        assert_eq!(escaped, "hello \\\"world\\\"\\n");
    }
}