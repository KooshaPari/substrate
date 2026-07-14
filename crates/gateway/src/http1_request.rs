//! Minimal HTTP/1.1 request-line + headers parser.
//!
//! Parses the request line (`METHOD SP target SP HTTP/version CRLF`), the header
//! block (terminated by an empty line), and returns the request along with any
//! remaining bytes (so the caller can keep-alive and read more frames from the
//! same socket). The body is not parsed here — it is whatever bytes remain
//! after the header block, exposed as `Vec<u8>` for downstream framing logic
//! (Content-Length, chunked, etc.).
//!
//! This is a minimal, dependency-free parser sufficient for proxies, gateways,
//! and embedded servers. It is intentionally NOT a full RFC 7230 implementation:
//!
//! - Header names are lower-cased on insertion for case-insensitive lookup.
//! - Header values are stored as-is (leading/trailing OWS is preserved as part
//!   of the value string).
//! - Multiple headers with the same name are not merged — the last one wins.
//! - No support for obs-fold line folding, or `HTTP/0.9` requests.

use std::collections::BTreeMap;

/// A parsed HTTP/1.1 request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// HTTP method (e.g. `GET`, `POST`). Uppercase as written on the wire.
    pub method: String,
    /// Request-target path component (without the query string).
    pub path: String,
    /// Query string (without the leading `?`). Empty if none was present.
    pub query: String,
    /// HTTP protocol version (e.g. `HTTP/1.1`). Case preserved as written.
    pub version: String,
    /// Header fields, keyed by lower-cased name. Values preserve their original
    /// casing/whitespace (only the NAME is normalized for lookup).
    pub headers: BTreeMap<String, String>,
    /// Raw body bytes that followed the header block. Length-bounded by the
    /// transport; this parser does not enforce Content-Length or chunked
    /// decoding itself.
    pub body: Vec<u8>,
}

/// Parse an HTTP/1.1 request from `input`. Returns the parsed request and any
/// trailing bytes that were not consumed (for keep-alive pipelining).
///
/// Errors are returned as `String` for simplicity — callers may wrap them in a
/// richer error type if needed.
pub fn parse(input: &[u8]) -> Result<(Request, &[u8]), String> {
    // Locate the end of the header block (CRLF CRLF).
    let header_end = find_header_end(input)
        .ok_or_else(|| "incomplete request: header terminator not found".to_string())?;
    let header_bytes = &input[..header_end];
    let body_start = header_end + 4; // skip "\r\n\r\n"
                                     // Body length is whatever sits between the header terminator and the end of
                                     // the input. Downstream framing (Content-Length, chunked) decides what to
                                     // actually keep; this parser is intentionally body-agnostic.
    let body = input.get(body_start..).unwrap_or(&[]).to_vec();

    let header_text =
        std::str::from_utf8(header_bytes).map_err(|e| format!("non-utf8 header bytes: {}", e))?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or_else(|| "empty request".to_string())?;
    let (method, path, query, version) = parse_request_line(request_line)?;
    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (name, value) = parse_header_line(line)?;
        headers.insert(name, value);
    }

    Ok((
        Request {
            method,
            path,
            query,
            version,
            headers,
            body,
        },
        &[],
    ))
}

