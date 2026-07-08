//! LDAP search filter parser (RFC 4515 + RFC 4517).
//!
//! LDAP search filters are a small parenthesised grammar. The
//! full ABNF in RFC 4515 §3 is:
//!
//! ```text
//! filter     = "(" filtercomp ")"
//! filtercomp = and / or / not / item
//! and        = "&" filterlist
//! or         = "|" filterlist
//! not        = "!" filter
//! filterlist = 1*filter
//! item       = simple / present / substring
//! simple     = attr filtertype assertionvalue
//! filtertype = equal / approx / greater / less
//! equal      = "="
//! approx     = "~="
//! greater    = ">="
//! less       = "<="
//! present    = attr "=*"
//! substring  = attr "=" [initial] any [final]
//! initial    = assertionvalue
//! any        = "*" *(assertionvalue "*")
//! final      = assertionvalue
//! assertionvalue = 1*any
//! ```
//!
//! This module is a *parser only*: it converts a filter string to a
//! structured [`Filter`] enum. It does not evaluate the filter or
//! know anything about the LDAP server's schema.
//!
//! Note: this is the *parity* parser, side-by-side with the existing
//! `ldap_filter` module. They share the grammar but the existing
//! module flattens simple items into a single key-value
//! representation whereas this one returns a full AST.

/// One node in the parsed filter AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// `&(filter...)` — all of the children must match.
    And(Vec<Filter>),
    /// `|(filter...)` — at least one of the children must match.
    Or(Vec<Filter>),
    /// `!(filter)` — the child must not match.
    Not(Box<Filter>),
    /// `attr=value` (equality).
    Equal(String, String),
    /// `attr~=value` (approximate match).
    Approx(String, String),
    /// `attr>=value` (greater-or-equal).
    GreaterOrEqual(String, String),
    /// `attr<=value` (less-or-equal).
    LessOrEqual(String, String),
    /// `attr=*` (presence).
    Present(String),
    /// `attr=[initial]*any*...[final]` (substring match).
    Substring {
        attr: String,
        initial: Option<String>,
        any: Vec<String>,
        final_part: Option<String>,
    },
    /// `attr:=value` (extensible match, attribute-options stripped).
    ExtensibleMatch(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterType {
    Equal,
    Approx,
    GreaterOrEqual,
    LessOrEqual,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(s: &'a str) -> Self {
        Self { bytes: s.as_bytes(), pos: 0 }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let b = self.bytes.get(self.pos).copied();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }
    fn expect(&mut self, b: u8) -> Result<(), String> {
        match self.peek() {
            Some(x) if x == b => {
                self.pos += 1;
                Ok(())
            }
            Some(x) => Err(format!("expected {:?}, got {:?}", b as char, x as char)),
            None => Err(format!("expected {:?}, got EOF", b as char)),
        }
    }
    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }
    fn slice_from(&self, start: usize) -> &'a str {
        std::str::from_utf8(&self.bytes[start..self.pos])
            .unwrap_or("")
    }
}

/// Parse an LDAP search filter. Returns the AST or a string error.
pub fn parse(input: &str) -> Result<Filter, String> {
    let mut cur = Cursor::new(input);
    let f = parse_filter(&mut cur)?;
    if !cur.at_end() {
        return Err(format!(
            "trailing input after filter: {:?}",
            cur.slice_from(cur.pos)
        ));
    }
    Ok(f)
}

fn parse_filter(cur: &mut Cursor<'_>) -> Result<Filter, String> {
    cur.expect(b'(')?;
    let f = parse_filtercomp(cur)?;
    cur.expect(b')')?;
    Ok(f)
}

fn parse_filtercomp(cur: &mut Cursor<'_>) -> Result<Filter, String> {
    match cur.peek() {
        Some(b'&') => {
            cur.bump();
            let mut list = Vec::new();
            while cur.peek() == Some(b'(') {
                list.push(parse_filter(cur)?);
            }
            if list.is_empty() {
                return Err("`&` filter list is empty".to_string());
            }
            Ok(Filter::And(list))
        }
        Some(b'|') => {
            cur.bump();
            let mut list = Vec::new();
            while cur.peek() == Some(b'(') {
                list.push(parse_filter(cur)?);
            }
            if list.is_empty() {
                return Err("`|` filter list is empty".to_string());
            }
            Ok(Filter::Or(list))
        }
        Some(b'!') => {
            cur.bump();
            let inner = parse_filter(cur)?;
            Ok(Filter::Not(Box::new(inner)))
        }
        Some(b'(') => parse_item(cur),
        // Per RFC 4515 §3 `filtercomp = and / or / not / item`, an
        // `item` (e.g. `(uid=alice)`) is a valid filtercomp at the
        // top level when the parser has already consumed the
        // surrounding `(...)` in `parse_filter`. After `parse_filter`
        // strips the outer `(`, an item starts with an attribute name
        // (a letter), not another `(`.
        Some(_) => parse_item(cur),
        None => Err("unexpected EOF in filtercomp".to_string()),
    }
}

