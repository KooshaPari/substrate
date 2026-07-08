//! Unicode case folding for ASCII identifiers.
//!
//! Implements ASCII-only uppercase/lowercase transformations and case-
//! insensitive ASCII comparison. Non-ASCII bytes are passed through
//! unchanged — this is the right behaviour for the common case of matching
//! HTTP header names, MIME types, and CLI subcommands where the protocol
//! is explicitly ASCII.
//!
//! For full Unicode-aware normalization (NFKC, NFC, NFD), use the `unicode-
//! normalization` crate. This module is dependency-free and std-only.

/// ASCII lowercase: A-Z -> a-z; other bytes unchanged.
pub fn ascii_lowercase(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push((c as u8 + 32) as char);
        } else {
            out.push(c);
        }
    }
    out
}

/// ASCII uppercase: a-z -> A-Z; other bytes unchanged.
pub fn ascii_uppercase(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_lowercase() {
            out.push((c as u8 - 32) as char);
        } else {
            out.push(c);
        }
    }
    out
}

/// Case-insensitive ASCII equality. Non-ASCII bytes are compared
/// verbatim — so "naïve" != "NAÏVE" but "Foo" == "foo".
pub fn ascii_eq_ignore_case(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).all(|(x, y)| {
        x.eq_ignore_ascii_case(&y)
    })
}

/// Case-insensitive ASCII prefix check.
pub fn ascii_starts_with_ignore_case(haystack: &str, prefix: &str) -> bool {
    if prefix.len() > haystack.len() {
        return false;
    }
    haystack[..prefix.len()]
        .bytes()
        .zip(prefix.bytes())
        .all(|(x, y)| x.eq_ignore_ascii_case(&y))
}

/// Strip ASCII whitespace (SP, HT, CR, LF) from both ends of `s`.
pub fn ascii_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c == ' ' || c == '\t' || c == '\r' || c == '\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_lowercase_basic() {
        assert_eq!(ascii_lowercase("HELLO"), "hello");
        assert_eq!(ascii_lowercase("Hello World"), "hello world");
    }

    #[test]
    fn ascii_lowercase_preserves_non_ascii() {
        // The "Ï" stays as-is (not folded to ASCII)
        assert_eq!(ascii_lowercase("NAÏVE"), "naÏve");
    }

    #[test]
    fn ascii_uppercase_basic() {
        assert_eq!(ascii_uppercase("hello"), "HELLO");
        assert_eq!(ascii_uppercase("foo bar"), "FOO BAR");
    }

    #[test]
    fn ascii_uppercase_preserves_non_ascii() {
        // ASCII-only contract: non-ASCII bytes (ï = U+00EF) pass through
        // verbatim and are NOT folded to their Latin-1 uppercase counterpart.
        // The sibling `ascii_lowercase_preserves_non_ascii` test enforces the
        // same invariant in the other direction. For full Unicode case
        // folding (ï ↔ Ï, etc.) use the `unicode-normalization` crate.
        assert_eq!(ascii_uppercase("naïve"), "NAïVE");
    }

    #[test]
    fn ascii_eq_ignore_case_basic() {
        assert!(ascii_eq_ignore_case("FOO", "foo"));
        assert!(ascii_eq_ignore_case("Content-Type", "content-type"));
    }

    #[test]
    fn ascii_eq_ignore_case_different_lengths() {
        assert!(!ascii_eq_ignore_case("foo", "foobar"));
    }

    #[test]
    fn ascii_eq_ignore_case_non_ascii_verbatim() {
        // Non-ASCII bytes are not folded; the lowercase ï != uppercase Ï
        assert!(!ascii_eq_ignore_case("naïve", "NAÏVE"));
    }

    #[test]
    fn ascii_starts_with_ignore_case_basic() {
        assert!(ascii_starts_with_ignore_case("Content-Type", "content"));
        assert!(!ascii_starts_with_ignore_case("Content-Type", "type"));
    }

    #[test]
    fn ascii_starts_with_prefix_longer_than_string() {
        assert!(!ascii_starts_with_ignore_case("hi", "hello"));
    }

    #[test]
    fn ascii_trim_strips_whitespace() {
        assert_eq!(ascii_trim("  hello  "), "hello");
        assert_eq!(ascii_trim("\t\r\nfoo\n\r\t"), "foo");
    }
}