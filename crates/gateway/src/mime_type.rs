//! MIME type registry and parser.
//!
//! Parses a `Content-Type`-style header into a structured
//! [`MimeType`] value with `type`, `subtype`, and an optional
//! `charset`/`boundary` parameter. Also exposes a small static lookup
//! table for the most common file extensions → MIME types
//! ([`lookup_by_extension`]).
//!
//! Reference: RFC 7231 §3.1.1.1, RFC 2046 §5.1.

use std::collections::BTreeMap;

/// A parsed MIME type value (e.g. `text/html; charset=utf-8`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MimeType {
    /// Top-level type, lowercased (e.g. `"text"`, `"application"`).
    pub type_: String,
    /// Subtype, lowercased (e.g. `"html"`, `"json"`, `"vnd.api+json"`).
    pub subtype: String,
    /// Parameter map preserving insertion order of the original header.
    pub parameters: BTreeMap<String, String>,
}

impl MimeType {
    /// Construct a `text/<subtype>` MIME.
    pub fn text(subtype: &str) -> Self {
        Self {
            type_: "text".into(),
            subtype: subtype.into(),
            ..Default::default()
        }
    }

    /// Construct an `application/<subtype>` MIME.
    pub fn application(subtype: &str) -> Self {
        Self {
            type_: "application".into(),
            subtype: subtype.into(),
            ..Default::default()
        }
    }

    /// Build the canonical `type/subtype` (no parameters).
    pub fn essence(&self) -> String {
        format!("{}/{}", self.type_, self.subtype)
    }

    /// Build the full `Content-Type` header value (essence + params).
    pub fn to_header(&self) -> String {
        let mut s = self.essence();
        for (k, v) in &self.parameters {
            s.push_str(&format!("; {}={}", k, v));
        }
        s
    }
}

/// Parse a Content-Type-style value, e.g. `"Application/JSON; charset=UTF-8"`.
/// Parameters may use quoted values for tokens with spaces/special chars.
pub fn parse(input: &str) -> Result<MimeType, String> {
    let mut iter = input.split(';');
    let essence = iter
        .next()
        .ok_or_else(|| "missing type/subtype".to_string())?
        .trim();
    let (type_, subtype) = essence
        .split_once('/')
        .ok_or_else(|| format!("missing '/' in {essence:?}"))?;
    let type_ = type_.trim().to_ascii_lowercase();
    if type_.is_empty() {
        return Err("empty type".to_string());
    }
    let subtype = subtype.trim().to_ascii_lowercase();
    if subtype.is_empty() {
        return Err("empty subtype".to_string());
    }
    let mut parameters = BTreeMap::new();
    for raw in iter {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (k, v) = trimmed
            .split_once('=')
            .ok_or_else(|| format!("parameter missing '=': {trimmed:?}"))?;
        let key = k.trim().to_ascii_lowercase();
        let value = strip_quotes(v.trim());
        parameters.insert(key, value);
    }
    Ok(MimeType {
        type_,
        subtype,
        parameters,
    })
}

fn strip_quotes(s: &str) -> String {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

const MIME_TABLE: &[(&str, &str)] = &[
    ("html", "text/html"),
    ("htm", "text/html"),
    ("css", "text/css"),
    ("csv", "text/csv"),
    ("txt", "text/plain"),
    ("md", "text/markdown"),
    ("json", "application/json"),
    ("geojson", "application/geo+json"),
    ("xml", "application/xml"),
    ("pdf", "application/pdf"),
    ("zip", "application/zip"),
    ("tar", "application/x-tar"),
    ("gz", "application/gzip"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("svg", "image/svg+xml"),
    ("webp", "image/webp"),
    ("mp3", "audio/mpeg"),
    ("mp4", "video/mp4"),
    ("webm", "video/webm"),
    ("wasm", "application/wasm"),
    ("js", "application/javascript"),
    ("mjs", "application/javascript"),
    ("yaml", "application/yaml"),
    ("yml", "application/yaml"),
    ("toml", "application/toml"),
    ("bin", "application/octet-stream"),
];

/// Look up the canonical MIME type for a file extension (lowercase,
/// without the leading dot). Returns None if no mapping is known.
pub fn lookup_by_extension(ext: &str) -> Option<&'static str> {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    MIME_TABLE.iter().find(|(k, _)| *k == e).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let m = parse("text/html").unwrap();
        assert_eq!(m.type_, "text");
        assert_eq!(m.subtype, "html");
        assert!(m.parameters.is_empty());
    }

    #[test]
    fn parse_lowercase() {
        let m = parse("Application/JSON").unwrap();
        assert_eq!(m.type_, "application");
        assert_eq!(m.subtype, "json");
    }

    #[test]
    fn parse_with_charset() {
        let m = parse("text/plain; charset=UTF-8").unwrap();
        assert_eq!(m.type_, "text");
        assert_eq!(
            m.parameters.get("charset").map(|s| s.as_str()),
            Some("UTF-8")
        );
    }

    #[test]
    fn parse_with_quoted_param() {
        let m = parse(r#"multipart/form-data; boundary="abc 123""#).unwrap();
        assert_eq!(m.type_, "multipart");
        assert_eq!(m.subtype, "form-data");
        assert_eq!(
            m.parameters.get("boundary").map(|s| s.as_str()),
            Some("abc 123")
        );
    }

    #[test]
    fn parse_multiple_parameters() {
        let m = parse("text/html; charset=UTF-8; format=flowed").unwrap();
        assert_eq!(m.parameters.len(), 2);
    }

    #[test]
    fn rejects_missing_slash() {
        assert!(parse("text").is_err());
        assert!(parse("/json").is_err());
        assert!(parse("text/").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(parse("").is_err());
    }

    #[test]
    fn rejects_param_without_equals() {
        assert!(parse("text/html; charset").is_err());
    }

    #[test]
    fn essence_and_header() {
        let m = MimeType::application("json");
        assert_eq!(m.essence(), "application/json");
        let mut m = m;
        m.parameters.insert("charset".into(), "utf-8".into());
        assert_eq!(m.to_header(), "application/json; charset=utf-8");
    }

    #[test]
    fn lookup_known() {
        assert_eq!(lookup_by_extension("html"), Some("text/html"));
        assert_eq!(lookup_by_extension("JSON"), Some("application/json"));
        assert_eq!(lookup_by_extension(".png"), Some("image/png"));
        assert_eq!(lookup_by_extension("mp4"), Some("video/mp4"));
    }

    #[test]
    fn lookup_unknown() {
        assert_eq!(lookup_by_extension("zzznotreal"), None);
    }

    #[test]
    fn lookup_multiple_extensions_for_one_mime() {
        // htm and html both map to text/html; jpg and jpeg both map to image/jpeg.
        assert_eq!(lookup_by_extension("htm"), Some("text/html"));
        assert_eq!(lookup_by_extension("jpeg"), Some("image/jpeg"));
    }
}
