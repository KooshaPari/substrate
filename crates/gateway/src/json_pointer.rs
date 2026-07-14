//! RFC 6901 JSON Pointer parser and resolver.
//!
//! A JSON Pointer is a Unicode string (`/foo/bar/0`) that locates a
//! specific value inside a JSON document. The empty string `""`
//! refers to the whole document; `"/"` selects the member whose name is
//! the empty string.
//!
//! Reference token grammar:
//! - `pointer   ::= "" | ("/" reference-token)+`
//! - `reference ::= *(unescaped / escaped)`
//! - `escaped   ::= "~" ( "0" / "1" )` where `~0` decodes to `~` and
//!   `~1` decodes to `/`.
//!
//! This module provides:
//! - [`parse`] to split a pointer string into reference tokens.
//! - [`evaluate`] to walk a JSON document and return a value at the
//!   pointer (or `None` if any segment is missing).
//!
//! The document is represented generically via the [`Json`] enum so the
//! caller can plug in any upstream parser (e.g. `serde_json::Value`).

use std::collections::BTreeMap;

/// A minimal JSON value enum used by JSON Pointer operations.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

/// Parse a JSON Pointer string into its reference tokens.
///
/// Examples:
/// - `parse("")` == `Vec::new()` (whole document)
/// - `parse("/foo/0")` == `["foo", "0"]`
/// - `parse("/a~1b")` == `["a/b"]`
/// - `parse("/a~0b")` == `["a~b"]`
pub fn parse(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(format!("json pointer must start with '/': {:?}", pointer));
    }
    let mut tokens = Vec::new();
    for raw in pointer.split('/').skip(1) {
        // Unescape ~1 -> / and ~0 -> ~. The replacement order is important
        // because ~0 must be escaped AFTER ~1 to avoid double-substitution.
        let decoded = raw.replace("~1", "/").replace("~0", "~");
        tokens.push(decoded);
    }
    Ok(tokens)
}

/// Walk `doc` according to `tokens`. Returns `Some(&Json)` for the
/// targeted value or `None` if any segment is missing or the index is
/// out of bounds for an array.
pub fn evaluate<'a>(doc: &'a Json, tokens: &[String]) -> Option<&'a Json> {
    if tokens.is_empty() {
        return Some(doc);
    }
    let mut node = doc;
    for tok in tokens {
        node = match node {
            Json::Array(arr) => {
                let idx: usize = tok.parse().ok()?;
                arr.get(idx)?
            }
            Json::Object(map) => map.get(tok)?,
            _ => return None,
        };
    }
    Some(node)
}

/// Convenience: parse and evaluate in one call.
pub fn at<'a>(doc: &'a Json, pointer: &str) -> Result<Option<&'a Json>, String> {
    let tokens = parse(pointer)?;
    Ok(evaluate(doc, &tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Json {
        let mut obj = BTreeMap::new();
        obj.insert(
            "foo".to_string(),
            Json::Array(vec![
                Json::Number(1.0),
                Json::Number(2.0),
                Json::String("three".to_string()),
            ]),
        );
        obj.insert("".to_string(), Json::Bool(true));
        Json::Object(obj)
    }

    #[test]
    fn parse_empty() {
        assert_eq!(parse("").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn parse_root_slash() {
        assert_eq!(parse("/").unwrap(), vec![""]);
    }

    #[test]
    fn parse_simple() {
        assert_eq!(parse("/foo/0").unwrap(), vec!["foo", "0"]);
        assert_eq!(parse("/foo/2").unwrap(), vec!["foo", "2"]);
    }

    #[test]
    fn parse_escapes() {
        assert_eq!(parse("/a~1b").unwrap(), vec!["a/b"]);
        assert_eq!(parse("/a~0b").unwrap(), vec!["a~b"]);
        assert_eq!(parse("/a~01").unwrap(), vec!["a~1"]);
    }

    #[test]
    fn parse_must_start_with_slash() {
        assert!(parse("foo").is_err());
        assert!(parse("foo/bar").is_err());
    }

    #[test]
    fn evaluate_root() {
        let d = doc();
        assert_eq!(at(&d, "").unwrap(), Some(&d));
    }

    #[test]
    fn evaluate_array_index() {
        let d = doc();
        assert_eq!(at(&d, "/foo/0").unwrap(), Some(&Json::Number(1.0)));
        assert_eq!(
            at(&d, "/foo/2").unwrap(),
            Some(&Json::String("three".to_string()))
        );
    }

    #[test]
    fn evaluate_missing_key() {
        let d = doc();
        assert!(at(&d, "/missing").unwrap().is_none());
    }

    #[test]
    fn evaluate_out_of_bounds_index() {
        let d = doc();
        assert!(at(&d, "/foo/99").unwrap().is_none());
    }

    #[test]
    fn evaluate_through_non_container() {
        let d = doc();
        // /foo/0 is a number; can't index further.
        assert!(at(&d, "/foo/0/x").unwrap().is_none());
    }

    #[test]
    fn evaluate_empty_key_in_object() {
        // / refers to the member with the empty-string key.
        let d = doc();
        assert_eq!(at(&d, "/").unwrap(), Some(&Json::Bool(true)));
    }

    #[test]
    fn round_trip_on_complex_doc() {
        // Build a nested structure and verify pointer traversal.
        let mut inner = BTreeMap::new();
        inner.insert("k".to_string(), Json::String("v".to_string()));
        let mut outer = BTreeMap::new();
        outer.insert("nested".to_string(), Json::Object(inner));
        let doc = Json::Object(outer);
        assert_eq!(
            at(&doc, "/nested/k").unwrap(),
            Some(&Json::String("v".to_string()))
        );
    }
}
