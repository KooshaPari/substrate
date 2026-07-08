// Minimal MIME multipart parser (RFC 2046). Splits a multipart/* body into parts,
// extracts the Content-Disposition name and filename, and returns each part's
// headers + body.
//
// This handles the common `multipart/form-data` and `multipart/mixed` cases but
// does not implement nested multiparts or binary-safe boundary scanning with
// leading CRLF. Use a real RFC-compliant parser (mime-multipart crate) if you
// need to handle arbitrary inbound traffic — this is for in-cluster use.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Part {
    pub headers: BTreeMap<String, String>,
    pub name: Option<String>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

pub fn parse(content_type: &str, body: &[u8]) -> Result<Vec<Part>, String> {
    let boundary = extract_boundary(content_type).ok_or_else(|| "no boundary in Content-Type".to_string())?;
    let delim: Vec<u8> = format!("--{}", boundary).into_bytes();
    let mut parts = Vec::new();
    let mut pos = 0usize;
    while let Some(start) = find_subsequence(&body[pos..], &delim) {
        pos += start + delim.len();
        if body[pos..].starts_with(b"--") { break; }
        if body[pos..].starts_with(b"\r\n") { pos += 2; }
        else if body[pos..].starts_with(b"\n") { pos += 1; }
        let next = find_subsequence(&body[pos..], &delim).unwrap_or(body.len() - pos);
        let part_bytes = &body[pos..pos + next];
        pos += next;
        if let Some(p) = parse_part(part_bytes)? { parts.push(p); }
    }
    Ok(parts)
}

fn extract_boundary(ct: &str) -> Option<String> {
    for piece in ct.split(';').skip(1) {
        let piece = piece.trim();
        if let Some(rest) = piece.strip_prefix("boundary=") {
            let v = rest.trim_matches('"').to_string();
            return Some(v);
        }
    }
    None
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() { return None; }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle { return Some(i); }
    }
    None
}

fn parse_part(part_bytes: &[u8]) -> Result<Option<Part>, String> {
    let sep = find_subsequence(part_bytes, b"\r\n\r\n").or_else(|| find_subsequence(part_bytes, b"\n\n"));
    let (raw_headers, body) = match sep {
        Some(s) => {
            let header_end = if part_bytes[s..].starts_with(b"\r\n") { s + 4 } else { s + 2 };
            (&part_bytes[..s], &part_bytes[header_end..])
        }
        None => return Ok(None),
    };
    let header_str = std::str::from_utf8(raw_headers).map_err(|_| "bad header utf8")?;
    let mut headers = BTreeMap::new();
    for line in header_str.split(|c| c == '\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() { continue; }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    let cd = headers.get("content-disposition");
    let (name, filename) = if let Some(cd) = cd {
        let mut name = None;
        let mut filename = None;
        for piece in cd.split(';').skip(1) {
            let piece = piece.trim();
            if let Some(rest) = piece.strip_prefix("name=") {
                name = Some(rest.trim_matches('"').to_string());
            } else if let Some(rest) = piece.strip_prefix("filename=") {
                filename = Some(rest.trim_matches('"').to_string());
            }
        }
        (name, filename)
    } else { (None, None) };
    let content_type = headers.get("content-type").cloned();
    let body_trimmed = if body.len() >= 2 && body[body.len()-2..] == [b'\r', b'\n'] {
        &body[..body.len()-2]
    } else if body.len() >= 1 && body[body.len()-1..] == [b'\n'] {
        &body[..body.len()-1]
    } else { body };
    Ok(Some(Part {
        headers,
        name,
        filename,
        content_type,
        body: body_trimmed.to_vec(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn extract_boundary_with_quotes() {
        let ct = "multipart/form-data; boundary=\"abc123\"";
        assert_eq!(extract_boundary(ct), Some("abc123".into()));
    }
    #[test] fn extract_boundary_no_quotes() {
        let ct = "multipart/mixed; boundary=xyz";
        assert_eq!(extract_boundary(ct), Some("xyz".into()));
    }
    #[test] fn extract_boundary_missing() {
        assert_eq!(extract_boundary("text/plain"), None);
    }
    #[test] fn parse_simple_form_data() {
        let body = b"--abc\r\nContent-Disposition: form-data; name=\"field1\"\r\n\r\nhello\r\n--abc\r\nContent-Disposition: form-data; name=\"file1\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nfile-bytes\r\n--abc--\r\n";
        let parts = parse("multipart/form-data; boundary=abc", body).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name.as_deref(), Some("field1"));
        assert_eq!(parts[0].body, b"hello");
        assert_eq!(parts[1].name.as_deref(), Some("file1"));
        assert_eq!(parts[1].filename.as_deref(), Some("a.txt"));
        assert_eq!(parts[1].content_type.as_deref(), Some("text/plain"));
        assert_eq!(parts[1].body, b"file-bytes");
    }
    #[test] fn parse_empty_body() {
        let body = b"--xyz\r\nContent-Disposition: form-data; name=\"empty\"\r\n\r\n\r\n--xyz--\r\n";
        let parts = parse("multipart/mixed; boundary=xyz", body).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name.as_deref(), Some("empty"));
        assert_eq!(parts[0].body, b"");
    }
    #[test] fn missing_boundary_err() {
        assert!(parse("text/plain", b"data").is_err());
    }
    #[test] fn header_lowercased() {
        let body = b"--b\r\nContent-Type: text/plain\r\n\r\ndata\r\n--b--\r\n";
        let parts = parse("multipart/mixed; boundary=b", body).unwrap();
        assert_eq!(parts[0].headers.get("content-type").unwrap(), "text/plain");
    }
    #[test] fn find_subsequence_basic() {
        assert_eq!(find_subsequence(b"hello world", b"world"), Some(6));
        assert_eq!(find_subsequence(b"abc", b"x"), None);
        assert_eq!(find_subsequence(b"abc", b""), None);
    }
    #[test] fn body_trimmed_of_trailing_newline() {
        let body = b"--b\r\nContent-Disposition: form-data; name=\"k\"\r\n\r\nvalue\r\n--b--\r\n";
        let parts = parse("multipart/mixed; boundary=b", body).unwrap();
        assert_eq!(parts[0].body, b"value");
    }
}