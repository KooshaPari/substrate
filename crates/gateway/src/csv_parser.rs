//! RFC 4180 CSV parser and writer.
//!
//! Parses CSV records with quoted fields, escaped quotes (""), and
//! embedded newlines (inside quoted fields). Supports `\r\n` and `\n`
//! row terminators. Trims a single optional trailing newline if present;
//! leading newlines produce empty records.
//!
//! Use [`parse_line`] for a single line without embedded-newline support
//! (cheaper) and [`parse`] for full RFC 4180 multiline input.

/// Parse a single CSV line (no embedded newlines inside quotes). Returns
/// the fields as `Vec<String>`.
pub fn parse_line(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                if i + 1 < chars.len() && chars[i + 1] == '"' {
                    field.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
                i += 1;
                continue;
            }
            field.push(c);
            i += 1;
        } else {
            if c == '"' && field.is_empty() {
                in_quotes = true;
                i += 1;
                continue;
            }
            if c == ',' {
                out.push(std::mem::take(&mut field));
                i += 1;
                continue;
            }
            field.push(c);
            i += 1;
        }
    }
    out.push(field);
    out
}

/// Parse a full RFC 4180 CSV document (multiline, embedded newlines
/// inside quoted fields). Returns each row as `Vec<String>`.
pub fn parse(input: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        let c = b as char;
        if in_quotes {
            if c == '"' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                    field.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
                i += 1;
                continue;
            }
            field.push(c);
            i += 1;
        } else {
            match c {
                '"' if field.is_empty() => {
                    in_quotes = true;
                    i += 1;
                }
                ',' => {
                    row.push(std::mem::take(&mut field));
                    i += 1;
                }
                '\r' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    // Swallow an optional following \n.
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    i += 1;
                }
                _ => {
                    field.push(c);
                    i += 1;
                }
            }
        }
    }
    // Trailing field/row if input doesn't end with a newline.
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Serialize a single record back to CSV. Quotes any field containing
/// commas, quotes, or newlines; doubles internal quotes.
pub fn write_record<I, S>(record: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = String::new();
    for (i, field) in record.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let f = field.as_ref();
        let needs_quotes =
            f.contains(',') || f.contains('"') || f.contains('\n') || f.contains('\r');
        if needs_quotes {
            out.push('"');
            for ch in f.chars() {
                if ch == '"' {
                    out.push_str("\"\"");
                } else {
                    out.push(ch);
                }
            }
            out.push('"');
        } else {
            out.push_str(f);
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_simple() {
        assert_eq!(parse_line("a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_line_quoted() {
        assert_eq!(
            parse_line(r#"hello,"world, with comma","with ""quotes"""#),
            vec!["hello", "world, with comma", "with \"quotes\""]
        );
    }

    #[test]
    fn parse_line_empty_fields() {
        assert_eq!(parse_line("a,,b"), vec!["a", "", "b"]);
        assert_eq!(parse_line(",,,"), vec!["", "", "", ""]);
    }

    #[test]
    fn parse_multiline() {
        let csv = "a,b\nc,d\n";
        assert_eq!(parse(csv), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_crlf() {
        let csv = "a,b\r\nc,d\r\n";
        assert_eq!(parse(csv), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_quoted_newline() {
        let csv = "a,\"b\nc\",d";
        assert_eq!(parse(csv), vec![vec!["a", "b\nc", "d"]]);
    }

    #[test]
    fn parse_escaped_quote() {
        let csv = r#"a,"he said ""hi""",b"#;
        assert_eq!(parse(csv), vec![vec!["a", "he said \"hi\"", "b"]]);
    }

    #[test]
    fn parse_no_trailing_newline() {
        assert_eq!(parse("a,b"), vec![vec!["a", "b"]]);
    }

    #[test]
    fn parse_empty_input() {
        assert_eq!(parse(""), Vec::<Vec<String>>::new());
    }

    #[test]
    fn parse_single_column() {
        assert_eq!(parse("a\nb\nc\n"), vec![vec!["a"], vec!["b"], vec!["c"]]);
    }

    #[test]
    fn write_simple() {
        assert_eq!(write_record(vec!["a", "b", "c"]), "a,b,c\n");
    }

    #[test]
    fn write_escapes_commas() {
        assert_eq!(write_record(vec!["a", "b,c", "d"]), "a,\"b,c\",d\n");
    }

    #[test]
    fn write_escapes_quotes() {
        assert_eq!(write_record(vec!["a", "b\"c", "d"]), "a,\"b\"\"c\",d\n");
    }

    #[test]
    fn write_empty_fields() {
        assert_eq!(write_record(vec!["", "", ""]), ",,\n");
    }

    #[test]
    fn round_trip() {
        let original = "name,age\n\"Smith, Alice\",30\nBob,25\n";
        let parsed = parse(original);
        let mut out = String::new();
        for r in &parsed {
            out.push_str(&write_record(r));
        }
        assert_eq!(out, original);
    }
}
