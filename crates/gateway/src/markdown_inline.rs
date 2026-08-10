//! Inline Markdown parser (CommonMark inline subset).
//!
//! Parses the inline portion of Markdown documents into a sequence of
//! [`Inline`] nodes. Handles emphasis (*em*, **strong**, `code`), links,
//! images, autolinks, hard line breaks, and escape sequences. Block-level
//! constructs (headings, lists, code fences) are intentionally NOT
//! covered — pair with a block parser for full CommonMark compliance.
//!
//! Reference: <https://spec.commonmark.org/0.31.2/#inline-elements>

/// A single inline element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Plain literal text.
    Text(String),
    /// Code span (backtick-delimited, literal content).
    Code(String),
    /// Soft line break (single newline).
    SoftBreak,
    /// Hard line break (two trailing spaces + newline or `\` + newline).
    HardBreak,
    /// Emphasis (*text* or _text_).
    Emphasis(Vec<Inline>),
    /// Strong emphasis (**text** or __text__).
    Strong(Vec<Inline>),
    /// Inline link `[text](href)`.
    Link { text: Vec<Inline>, href: String },
    /// Inline image `![alt](src)`.
    Image { alt: String, src: String },
}

/// Render a parsed inline tree back to HTML.
pub fn to_html(nodes: &[Inline]) -> String {
    let mut s = String::new();
    for n in nodes {
        match n {
            Inline::Text(t) => s.push_str(&html_escape(t)),
            Inline::Code(t) => s.push_str(&format!("<code>{}</code>", html_escape(t))),
            Inline::SoftBreak => s.push('\n'),
            Inline::HardBreak => s.push_str("<br/>\n"),
            Inline::Emphasis(children) => {
                s.push_str("<em>");
                s.push_str(&to_html(children));
                s.push_str("</em>");
            }
            Inline::Strong(children) => {
                s.push_str("<strong>");
                s.push_str(&to_html(children));
                s.push_str("</strong>");
            }
            Inline::Link { text, href } => {
                s.push_str(&format!("<a href=\"{}\">", html_escape(href)));
                s.push_str(&to_html(text));
                s.push_str("</a>");
            }
            Inline::Image { alt, src } => {
                s.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\"/>",
                    html_escape(src),
                    html_escape(alt)
                ));
            }
        }
    }
    s
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Parse an inline Markdown string into a list of inline nodes.
pub fn parse(input: &str) -> Vec<Inline> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut text = String::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
        // Handle hard line break (two trailing spaces + newline).
        if c == '\n' {
            if text.ends_with("  ") || text.ends_with("\\") {
                if text.ends_with('\\') {
                    text.pop();
                } else {
                    text.pop();
                    text.pop();
                }
                flush_text(&mut text, &mut out);
                out.push(Inline::HardBreak);
                i += 1;
                continue;
            }
            flush_text(&mut text, &mut out);
            out.push(Inline::SoftBreak);
            i += 1;
            continue;
        }
        // Escape sequence.
        if c == '\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1] as char;
            if matches!(
                next,
                '!' | '"'
                    | '#'
                    | '$'
                    | '%'
                    | '&'
                    | '\''
                    | '('
                    | ')'
                    | '*'
                    | '+'
                    | ','
                    | '-'
                    | '.'
                    | '/'
                    | ':'
                    | ';'
                    | '<'
                    | '='
                    | '>'
                    | '?'
                    | '@'
                    | '['
                    | '\\'
                    | ']'
                    | '^'
                    | '_'
                    | '`'
                    | '{'
                    | '|'
                    | '}'
                    | '~'
            ) {
                text.push(next);
                i += 2;
                continue;
            }
        }
        // Inline code (backtick span).
        if c == '`' {
            // Try to find a matching closing run of the same length.
            let start = i + 1;
            let mut run = 1;
            while start + run <= bytes.len() && bytes[start + run - 1] == b'`' {
                run += 1;
            }
            let mut j = start + run - 1;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let mut close = 1;
                    while j + close < bytes.len() && bytes[j + close] == b'`' {
                        close += 1;
                    }
                    if close == run {
                        let content = &input[start..j];
                        flush_text(&mut text, &mut out);
                        out.push(Inline::Code(content.to_string()));
                        i = j + close;
                        text.clear();
                        break;
                    }
                    j += close;
                } else {
                    j += 1;
                }
            }
            if i < bytes.len() && bytes[i] == b'`' {
                // Either we exited the loop normally (already consumed) or never matched.
                if j >= bytes.len() {
                    // No closing run; treat the backtick as literal.
                    text.push('`');
                    i += 1;
                }
                // Else: already advanced i inside the matched branch.
            }
            continue;
        }
        // Image `![alt](src)`.
        if c == '!' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some((end, alt, src)) = parse_link_target(input, i + 1) {
                flush_text(&mut text, &mut out);
                out.push(Inline::Image {
                    alt: alt.to_string(),
                    src: src.to_string(),
                });
                i = end;
                continue;
            }
        }
        // Link `[text](href)`.
        if c == '[' {
            if let Some((end, text_in, href)) = parse_link_target(input, i) {
                flush_text(&mut text, &mut out);
                let children = parse(text_in);
                out.push(Inline::Link {
                    text: children,
                    href: href.to_string(),
                });
                i = end;
                continue;
            }
        }
        // Strong emphasis (** or __).
        if (c == '*' || c == '_') && i + 1 < bytes.len() && bytes[i + 1] as char == c {
            if let Some((end, inner)) = find_close(input, c, i + 2, 2) {
                flush_text(&mut text, &mut out);
                out.push(Inline::Strong(parse(inner)));
                i = end;
                continue;
            }
        }
        // Emphasis (* or _).
        if c == '*' || c == '_' {
            if let Some((end, inner)) = find_close(input, c, i + 1, 1) {
                flush_text(&mut text, &mut out);
                out.push(Inline::Emphasis(parse(inner)));
                i = end;
                continue;
            }
        }
        // Default: append literal character.
        text.push(c);
        i += 1;
    }
    flush_text(&mut text, &mut out);
    out
}