fn parse_item(cur: &mut Cursor<'_>) -> Result<Filter, String> {
    let attr = parse_attr(cur)?;
    let ft = parse_filtertype(cur)?;
    // Capture the assertion value until the matching ')'.
    let value_start = cur.pos;
    let value = parse_assertion_value(cur)?;
    let _ = value_start;
    let _ = value;
    // Re-parse cleanly: assertion value runs until the matching ')'
    // for this item; we need to re-walk because parse_filtertype
    // didn't track depth. We could refactor but the item branch
    // has special substring handling, so re-walking is simplest.
    // Re-parse using a sub-cursor from value_start for cleanliness.
    let mut sub = Cursor {
        bytes: &cur.bytes[value_start..],
        pos: 0,
    };
    let value = parse_assertion_value(&mut sub)?;
    cur.pos = value_start + sub.pos;
    let value = value;
    match ft {
        FilterType::Equal if value == "*" => Ok(Filter::Present(attr)),
        FilterType::Equal if value.contains('*') => parse_substring(&attr, &value),
        FilterType::Equal => Ok(Filter::Equal(attr, value)),
        FilterType::Approx => Ok(Filter::Approx(attr, value)),
        FilterType::GreaterOrEqual => Ok(Filter::GreaterOrEqual(attr, value)),
        FilterType::LessOrEqual => Ok(Filter::LessOrEqual(attr, value)),
    }
}

fn parse_substring(attr: &str, value: &str) -> Result<Filter, String> {
    let parts: Vec<&str> = value.split('*').collect();
    if parts.is_empty() {
        return Err(format!("invalid substring filter: {value:?}"));
    }
    let initial = if !parts[0].is_empty() {
        Some(parts[0].to_string())
    } else {
        None
    };
    let final_part = if parts.len() > 1 && !parts.last().unwrap().is_empty() {
        Some(parts.last().unwrap().to_string())
    } else {
        None
    };
    let mut any: Vec<String> = Vec::new();
    for mid in &parts[1..parts.len().saturating_sub(1)] {
        if !mid.is_empty() {
            any.push((*mid).to_string());
        }
    }
    if initial.is_none() && final_part.is_none() && any.is_empty() {
        return Err(format!("substring filter has no non-wildcard parts: {value:?}"));
    }
    Ok(Filter::Substring {
        attr: attr.to_string(),
        initial,
        any,
        final_part,
    })
}

fn parse_attr(cur: &mut Cursor<'_>) -> Result<String, String> {
    let start = cur.pos;
    while let Some(b) = cur.peek() {
        if b == b'=' || b == b'~' || b == b'<' || b == b'>' {
            break;
        }
        cur.bump();
    }
    if cur.pos == start {
        return Err("empty attribute name".to_string());
    }
    Ok(cur.slice_from(start).to_string())
}

fn parse_filtertype(cur: &mut Cursor<'_>) -> Result<FilterType, String> {
    match cur.peek() {
        Some(b'=') => {
            cur.bump();
            Ok(FilterType::Equal)
        }
        Some(b'~') => {
            cur.bump();
            cur.expect(b'=')?;
            Ok(FilterType::Approx)
        }
        Some(b'>') => {
            cur.bump();
            cur.expect(b'=')?;
            Ok(FilterType::GreaterOrEqual)
        }
        Some(b'<') => {
            cur.bump();
            cur.expect(b'=')?;
            Ok(FilterType::LessOrEqual)
        }
        Some(c) => Err(format!("expected filter type, got {:?}", c as char)),
        None => Err("expected filter type, got EOF".to_string()),
    }
}