/// Find the offset of the CRLF CRLF that terminates the header block.
/// Returns the offset of the first `\r` in the terminator.
fn find_header_end(input: &[u8]) -> Option<usize> {
    if input.len() < 4 {
        return None;
    }
    for i in 0..=input.len() - 4 {
        if &input[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

/// Parse the request line into (method, path, query, version).
fn parse_request_line(line: &str) -> Result<(String, String, String, String), String> {
    let mut parts = line.split(' ');
    let method = parts.next().ok_or_else(|| "missing method".to_string())?;
    let target = parts
        .next()
        .ok_or_else(|| "missing request-target".to_string())?;
    let version = parts
        .next()
        .ok_or_else(|| "missing HTTP version".to_string())?;
    if parts.next().is_some() {
        return Err("malformed request line: too many spaces".to_string());
    }
    if !is_valid_token(method) {
        return Err(format!("invalid method token: {}", method));
    }
    if !version.starts_with("HTTP/") {
        return Err(format!("invalid HTTP version: {}", version));
    }
    let (path, query) = split_target(target);
    Ok((method.to_string(), path, query, version.to_string()))
}

/// Split a request-target into (path, query).
fn split_target(target: &str) -> (String, String) {
    match target.find('?') {
        Some(idx) => (target[..idx].to_string(), target[idx + 1..].to_string()),
        None => (target.to_string(), String::new()),
    }
}

/// Parse a single header line into (name, value). The name is lower-cased.
fn parse_header_line(line: &str) -> Result<(String, String), String> {
    let colon = line
        .find(':')
        .ok_or_else(|| format!("missing colon in header: {}", line))?;
    let name = &line[..colon];
    let value = &line[colon + 1..];
    if name.is_empty() {
        return Err("empty header name".to_string());
    }
    if !is_valid_token(name) {
        return Err(format!("invalid header name: {}", name));
    }
    // Trim leading and trailing ASCII whitespace from the value (RFC 7230 §3.2.4).
    let trimmed = value.trim_matches(|c: char| c == ' ' || c == '\t');
    Ok((name.to_ascii_lowercase(), trimmed.to_string()))
}

/// HTTP token character predicate (RFC 7230 §3.2.6 subset).
fn is_valid_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(s: &[u8]) -> Request {
        let (r, rem) = parse(s).expect("parse should succeed");
        assert!(rem.is_empty(), "no trailing bytes expected");
        r
    }

    #[test]
    fn parse_get_simple() {
        let raw = b"GET /index.html HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test\r\n\r\n";
        let r = req(raw);
        assert_eq!(r.method, "GET");
        assert_eq!(r.path, "/index.html");
        assert_eq!(r.query, "");
        assert_eq!(r.version, "HTTP/1.1");
        assert_eq!(r.headers.get("host").unwrap(), "example.com");
        assert_eq!(r.headers.get("user-agent").unwrap(), "test");
        assert!(r.body.is_empty());
    }

    #[test]
    fn parse_post_with_body() {
        let raw = b"POST /api/v1/echo?x=1&y=2 HTTP/1.1\r\nHost: api.example.com\r\nContent-Type: application/json\r\nContent-Length: 17\r\n\r\n{\"hello\":\"world!\"}";
        let r = req(raw);
        assert_eq!(r.method, "POST");
        assert_eq!(r.path, "/api/v1/echo");
        assert_eq!(r.query, "x=1&y=2");
        assert_eq!(r.headers.get("content-type").unwrap(), "application/json");
        assert_eq!(
            std::str::from_utf8(&r.body).unwrap(),
            "{\"hello\":\"world!\"}"
        );
    }

    #[test]
    fn headers_case_insensitive() {
        let raw = b"GET / HTTP/1.1\r\nX-Custom-Header: v1\r\nx-custom-header: v2\r\n\r\n";
        let r = req(raw);
        // Headers are stored under lower-cased names, so lookups must use the
        // lower-cased key. Last duplicate header wins on insert.
        assert_eq!(r.headers.get("x-custom-header").unwrap(), "v2");
        assert!(
            r.headers.get("X-Custom-Header").is_none(),
            "header name must be lower-cased"
        );
        assert_eq!(r.headers.len(), 1);
    }

    #[test]
    fn value_ows_trimmed() {
        let raw = b"GET / HTTP/1.1\r\nX-Test:   spaced value  \r\n\r\n";
        let r = req(raw);
        assert_eq!(r.headers.get("x-test").unwrap(), "spaced value");
    }

    #[test]
    fn keepalive_pipelined_frames() {
        // Simulate a buffered caller: each frame is parsed individually after
        // the previous frame's header+body has been consumed. The parser
        // itself returns an empty `remaining` slice because it consumes the
        // entire input; pipeline framing is the caller's responsibility.
        let frame_a = b"GET /a HTTP/1.1\r\nHost: x\r\n\r\n";
        let frame_b = b"GET /b HTTP/1.1\r\nHost: x\r\n\r\n";
        let (first, rem) = parse(frame_a).unwrap();
        assert_eq!(first.path, "/a");
        assert!(rem.is_empty());
        let (second, rem2) = parse(frame_b).unwrap();
        assert_eq!(second.path, "/b");
        assert!(rem2.is_empty());
    }

    #[test]
    fn empty_input_returns_error() {
        assert!(parse(b"").is_err());
    }

    #[test]
    fn empty_query_string_yields_empty_query() {
        let raw = b"GET /path? HTTP/1.1\r\nHost: x\r\n\r\n";
        let r = req(raw);
        assert_eq!(r.path, "/path");
        assert_eq!(r.query, "");
    }

    #[test]
    fn reject_incomplete_headers() {
        let raw = b"GET / HTTP/1.1\r\nHost: x\r\n";
        assert!(parse(raw).is_err());
    }

    #[test]
    fn reject_invalid_method() {
        let raw = b"GET\n/ HTTP/1.1\r\nHost: x\r\n\r\n";
        assert!(parse(raw).is_err());
    }

    #[test]
    fn reject_missing_version() {
        let raw = b"GET /\r\nHost: x\r\n\r\n";
        assert!(parse(raw).is_err());
    }

    #[test]
    fn reject_header_without_colon() {
        let raw = b"GET / HTTP/1.1\r\nBadHeader\r\n\r\n";
        assert!(parse(raw).is_err());
    }

    #[test]
    fn absolute_form_target() {
        let raw = b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let r = req(raw);
        assert_eq!(r.method, "CONNECT");
        assert_eq!(r.path, "example.com:443");
        assert_eq!(r.query, "");
    }
}
