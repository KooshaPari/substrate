pub fn parse_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if in_quotes && chars.peek() == Some(&'"') { cur.push('"'); chars.next(); }
            else { in_quotes = !in_quotes; }
        } else if c == ',' && !in_quotes {
            out.push(std::mem::take(&mut cur));
        } else { cur.push(c); }
    }
    out.push(cur);
    out
}
pub fn parse(s: &str) -> Vec<Vec<String>> {
    s.lines().filter(|l| !l.is_empty()).map(parse_line).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn basic() { assert_eq!(parse_line("a,b,c"), vec!["a".to_string(), "b".to_string(), "c".to_string()]); }
    #[test] fn quoted() { assert_eq!(parse_line(r#""a,b",c"#), vec!["a,b".to_string(), "c".to_string()]); }
    #[test] fn escaped() { assert_eq!(parse_line(r#""a""b",c"#), vec!["a\"b".to_string(), "c".to_string()]); }
    #[test] fn empty() { assert_eq!(parse_line(""), vec!["".to_string()]); }
    #[test] fn multi() { assert_eq!(parse("a,b\nc,d").len(), 2); }
}
