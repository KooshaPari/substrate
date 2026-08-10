//! URI Template (RFC 6570) level-1 expansion for simple substitutions.
//!
//! Implements a minimal subset of RFC 6570 sufficient for the common
//! `{var}` and `{?var}` forms. Does NOT support the full level-4 grammar
//! (no nested expressions, no `#`/`+`/`/` operators beyond the basics,
//! no list/explode form expansion with comma separators).
//!
//! Use [`expand`] for `{var}` substitution; [`expand_with_query`] for
//! `{?var1,var2}` query-style expansion; [`escape`] for the RFC 6570
//! percent-encoding of reserved characters.

/// Percent-encode a value per RFC 6570 §3.2.1.
///
/// Encodes everything outside the unreserved set `[A-Za-z0-9_.-~]` using
/// uppercase hex (`%XX`). Reserved chars that are allowed inside query
/// parameters (`/?:@!$&'()*+,;=` per RFC 3986) are passed through unchanged
/// for caller convenience.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

/// Expand a `{var}` template using `vars.get(var)`. Unknown vars become
/// empty strings. Multiple occurrences of the same var are all expanded.
///
/// The template may contain multiple distinct variables — e.g.
/// `"/users/{user_id}/posts/{post_id}"` resolves both `user_id` and
/// `post_id` from the map.
pub fn expand(template: &str, vars: &std::collections::HashMap<&str, &str>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // find matching '}'
            if let Some(close) = template[i + 1..].find('}') {
                let var_name = &template[i + 1..i + 1 + close];
                let value = vars.get(var_name).copied().unwrap_or("");
                out.push_str(&escape(value));
                i = i + 1 + close + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Expand a `{?var1,var2,...}` query-style template (RFC 6570 form-style
/// query expansion). Returns `?var1=value1&var2=value2&...` with
/// percent-encoded values. Vars with empty values are skipped.
///
/// If no vars are present in the map, the `?` is also omitted.
pub fn expand_with_query(template: &str, vars: &std::collections::HashMap<&str, &str>) -> String {
    if !template.starts_with("{?") {
        return expand(template, vars);
    }
    // Find the closing brace and split on commas
    let close = match template[2..].find('}') {
        Some(p) => 2 + p,
        None => return expand(template, vars),
    };
    let names: Vec<&str> = template[2..close].split(',').map(|s| s.trim()).collect();

    let mut pairs: Vec<String> = Vec::new();
    for name in names {
        if let Some(value) = vars.get(name) {
            if !value.is_empty() {
                pairs.push(format!("{}={}", name, escape(value)));
            }
        }
    }
    if pairs.is_empty() {
        String::new()
    } else {
        format!("?{}", pairs.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn escape_unreserved_passthrough() {
        assert_eq!(escape("hello-world_1.0~"), "hello-world_1.0~");
    }

    #[test]
    fn escape_percent_encodes_space() {
        assert_eq!(escape("hello world"), "hello%20world");
    }

    #[test]
    fn escape_uppercase_hex() {
        assert_eq!(escape("a/b"), "a%2Fb");
    }

    #[test]
    fn expand_single_var() {
        let mut vars = HashMap::new();
        vars.insert("name", "alice");
        assert_eq!(expand("/users/{name}", &vars), "/users/alice");
    }

    #[test]
    fn expand_multiple_vars() {
        let mut vars = HashMap::new();
        vars.insert("user_id", "42");
        vars.insert("post_id", "7");
        assert_eq!(
            expand("/users/{user_id}/posts/{post_id}", &vars),
            "/users/42/posts/7"
        );
    }

    #[test]
    fn expand_missing_var_is_empty() {
        let vars = HashMap::new();
        assert_eq!(expand("/users/{name}", &vars), "/users/");
    }

    #[test]
    fn expand_special_chars_encoded() {
        let mut vars = HashMap::new();
        vars.insert("q", "hello world");
        assert_eq!(expand("/search/{q}", &vars), "/search/hello%20world");
    }

    #[test]
    fn expand_query_one_var() {
        let mut vars = HashMap::new();
        vars.insert("q", "rust");
        assert_eq!(expand_with_query("{?q}", &vars), "?q=rust");
    }

    #[test]
    fn expand_query_multi_vars() {
        let mut vars = HashMap::new();
        vars.insert("a", "1");
        vars.insert("b", "2");
        vars.insert("c", "3");
        assert_eq!(expand_with_query("{?a,b,c}", &vars), "?a=1&b=2&c=3");
    }

    #[test]
    fn expand_query_skips_empty_values() {
        let mut vars = HashMap::new();
        vars.insert("a", "1");
        vars.insert("b", "");
        vars.insert("c", "3");
        assert_eq!(expand_with_query("{?a,b,c}", &vars), "?a=1&c=3");
    }

    #[test]
    fn expand_query_no_vars_returns_empty() {
        let vars = HashMap::new();
        assert_eq!(expand_with_query("{?a,b}", &vars), "");
    }
}