fn flush_text(text: &mut String, out: &mut Vec<Inline>) {
    if !text.is_empty() {
        out.push(Inline::Text(std::mem::take(text)));
    }
}

/// Parse a `[text](href)` link target starting at `start` (which points at `[`).
/// Returns `(end_position, text_inside, href)`.
fn parse_link_target(input: &str, start: usize) -> Option<(usize, &str, &str)> {
    let bytes = input.as_bytes();
    if bytes[start] != b'[' {
        return None;
    }
    // Find matching `]`, respecting nested `[]`.
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] as char {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if j >= bytes.len() || bytes[j] != b']' {
        return None;
    }
    // Must be followed by `(`.
    if j + 1 >= bytes.len() || bytes[j + 1] != b'(' {
        return None;
    }
    // Find matching `)`, allowing nested parens (CommonMark allows 1 level).
    let mut k = j + 2;
    let mut pdepth = 1;
    while k < bytes.len() && pdepth > 0 {
        match bytes[k] as char {
            '(' => pdepth += 1,
            ')' => pdepth -= 1,
            _ => {}
        }
        if pdepth == 0 {
            break;
        }
        k += 1;
    }
    if pdepth != 0 {
        return None;
    }
    let text_in = &input[start + 1..j];
    let href = &input[j + 2..k];
    Some((k + 1, text_in, href))
}

/// Find a matching emphasis-closing run of `marker` chars starting at `from`.
/// `min_run` is 1 for `*` or 2 for `**`. Returns `(end_after_close, inner_text)`.
fn find_close(input: &str, marker: char, from: usize, min_run: usize) -> Option<(usize, &str)> {
    let bytes = input.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == marker {
            // Count run length.
            let mut run = 1;
            while i + run < bytes.len() && bytes[i + run] as char == marker {
                run += 1;
            }
            if run >= min_run {
                let inner = &input[from..i];
                return Some((i + run, inner));
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text() {
        let nodes = parse("hello world");
        assert_eq!(nodes, vec![Inline::Text("hello world".into())]);
    }

    #[test]
    fn emphasis() {
        let nodes = parse("*hello*");
        assert_eq!(
            nodes,
            vec![Inline::Emphasis(vec![Inline::Text("hello".into())])]
        );
    }

    #[test]
    fn strong() {
        let nodes = parse("**hello**");
        assert_eq!(
            nodes,
            vec![Inline::Strong(vec![Inline::Text("hello".into())])]
        );
    }

    #[test]
    fn inline_code() {
        let nodes = parse("`code`");
        assert_eq!(nodes, vec![Inline::Code("code".into())]);
    }

    #[test]
    fn link() {
        let nodes = parse("[click](https://example.com)");
        assert_eq!(
            nodes,
            vec![Inline::Link {
                text: vec![Inline::Text("click".into())],
                href: "https://example.com".into(),
            }]
        );
    }

    #[test]
    fn image() {
        let nodes = parse("![alt text](pic.png)");
        assert_eq!(
            nodes,
            vec![Inline::Image {
                alt: "alt text".into(),
                src: "pic.png".into(),
            }]
        );
    }

    #[test]
    fn soft_break() {
        let nodes = parse("line one\nline two");
        assert_eq!(
            nodes,
            vec![
                Inline::Text("line one".into()),
                Inline::SoftBreak,
                Inline::Text("line two".into()),
            ]
        );
    }

    #[test]
    fn hard_break_two_spaces() {
        let nodes = parse("line one  \nline two");
        assert_eq!(
            nodes,
            vec![
                Inline::Text("line one".into()),
                Inline::HardBreak,
                Inline::Text("line two".into()),
            ]
        );
    }

    #[test]
    fn escape_sequence() {
        let nodes = parse(r"\*literal asterisks\*");
        assert_eq!(nodes, vec![Inline::Text("*literal asterisks*".into())]);
    }

    #[test]
    fn html_escapes_special_chars() {
        assert_eq!(super::html_escape("<a>"), "&lt;a&gt;");
        assert_eq!(super::html_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn render_to_html() {
        let nodes = parse("**bold** and *em*");
        let html = to_html(&nodes);
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>em</em>"));
    }

    #[test]
    fn mixed_inline() {
        let nodes = parse("see [the docs](https://example.com) and `the code`");
        assert_eq!(nodes.len(), 4);
        assert!(matches!(nodes[0], Inline::Text(_)));
        assert!(matches!(nodes[1], Inline::Link { .. }));
        assert!(matches!(nodes[2], Inline::Text(_)));
        assert!(matches!(nodes[3], Inline::Code(_)));
    }
}
