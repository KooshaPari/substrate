//! YAML subset parser.
//!
//! Parses a small but useful subset of YAML 1.2: scalars, sequences
//! (block and flow), maps (block and flow), nested structures, and
//! quoted strings (single and double). Indentation-sensitive block
//! collections; flow collections (`[ ]`/`{ }`) inline on a single line.
//!
//! Not supported (out of scope): anchors/aliases, tags, multi-line
//! scalars, complex keys, document separators (`---`/`...`),
//! directives (`%YAML`), and merge keys (`<<`).
//!
//! For full YAML compliance, use the `serde_yaml` crate.

use std::collections::BTreeMap;

/// A parsed YAML value.
#[derive(Debug, Clone, PartialEq)]
pub enum Yaml {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Sequence(Vec<Yaml>),
    Map(BTreeMap<String, Yaml>),
}

impl Yaml {
    /// Empty sequence/map/null convenience constructors.
    pub fn null() -> Self {
        Yaml::Null
    }
    pub fn seq() -> Self {
        Yaml::Sequence(Vec::new())
    }
    pub fn map() -> Self {
        Yaml::Map(BTreeMap::new())
    }
}

/// Parse a YAML document (single document, no front-matter).
pub fn parse(input: &str) -> Result<Yaml, String> {
    let mut lines: Vec<&str> = input.lines().collect();
    // Strip blank lines and comments-only lines.
    lines.retain(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#')
    });
    if lines.is_empty() {
        return Ok(Yaml::Null);
    }
    let mut p = Parser {
        lines: &lines,
        pos: 0,
    };
    let v = p.parse_node(0)?;
    // Allow trailing blank lines / comments after the top-level node.
    Ok(v)
}

struct Parser<'a> {
    lines: &'a [&'a str],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse_node(&mut self, min_indent: usize) -> Result<Yaml, String> {
        if self.pos >= self.lines.len() {
            return Err("unexpected end of input".to_string());
        }
        let line = self.lines[self.pos];
        let indent = count_indent(line);
        if indent < min_indent {
            return Err(format!(
                "line {} indent {} < min {}",
                self.pos, indent, min_indent
            ));
        }
        let trimmed = line[indent..].trim_start();
        if trimmed.starts_with('#') {
            return Err("unexpected comment in flow".to_string());
        }
        if trimmed.starts_with("- ") || trimmed == "-" {
            return self.parse_block_sequence(indent);
        }
        if trimmed.starts_with('[') {
            return self.parse_flow_sequence(line, indent);
        }
        if trimmed.starts_with('{') {
            return self.parse_flow_map(line, indent);
        }
        if trimmed.contains(':') {
            return self.parse_block_map(indent);
        }
        // Scalar
        self.pos += 1;
        Ok(parse_scalar(trimmed))
    }

    fn parse_block_sequence(&mut self, indent: usize) -> Result<Yaml, String> {
        let mut items = Vec::new();
        loop {
            if self.pos >= self.lines.len() {
                break;
            }
            let line = self.lines[self.pos];
            let line_indent = count_indent(line);
            if line_indent < indent {
                break;
            }
            let trimmed = line[line_indent..].trim_start();
            if !trimmed.starts_with('-') {
                break;
            }
            // Advance past the "- " (or just "-").
            let after = if trimmed.len() > 1 && trimmed.as_bytes()[1] == b' ' {
                &trimmed[2..]
            } else {
                &trimmed[1..]
            };
            if after.is_empty() {
                self.pos += 1;
                items.push(self.parse_node(indent + 2)?);
            } else {
                // Inline scalar: parse the remainder as scalar (with possible
                // "key: value" form treated as a single-item map).
                self.pos += 1;
                items.push(parse_scalar(after));
            }
        }
        Ok(Yaml::Sequence(items))
    }

    fn parse_block_map(&mut self, indent: usize) -> Result<Yaml, String> {
        let mut map = BTreeMap::new();
        loop {
            if self.pos >= self.lines.len() {
                break;
            }
            let line = self.lines[self.pos];
            let line_indent = count_indent(line);
            if line_indent < indent {
                break;
            }
            let trimmed = line[line_indent..].trim_start();
            if trimmed.starts_with('#') {
                self.pos += 1;
                continue;
            }
            // Find first ':' not inside a quoted string.
            let (key, val_inline) = match split_top_colon(trimmed) {
                Some(p) => p,
                None => {
                    return Err(format!("expected ':' in mapping at line {}", self.pos));
                }
            };
            let key = key.trim().to_string();
            self.pos += 1;
            let value = if val_inline.is_empty() {
                // Value on next line (or absent -> null).
                if self.pos < self.lines.len() {
                    let next = self.lines[self.pos];
                    let next_indent = count_indent(next);
                    let next_trimmed = next[next_indent..].trim_start();
                    if next_indent > indent && !next_trimmed.is_empty() {
                        self.parse_node(next_indent)?
                    } else {
                        Yaml::Null
                    }
                } else {
                    Yaml::Null
                }
            } else {
                parse_scalar(val_inline.trim())
            };
            map.insert(key, value);
        }
        Ok(Yaml::Map(map))
    }

    fn parse_flow_sequence(&mut self, line: &str, indent: usize) -> Result<Yaml, String> {
        let s = line[indent..].trim_start();
        let close = s.rfind(']').ok_or_else(|| "missing ]".to_string())?;
        let body = &s[1..close];
        let mut items = Vec::new();
        if body.trim().is_empty() {
            self.pos += 1;
            return Ok(Yaml::Sequence(items));
        }
        // Naive: split on top-level commas (no nested flow).
        // We don't support nested [ ] or { } inline in this minimal version.
        for part in body.split(',') {
            items.push(parse_scalar(part.trim()));
        }
        self.pos += 1;
        Ok(Yaml::Sequence(items))
    }

    fn parse_flow_map(&mut self, line: &str, indent: usize) -> Result<Yaml, String> {
        let s = line[indent..].trim_start();
        let close = s.rfind('}').ok_or_else(|| "missing }".to_string())?;
        let body = &s[1..close];
        let mut map = BTreeMap::new();
        if body.trim().is_empty() {
            self.pos += 1;
            return Ok(Yaml::Map(map));
        }
        for part in body.split(',') {
            let (k, v) = part
                .split_once(':')
                .ok_or_else(|| format!("expected ':' in flow map: {part:?}"))?;
            map.insert(k.trim().to_string(), parse_scalar(v.trim()));
        }
        self.pos += 1;
        Ok(Yaml::Map(map))
    }
}

