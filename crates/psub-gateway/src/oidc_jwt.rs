//! Minimal OIDC ID token (JWT) decoder.
//!
//! This module decodes a JWT's three URL-safe base64 segments (header, payload,
//! signature) **without verifying the signature**. Verification is intentionally
//! out of scope here — callers that need to validate an ID token should pair
//! this decoder with a signature check against the issuer's JWKS (e.g. via
//! `crate::jwt_jwks`).
//!
//! Standard JWT format reference: RFC 7519. OIDC ID tokens follow the same
//! shape; OIDC-specific claims (`iss`, `sub`, `aud`, `exp`, `iat`) are surfaced
//! as a convenience struct, and any other claims land in
//! [`JwtPayload::custom_fields`] as a `serde_json::Value` map.

use std::collections::HashMap;

/// Decoded JOSE header (RFC 7515 §4).
///
/// `alg`, `kid`, and `typ` are surfaced directly. Additional JOSE header
/// parameters (e.g. `jku`, `jwk`, `x5u`, `x5c`) are preserved verbatim under
/// [`JwtHeader::extra`].
#[derive(Debug, Clone, PartialEq)]
pub struct JwtHeader {
    /// Algorithm — e.g. `RS256`, `HS256`, `ES256`, or `none`.
    pub alg: String,
    /// Optional key identifier (used to pick a verification key from a JWKS).
    pub kid: Option<String>,
    /// Optional type — typically `JWT` for ID tokens.
    pub typ: Option<String>,
    /// Any other JOSE header parameters not surfaced above.
    pub extra: HashMap<String, serde_json::Value>,
}

/// Decoded JWT payload (claims set).
///
/// Standard OIDC ID token claims are surfaced individually for ergonomic access.
/// Any other claims (custom namespace or vendor-specific) are preserved under
/// [`JwtPayload::custom_fields`].
#[derive(Debug, Clone, PartialEq)]
pub struct JwtPayload {
    /// Issuer — required by OIDC.
    pub iss: Option<String>,
    /// Subject — required by OIDC.
    pub sub: Option<String>,
    /// Audience — required by OIDC. When the underlying JSON claim is an array
    /// the first element is stored here and the full array is preserved in
    /// [`JwtPayload::custom_fields`].
    pub aud: Option<String>,
    /// Expiration time (seconds since the Unix epoch). `None` if absent.
    pub exp: Option<i64>,
    /// Issued-at (seconds since the Unix epoch). `None` if absent.
    pub iat: Option<i64>,
    /// All other claims not surfaced above (includes the original `aud` array
    /// if it was an array, plus any vendor-specific claims).
    pub custom_fields: HashMap<String, serde_json::Value>,
}

/// Decode a JWT into its (header, payload) pair.
///
/// The signature segment is **not** validated. It is exposed as raw bytes via
/// [`DecodedJwt::signature`] when callers need it for external verification.
///
/// # Errors
///
/// Returns `Err` for:
/// * tokens that do not contain exactly three `.`-separated segments
/// * non-UTF-8 header/payload JSON
/// * header/payload that fail to parse as JSON
/// * header missing the required `alg` claim
pub fn decode(token: &str) -> Result<DecodedJwt, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(format!(
            "expected 3 JWT segments, got {}",
            parts.len()
        ));
    }

    let header_bytes = b64url_decode(parts[0])?;
    let payload_bytes = b64url_decode(parts[1])?;
    let signature = b64url_decode(parts[2])?;

    let header_json: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| format!("header JSON parse failed: {e}"))?;
    let payload_json: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("payload JSON parse failed: {e}"))?;

    let header = parse_header(&header_json)?;
    let payload = parse_payload(payload_json);

    Ok(DecodedJwt {
        header,
        payload,
        signature,
    })
}

/// A fully decoded JWT — header, payload, and raw signature bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedJwt {
    pub header: JwtHeader,
    pub payload: JwtPayload,
    /// Raw signature bytes (decoded from the third segment).
    pub signature: Vec<u8>,
}

fn parse_header(v: &serde_json::Value) -> Result<JwtHeader, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "header must be a JSON object".to_string())?;
    let alg = obj
        .get("alg")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "header missing string `alg`".to_string())?
        .to_string();
    let kid = obj.get("kid").and_then(|x| x.as_str()).map(String::from);
    let typ = obj.get("typ").and_then(|x| x.as_str()).map(String::from);
    let mut extra = HashMap::new();
    for (k, val) in obj {
        if k == "alg" || k == "kid" || k == "typ" {
            continue;
        }
        extra.insert(k.clone(), val.clone());
    }
    Ok(JwtHeader {
        alg,
        kid,
        typ,
        extra,
    })
}

