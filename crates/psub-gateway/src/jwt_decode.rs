//! JWT (JSON Web Token) decoder — header and payload only, no signature verification.
//!
//! Parses a `header.payload.signature` token into the typed [`Jwt`] struct
//! with decoded JSON values for the header and payload. Use [`decode_unsigned`]
//! when you don't have or need the signing key (e.g., debug inspection,
//! claim extraction, kid lookup).
//!
//! For signature verification, use the `jsonwebtoken` crate or
//! `jwt_es256` / `jwt_hs256` modules in this crate.

use std::collections::BTreeMap;

/// A parsed JSON value (sufficient for typical header/payload decoding).
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

/// Minimal JSON parser for the JWT header/payload. Handles objects,
/// arrays, strings (with escape sequences), numbers (with optional
/// exponent), booleans, and null. Tolerant of leading whitespace.
fn parse_json(input: &str) -> Result<Json, String> {
    let mut p = Parser { input, pos: 0 };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos < p.input.len() {
        return Err(format!("trailing input at {}", p.pos));
    }
    Ok(v)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.input.len() {
            let c = self.input.as_bytes()[self.pos];
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn expect(&mut self, ch: u8) -> Result<(), String> {
        match self.peek() {
            Some(c) if c == ch => {
                self.pos += 1;
                Ok(())
            }
            Some(c) => Err(format!("expected {:?} at {}, got {:?}", ch as char, self.pos, c as char)),
            None => Err(format!("expected {:?} at end of input", ch as char)),
        }
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(Json::String),
            Some(b't') => {
                self.expect_keyword(b"true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.expect_keyword(b"false")?;
                Ok(Json::Bool(false))
            }
            Some(b'n') => {
                self.expect_keyword(b"null")?;
                Ok(Json::Null)
            }
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number().map(Json::Number),
            Some(c) => Err(format!("unexpected char {:?} at {}", c as char, self.pos)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut obj = BTreeMap::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(Json::Object(obj));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            obj.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(obj));
                }
                _ => return Err(format!("expected ',' or '}}' at {}", self.pos)),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut arr = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.pos += 1;
                return Ok(Json::Array(arr));
            }
            let v = self.parse_value()?;
            arr.push(v);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(arr));
                }
                _ => return Err(format!("expected ',' or ']' at {}", self.pos)),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let bytes = self.input.as_bytes();
        let mut out = String::new();
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if c == b'\\' {
                self.pos += 1;
                if self.pos >= bytes.len() {
                    return Err("unterminated escape".to_string());
                }
                let esc = bytes[self.pos];
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\x08'),
                    b'f' => out.push('\x0c'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        if self.pos + 4 > bytes.len() {
                            return Err("bad unicode escape".to_string());
                        }
                        let hex = std::str::from_utf8(&bytes[self.pos..self.pos + 4])
                            .map_err(|_| "bad unicode escape".to_string())?;
                        let cp = u32::from_str_radix(hex, 16)
                            .map_err(|_| "bad unicode escape".to_string())?;
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                        self.pos += 4;
                    }
                    _ => return Err(format!("invalid escape {:?}", esc as char)),
                }
                continue;
            }
            // Push ASCII byte; for multi-byte chars just push a char from UTF-8.
            self.pos += 1;
            if c < 0x80 {
                out.push(c as char);
            } else {
                // Re-decode the UTF-8 char from input.
                let remaining = &self.input[self.pos - 1..];
                if let Some(ch) = remaining.chars().next() {
                    out.push(ch);
                    self.pos += ch.len_utf8() - 1;
                }
            }
        }
        Err("unterminated string".to_string())
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        let bytes = self.input.as_bytes();
        let start = self.pos;
        if bytes[self.pos] == b'-' {
            self.pos += 1;
        }
        while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos < bytes.len() && bytes[self.pos] == b'.' {
            self.pos += 1;
            while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        if self.pos < bytes.len() && (bytes[self.pos] == b'e' || bytes[self.pos] == b'E') {
            self.pos += 1;
            if self.pos < bytes.len() && (bytes[self.pos] == b'+' || bytes[self.pos] == b'-') {
                self.pos += 1;
            }
            while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let s = &self.input[start..self.pos];
        s.parse::<f64>().map_err(|e| format!("bad number {:?}: {}", s, e))
    }

    fn expect_keyword(&mut self, kw: &[u8]) -> Result<(), String> {
        let remaining = self.input.as_bytes().get(self.pos..).unwrap_or(&[]);
        if remaining.starts_with(kw) {
            self.pos += kw.len();
            Ok(())
        } else {
            Err(format!(
                "expected keyword {:?} at {}",
                std::str::from_utf8(kw).unwrap_or("?"),
                self.pos
            ))
        }
    }
}