fn count_indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn parse_scalar(s: &str) -> Yaml {
    let t = s.trim();
    if t.is_empty() {
        return Yaml::Null;
    }
    // Strip surrounding quotes.
    if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        return Yaml::String(t[1..t.len() - 1].to_string());
    }
    // Booleans and null.
    match t.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" => return Yaml::Bool(true),
        "false" | "no" | "off" => return Yaml::Bool(false),
        "null" | "~" => return Yaml::Null,
        _ => {}
    }
    if let Ok(n) = t.parse::<f64>() {
        return Yaml::Number(n);
    }
    Yaml::String(t.to_string())
}

fn split_top_colon(s: &str) -> Option<(&str, &str)> {
    // Find the first ':' that is not inside a quoted region.
    let bytes = s.as_bytes();
    let mut in_dq = false;
    let mut in_sq = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' if !in_sq => in_dq = !in_dq,
            b'\'' if !in_dq => in_sq = !in_sq,
            b':' if !in_dq && !in_sq => {
                return Some((&s[..i], &s[i + 1..]));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        assert_eq!(parse("").unwrap(), Yaml::Null);
    }

    #[test]
    fn parse_scalar_number() {
        assert_eq!(parse("42").unwrap(), Yaml::Number(42.0));
    }

    #[test]
    fn parse_scalar_bool() {
        assert_eq!(parse("true").unwrap(), Yaml::Bool(true));
        assert_eq!(parse("false").unwrap(), Yaml::Bool(false));
        assert_eq!(parse("yes").unwrap(), Yaml::Bool(true));
    }

    #[test]
    fn parse_quoted_string() {
        assert_eq!(parse(r#""hello""#).unwrap(), Yaml::String("hello".into()));
    }

    #[test]
    fn parse_block_map_simple() {
        let y = parse("name: Alice\nage: 30\n").unwrap();
        match y {
            Yaml::Map(m) => {
                assert_eq!(m.get("name"), Some(&Yaml::String("Alice".into())));
                assert_eq!(m.get("age"), Some(&Yaml::Number(30.0)));
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn parse_block_sequence() {
        let y = parse("- a\n- b\n- c\n").unwrap();
        assert_eq!(
            y,
            Yaml::Sequence(vec![
                Yaml::String("a".into()),
                Yaml::String("b".into()),
                Yaml::String("c".into())
            ])
        );
    }

    #[test]
    fn parse_nested_map_with_seq() {
        let input = "tags:\n  - rust\n  - yaml\n  - parser\nversion: 1\n";
        let y = parse(input).unwrap();
        match y {
            Yaml::Map(m) => {
                assert_eq!(
                    m.get("tags"),
                    Some(&Yaml::Sequence(vec![
                        Yaml::String("rust".into()),
                        Yaml::String("yaml".into()),
                        Yaml::String("parser".into())
                    ]))
                );
                assert_eq!(m.get("version"), Some(&Yaml::Number(1.0)));
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn parse_nested_seq_of_maps() {
        let input = "-\n  name: a\n  value: 1\n-\n  name: b\n  value: 2\n";
        let y = parse(input).unwrap();
        match y {
            Yaml::Sequence(items) => {
                assert_eq!(items.len(), 2);
                match &items[0] {
                    Yaml::Map(m) => {
                        assert_eq!(m.get("name"), Some(&Yaml::String("a".into())));
                    }
                    _ => panic!("expected map"),
                }
            }
            _ => panic!("expected sequence"),
        }
    }

    #[test]
    fn parse_flow_sequence() {
        assert_eq!(
            parse("[1, 2, 3]").unwrap(),
            Yaml::Sequence(vec![
                Yaml::Number(1.0),
                Yaml::Number(2.0),
                Yaml::Number(3.0)
            ])
        );
    }

    #[test]
    fn parse_flow_map() {
        let y = parse("{a: 1, b: two}").unwrap();
        match y {
            Yaml::Map(m) => {
                assert_eq!(m.get("a"), Some(&Yaml::Number(1.0)));
                assert_eq!(m.get("b"), Some(&Yaml::String("two".into())));
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn parse_skip_comments_and_blank() {
        let y = parse("# leading comment\n\nname: Alice\n# inline comment\nage: 30\n").unwrap();
        match y {
            Yaml::Map(m) => {
                assert_eq!(m.len(), 2);
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn parse_string_with_colon() {
        // Colon inside a quoted string should not split the key.
        let y = parse(r#"greeting: "hello: world""#).unwrap();
        match y {
            Yaml::Map(m) => {
                assert_eq!(
                    m.get("greeting"),
                    Some(&Yaml::String("hello: world".into()))
                );
            }
            _ => panic!("expected map"),
        }
    }
}
