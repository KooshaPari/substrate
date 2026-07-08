//! HTTP `Set-Cookie` and `Cookie` header parser.
//!
//! Parses a [`CookieHeader`] (request `Cookie:` header) as a list of name/value
//! pairs, and a [`SetCookieHeader`] (response `Set-Cookie:` header) as a
//! single cookie with optional attributes (Path, Domain, Max-Age, Expires,
//! Secure, HttpOnly, SameSite).
//!
//! Both parsers follow RFC 6265 §5.2 / §5.3 semantics with two practical
//! relaxations: whitespace is lenient in attribute parsing (browsers
//! usually send `Path=/`; we accept `Path= /` too), and quoted attribute
//! values are tolerated but optional.

/// A single request-cookie name/value pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieHeader {
    pub name: String,
    pub value: String,
}

/// A single response cookie including attributes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SetCookieHeader {
    pub name: String,
    pub value: String,
    pub path: Option<String>,
    pub domain: Option<String>,
    pub max_age: Option<u64>,
    pub expires: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<SameSite>,
    pub partitioned: bool,
}

/// SameSite attribute variants (RFC 6265bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

fn decode_percent(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(a * 16 + b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Parse a `Cookie` request header value (e.g. `"name=value; foo=bar"`).
pub fn parse_request_header(value: &str) -> Vec<CookieHeader> {
    let mut out = Vec::new();
    for pair in value.split(';') {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((n, v)) = trimmed.split_once('=') {
            out.push(CookieHeader {
                name: n.trim().to_string(),
                value: decode_percent(v.trim().trim_matches('"')),
            });
        }
    }
    out
}

/// Parse a `Set-Cookie` response header value (e.g.
/// `"id=a3fW; Path=/; Secure; HttpOnly; SameSite=Lax"`).
pub fn parse_response_header(value: &str) -> Result<SetCookieHeader, String> {
    let mut parts = value.split(';');
    let first = parts.next().ok_or_else(|| "empty set-cookie".to_string())?;
    let (name, val) = first
        .split_once('=')
        .ok_or_else(|| format!("missing '=' in set-cookie: {first:?}"))?;
    let mut cookie = SetCookieHeader {
        name: name.trim().to_string(),
        value: decode_percent(val.trim().trim_matches('"')),
        ..Default::default()
    };
    for attr in parts {
        let a = attr.trim();
        if a.is_empty() {
            continue;
        }
        if let Some((k, v)) = a.split_once('=') {
            let key = k.trim();
            let val = v.trim().trim_matches('"');
            match key.to_ascii_lowercase().as_str() {
                "path" => cookie.path = Some(val.to_string()),
                "domain" => cookie.domain = Some(val.to_string()),
                "max-age" => cookie.max_age = val.parse().ok(),
                "expires" => cookie.expires = Some(val.to_string()),
                "samesite" => {
                    cookie.same_site = match val.to_ascii_lowercase().as_str() {
                        "strict" => Some(SameSite::Strict),
                        "lax" => Some(SameSite::Lax),
                        "none" => Some(SameSite::None),
                        _ => None,
                    };
                }
                _ => {}
            }
        } else {
            match a.to_ascii_lowercase().as_str() {
                "secure" => cookie.secure = true,
                "httponly" => cookie.http_only = true,
                "partitioned" => cookie.partitioned = true,
                _ => {}
            }
        }
    }
    Ok(cookie)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_single_cookie() {
        assert_eq!(
            parse_request_header("session=abc123"),
            vec![CookieHeader {
                name: "session".into(),
                value: "abc123".into()
            }]
        );
    }

    #[test]
    fn request_multiple_cookies() {
        let cs = parse_request_header("a=1; b=2; c=3");
        assert_eq!(cs.len(), 3);
        assert_eq!(cs[0].name, "a");
        assert_eq!(cs[2].name, "c");
        assert_eq!(cs[2].value, "3");
    }

    #[test]
    fn request_trims_whitespace() {
        let cs = parse_request_header("a=1 ;  b=2");
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].value, "1");
        assert_eq!(cs[1].value, "2");
    }

    #[test]
    fn request_strips_quotes() {
        let cs = parse_request_header(r#"a="hello world""#);
        assert_eq!(cs[0].value, "hello world");
    }

    #[test]
    fn request_percent_decoding() {
        let cs = parse_request_header("greeting=hello%20world");
        assert_eq!(cs[0].value, "hello world");
    }

    #[test]
    fn response_simple() {
        let c = parse_response_header("id=abc").unwrap();
        assert_eq!(c.name, "id");
        assert_eq!(c.value, "abc");
        assert!(!c.secure);
    }

    #[test]
    fn response_with_attributes() {
        let c = parse_response_header("id=abc; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=3600")
            .unwrap();
        assert_eq!(c.name, "id");
        assert_eq!(c.path.as_deref(), Some("/"));
        assert!(c.secure);
        assert!(c.http_only);
        assert_eq!(c.same_site, Some(SameSite::Lax));
        assert_eq!(c.max_age, Some(3600));
    }

    #[test]
    fn response_strict_samesite() {
        let c = parse_response_header("a=b; SameSite=Strict").unwrap();
        assert_eq!(c.same_site, Some(SameSite::Strict));
    }

    #[test]
    fn response_none_samesite() {
        let c = parse_response_header("a=b; SameSite=None").unwrap();
        assert_eq!(c.same_site, Some(SameSite::None));
    }

    #[test]
    fn response_partitioned() {
        let c = parse_response_header("a=b; Partitioned").unwrap();
        assert!(c.partitioned);
    }

    #[test]
    fn response_domain_and_expires() {
        let c = parse_response_header("a=b; Domain=example.com; Expires=Wed, 21 Oct 2026 07:28:00 GMT")
            .unwrap();
        assert_eq!(c.domain.as_deref(), Some("example.com"));
        assert_eq!(c.expires.as_deref(), Some("Wed, 21 Oct 2026 07:28:00 GMT"));
    }

    #[test]
    fn response_rejects_missing_equals() {
        assert!(parse_response_header("novalue").is_err());
    }
}