fn parse_payload(v: serde_json::Value) -> JwtPayload {
    let mut custom_fields = HashMap::new();
    let iss = take_string(&v, "iss", &mut custom_fields);
    let sub = take_string(&v, "sub", &mut custom_fields);
    let exp = take_int(&v, "exp", &mut custom_fields);
    let iat = take_int(&v, "iat", &mut custom_fields);

    let aud = match v.get("aud") {
        Some(serde_json::Value::String(s)) => {
            custom_fields.insert("aud".to_string(), serde_json::Value::String(s.clone()));
            Some(s.clone())
        }
        Some(serde_json::Value::Array(items)) => {
            custom_fields.insert("aud".to_string(), serde_json::Value::Array(items.clone()));
            items
                .iter()
                .find_map(|it| it.as_str().map(String::from))
        }
        _ => None,
    };

    if let Some(obj) = v.as_object() {
        for (k, val) in obj {
            if matches!(
                k.as_str(),
                "iss" | "sub" | "aud" | "exp" | "iat"
            ) {
                continue;
            }
            custom_fields.insert(k.clone(), val.clone());
        }
    }

    JwtPayload {
        iss,
        sub,
        aud,
        exp,
        iat,
        custom_fields,
    }
}

fn take_string(
    v: &serde_json::Value,
    key: &str,
    sink: &mut HashMap<String, serde_json::Value>,
) -> Option<String> {
    let val = v.get(key)?.clone();
    match val.as_str() {
        Some(s) => Some(s.to_string()),
        None => {
            sink.insert(key.to_string(), val);
            None
        }
    }
}

fn take_int(
    v: &serde_json::Value,
    key: &str,
    sink: &mut HashMap<String, serde_json::Value>,
) -> Option<i64> {
    let val = v.get(key)?.clone();
    match val.as_i64() {
        Some(n) => Some(n),
        None => {
            sink.insert(key.to_string(), val);
            None
        }
    }
}

fn b64url_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut s = s.replace('-', "+").replace('_', "/");
    let pad = (4 - s.len() % 4) % 4;
    for _ in 0..pad {
        s.push('=');
    }
    base64_decode_standard(&s)
}