fn parse_assertion_value(cur: &mut Cursor<'_>) -> Result<String, String> {
    // The value runs until the matching close paren for this item,
    // handling RFC 4515 §3 escaping (\xx for arbitrary bytes).
    let mut out = String::new();
    let mut depth: i32 = 0;
    while let Some(b) = cur.peek() {
        match b {
            b'(' => {
                depth += 1;
                out.push('(');
                cur.bump();
            }
            b')' if depth == 0 => break,
            b')' => {
                depth -= 1;
                out.push(')');
                cur.bump();
            }
            b'\\' => {
                cur.bump();
                let h1 = cur
                    .bump()
                    .ok_or_else(|| "unterminated escape in assertion value".to_string())?;
                let h2 = cur
                    .bump()
                    .ok_or_else(|| "unterminated escape in assertion value".to_string())?;
                let hex = format!("{}{}", h1 as char, h2 as char);
                let byte = u8::from_str_radix(&hex, 16)
                    .map_err(|e| format!("invalid hex escape \\{hex}: {e}"))?;
                out.push(byte as char);
            }
            _ => {
                out.push(b as char);
                cur.bump();
            }
        }
    }
    if out.is_empty() {
        return Err("empty assertion value".to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_equality() {
        let f = parse("(uid=alice)").unwrap();
        assert_eq!(f, Filter::Equal("uid".into(), "alice".into()));
    }

    #[test]
    fn parses_presence() {
        let f = parse("(objectClass=*)").unwrap();
        assert_eq!(f, Filter::Present("objectClass".into()));
    }

    #[test]
    fn parses_substring() {
        let f = parse("(cn=foo*bar*baz)").unwrap();
        match f {
            Filter::Substring { attr, initial, any, final_part } => {
                assert_eq!(attr, "cn");
                assert_eq!(initial.as_deref(), Some("foo"));
                assert_eq!(any, vec!["bar".to_string()]);
                assert_eq!(final_part.as_deref(), Some("baz"));
            }
            other => panic!("expected substring, got {other:?}"),
        }
    }

    #[test]
    fn parses_leading_wildcard_substring() {
        let f = parse("(cn=*foo)").unwrap();
        match f {
            Filter::Substring { initial, any, final_part, .. } => {
                assert_eq!(initial, None);
                assert!(any.is_empty());
                assert_eq!(final_part.as_deref(), Some("foo"));
            }
            other => panic!("expected substring, got {other:?}"),
        }
    }

    #[test]
    fn parses_trailing_wildcard_substring() {
        let f = parse("(cn=foo*)").unwrap();
        match f {
            Filter::Substring { initial, any, final_part, .. } => {
                assert_eq!(initial.as_deref(), Some("foo"));
                assert!(any.is_empty());
                assert_eq!(final_part, None);
            }
            other => panic!("expected substring, got {other:?}"),
        }
    }

    #[test]
    fn parses_and() {
        let f = parse("(&(objectClass=person)(uid=alice))").unwrap();
        match f {
            Filter::And(list) => {
                assert_eq!(list.len(), 2);
                assert_eq!(list[0], Filter::Equal("objectClass".into(), "person".into()));
            }
            other => panic!("expected and, got {other:?}"),
        }
    }

    #[test]
    fn parses_or() {
        let f = parse("(|(uid=alice)(uid=bob))").unwrap();
        match f {
            Filter::Or(list) => {
                assert_eq!(list.len(), 2);
            }
            other => panic!("expected or, got {other:?}"),
        }
    }

    #[test]
    fn parses_not() {
        let f = parse("(!(uid=alice))").unwrap();
        match f {
            Filter::Not(inner) => {
                assert_eq!(*inner, Filter::Equal("uid".into(), "alice".into()));
            }
            other => panic!("expected not, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested() {
        let f = parse(
            "(&(objectClass=person)(|(uid=alice)(!(uid=bob))))",
        )
        .unwrap();
        match f {
            Filter::And(list) => {
                assert_eq!(list.len(), 2);
                assert!(matches!(list[1], Filter::Or(_)));
            }
            other => panic!("expected nested, got {other:?}"),
        }
    }

    #[test]
    fn parses_approx() {
        let f = parse("(cn~=smith)").unwrap();
        assert_eq!(f, Filter::Approx("cn".into(), "smith".into()));
    }

    #[test]
    fn parses_greater() {
        let f = parse("(age>=18)").unwrap();
        assert_eq!(f, Filter::GreaterOrEqual("age".into(), "18".into()));
    }

    #[test]
    fn parses_less() {
        let f = parse("(age<=65)").unwrap();
        assert_eq!(f, Filter::LessOrEqual("age".into(), "65".into()));
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse("").is_err());
    }

    #[test]
    fn rejects_unbalanced_parens() {
        assert!(parse("(uid=alice").is_err());
        assert!(parse("uid=alice)").is_err());
    }

    #[test]
    fn rejects_empty_and_list() {
        assert!(parse("(&)").is_err());
    }

    #[test]
    fn rejects_empty_or_list() {
        assert!(parse("(|)").is_err());
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(parse("(uid=alice)junk").is_err());
    }

    #[test]
    fn parses_middle_wildcards() {
        let f = parse("(cn=a*b*c*d*e)").unwrap();
        match f {
            Filter::Substring { initial, any, final_part, .. } => {
                assert_eq!(initial.as_deref(), Some("a"));
                assert_eq!(any, vec!["b".to_string(), "c".to_string(), "d".to_string()]);
                assert_eq!(final_part.as_deref(), Some("e"));
            }
            other => panic!("expected substring, got {other:?}"),
        }
    }
}