/// Base64 URL-decoder (RFC 7515 §3 base64url without padding).
fn b64url_decode(s: &str) -> Result<Vec<u8>, String> {
    let pad_len = (4 - s.len() % 4) % 4;
    let mut padded = s.to_string();
    padded.push_str(&"=".repeat(pad_len));
    // Translate base64url -> base64 alphabet.
    let standard: String = padded
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            x => x,
        })
        .collect();
    let bytes = standard.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &b in bytes {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return Err(format!("bad base64 char {:?}", b as char)),
        };
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1u32 << bits) - 1;
        }
    }
    Ok(out)
}

/// A parsed JWT containing the header and payload as JSON values plus
/// the raw signature bytes.
#[derive(Debug, Clone)]
pub struct Jwt {
    pub header: Json,
    pub payload: Json,
    pub signature: Vec<u8>,
}

/// Decode a JWT string of the form `header.payload.signature`. Returns
/// an error if any segment is malformed or the signature base64 is
/// invalid. **Does NOT verify the signature.**
pub fn decode_unsigned(token: &str) -> Result<Jwt, String> {
    let mut parts = token.split('.');
    let header_b64 = parts.next().ok_or("missing header segment")?;
    let payload_b64 = parts.next().ok_or("missing payload segment")?;
    let signature_b64 = parts.next().ok_or("missing signature segment")?;
    if parts.next().is_some() {
        return Err("too many '.' segments".to_string());
    }
    if header_b64.is_empty() || payload_b64.is_empty() {
        return Err("empty base64 segment".to_string());
    }
    let header_bytes = b64url_decode(header_b64)?;
    let payload_bytes = b64url_decode(payload_b64)?;
    let header_str = std::str::from_utf8(&header_bytes)
        .map_err(|e| format!("header not UTF-8: {}", e))?;
    let payload_str = std::str::from_utf8(&payload_bytes)
        .map_err(|e| format!("payload not UTF-8: {}", e))?;
    Ok(Jwt {
        header: parse_json(header_str)?,
        payload: parse_json(payload_str)?,
        signature: b64url_decode(signature_b64)?,
    })
}

/// Helper: extract a string claim from the payload object.
pub fn string_claim<'a>(jwt: &'a Jwt, key: &str) -> Option<&'a str> {
    match jwt.payload {
        Json::Object(ref m) => match m.get(key) {
            Some(Json::String(s)) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A canonical HS256 JWT signed with secret "secret" for the payload
    // {"sub":"123","name":"Alice","iat":1516239022}.
    // Generated deterministically; not used for signature verification here.
    const SAMPLE_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjMiLCJuYW1lIjoiQWxpY2UiLCJpYXQiOjE1MTYyMzkwMjJ9.tJ2gKHkm7RfYx5xXr8VfZwYJcqRfsx2FvC3qJwXuJmc";

    #[test]
    fn decode_header() {
        let jwt = decode_unsigned(SAMPLE_TOKEN).unwrap();
        let alg = match &jwt.header {
            Json::Object(m) => m.get("alg").and_then(|v| if let Json::String(s) = v { Some(s.as_str()) } else { None }),
            _ => None,
        }
        .unwrap();
        assert_eq!(alg, "HS256");
        let typ = match &jwt.header {
            Json::Object(m) => m.get("typ"),
            _ => None,
        };
        assert!(matches!(typ, Some(Json::String(s)) if s == "JWT"));
    }

    #[test]
    fn decode_payload() {
        let jwt = decode_unsigned(SAMPLE_TOKEN).unwrap();
        assert_eq!(string_claim(&jwt, "sub"), Some("123"));
        assert_eq!(string_claim(&jwt, "name"), Some("Alice"));
    }

    #[test]
    fn decode_signature_bytes() {
        let jwt = decode_unsigned(SAMPLE_TOKEN).unwrap();
        assert!(!jwt.signature.is_empty());
        // Signature should be 32 bytes for HMAC-SHA256.
        assert_eq!(jwt.signature.len(), 32);
    }

    #[test]
    fn detect_missing_segment() {
        assert!(decode_unsigned("only.two").is_err());
        assert!(decode_unsigned("only.two.extra.unused").is_err());
    }

    #[test]
    fn detect_bad_base64() {
        // '!' is not a base64url char.
        assert!(decode_unsigned("!!!.eyJhbGciOiJIUzI1NiJ9.aaaa").is_err());
    }

    #[test]
    fn detect_bad_json() {
        // Header decodes to plain "advanced" — not JSON. JSON parser fails.
        assert!(decode_unsigned("YWR2YW5jZWQ.eyJzdWIiOiIxIn0.aaaa").is_err());
        // Payload decodes to plain "hi" — also not JSON.
        assert!(decode_unsigned("eyJhbGciOiJIUzI1NiJ9.aGk.AAAA").is_err());
    }
}