fn base64_decode_standard(s: &str) -> Result<Vec<u8>, String> {
    const T: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        let v = lookup[b as usize];
        if v == 255 {
            if b == b'=' {
                continue;
            }
            return Err(format!("bad base64 character `{}`", b as char));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn b64url_encode(data: &[u8]) -> String {
        const T: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        let mut i = 0;
        while i + 3 <= data.len() {
            let n = ((data[i] as u32) << 16)
                | ((data[i + 1] as u32) << 8)
                | (data[i + 2] as u32);
            out.push(T[((n >> 18) & 0x3f) as usize] as char);
            out.push(T[((n >> 12) & 0x3f) as usize] as char);
            out.push(T[((n >> 6) & 0x3f) as usize] as char);
            out.push(T[(n & 0x3f) as usize] as char);
            i += 3;
        }
        let rem = data.len() - i;
        if rem == 1 {
            let n = (data[i] as u32) << 16;
            out.push(T[((n >> 18) & 0x3f) as usize] as char);
            out.push(T[((n >> 12) & 0x3f) as usize] as char);
        } else if rem == 2 {
            let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
            out.push(T[((n >> 18) & 0x3f) as usize] as char);
            out.push(T[((n >> 12) & 0x3f) as usize] as char);
            out.push(T[((n >> 6) & 0x3f) as usize] as char);
        }
        out
    }

    fn make_token(header_json: &str, payload_json: &str, sig: &[u8]) -> String {
        let h = b64url_encode(header_json.as_bytes());
        let p = b64url_encode(payload_json.as_bytes());
        let s = b64url_encode(sig);
        format!("{h}.{p}.{s}")
    }

    #[test]
    fn decode_valid_three_segment_token() {
        let header = r##"{"alg":"RS256","kid":"k1","typ":"JWT"}"##;
        let payload = r##"{"iss":"https://issuer","sub":"alice","aud":"app","exp":1800000000,"iat":1700000000}"##;
        let sig = b"\x01\x02\x03\x04\x05";
        let token = make_token(header, payload, sig);

        let decoded = decode(&token).unwrap();
        assert_eq!(decoded.header.alg, "RS256");
        assert_eq!(decoded.header.kid.as_deref(), Some("k1"));
        assert_eq!(decoded.header.typ.as_deref(), Some("JWT"));
        assert_eq!(decoded.payload.iss.as_deref(), Some("https://issuer"));
        assert_eq!(decoded.payload.sub.as_deref(), Some("alice"));
        assert_eq!(decoded.payload.aud.as_deref(), Some("app"));
        assert_eq!(decoded.payload.exp, Some(1800000000));
        assert_eq!(decoded.payload.iat, Some(1700000000));
        assert_eq!(decoded.signature, sig);
    }

    #[test]
    fn header_parse_preserves_alg_kid_typ() {
        let header = r##"{"alg":"ES256","kid":"abc","typ":"JWT"}"##;
        let payload = r##"{"sub":"x"}"##;
        let token = make_token(header, payload, b"");
        let decoded = decode(&token).unwrap();
        assert_eq!(decoded.header.alg, "ES256");
        assert_eq!(decoded.header.kid.as_deref(), Some("abc"));
        assert_eq!(decoded.header.typ.as_deref(), Some("JWT"));
        assert!(decoded.header.extra.is_empty());
    }

    #[test]
    fn header_with_extra_fields() {
        let header = r##"{"alg":"RS256","kid":"k1","x5t":"thumb"}"##;
        let payload = r##"{"sub":"x"}"##;
        let token = make_token(header, payload, b"\x00");
        let decoded = decode(&token).unwrap();
        assert_eq!(decoded.header.alg, "RS256");
        assert_eq!(
            decoded.header.extra.get("x5t").and_then(|v| v.as_str()),
            Some("thumb")
        );
    }

    #[test]
    fn payload_parse_extracts_custom_fields() {
        let header = r##"{"alg":"HS256"}"##;
        let payload = r##"{"iss":"https://x","sub":"alice","email":"alice@example.com","roles":["admin","ops"]}"##;
        let token = make_token(header, payload, b"sig");
        let decoded = decode(&token).unwrap();
        assert_eq!(decoded.payload.iss.as_deref(), Some("https://x"));
        assert_eq!(decoded.payload.sub.as_deref(), Some("alice"));
        let email = decoded
            .payload
            .custom_fields
            .get("email")
            .and_then(|v| v.as_str());
        assert_eq!(email, Some("alice@example.com"));
        let roles = decoded
            .payload
            .custom_fields
            .get("roles")
            .and_then(|v| v.as_array())
            .map(|a| a.len());
        assert_eq!(roles, Some(2));
    }

    #[test]
    fn payload_with_aud_array_takes_first_and_preserves_full() {
        let header = r##"{"alg":"RS256"}"##;
        let payload =
            r##"{"sub":"x","aud":["app1","app2"]}"##;
        let token = make_token(header, payload, b"\x00");
        let decoded = decode(&token).unwrap();
        assert_eq!(decoded.payload.aud.as_deref(), Some("app1"));
        let arr = decoded
            .payload
            .custom_fields
            .get("aud")
            .and_then(|v| v.as_array())
            .map(|a| a.len());
        assert_eq!(arr, Some(2));
    }

    #[test]
    fn signature_stored_as_raw_bytes() {
        let header = r##"{"alg":"RS256"}"##;
        let payload = r##"{"sub":"x"}"##;
        let sig = &[0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04];
        let token = make_token(header, payload, sig);
        let decoded = decode(&token).unwrap();
        assert_eq!(decoded.signature, sig);
    }

    #[test]
    fn rejects_two_segment_token() {
        let token = "abc.def";
        assert!(decode(token).is_err());
    }

    #[test]
    fn rejects_missing_alg() {
        let header = r##"{"kid":"k1"}"##;
        let payload = r##"{"sub":"x"}"##;
        let token = make_token(header, payload, b"");
        let err = decode(&token).unwrap_err();
        assert!(err.contains("alg"));
    }

    #[test]
    fn rejects_non_json_header() {
        // "garbage" is not valid JSON.
        let token = make_token("garbage", r##"{"sub":"x"}"##, b"");
        assert!(decode(&token).is_err());
    }
}