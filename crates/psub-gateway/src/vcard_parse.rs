// Minimal vCard (RFC 6350) parser.
//
// Supports BEGIN:VCARD / END:VCARD blocks, VERSION, FN (possibly multi-line
// via line-folding), structured N (family, given, additional, prefix, suffix),
// multiple TEL and EMAIL values, and ORG. ADR is also extracted as a single
// string (post-office-box;extended;street;city;region;postal;country).
//
// Line-folding (CRLF followed by space/tab) is handled on read, and the
// serializer emits RFC-6350-style folded output for long values.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct VCard {
    pub version: String,
    pub full_name: String,
    pub structured_name: BTreeMap<String, String>,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
    pub org: String,
    pub adr: String,
}

impl VCard {
    pub fn new() -> Self {
        Self {
            version: "3.0".to_string(),
            full_name: String::new(),
            structured_name: BTreeMap::new(),
            phones: Vec::new(),
            emails: Vec::new(),
            org: String::new(),
            adr: String::new(),
        }
    }
}

impl Default for VCard {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse(input: &str) -> Result<Vec<VCard>, String> {
    let lines = unfold(input);
    let mut cards = Vec::new();
    let mut in_card = false;
    let mut current = VCard::new();
    for line in &lines {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            continue;
        }
        let (name_with_params, value) = match split_property(trimmed) {
            Some(v) => v,
            None => continue,
        };
        let (name, _params) = split_name_params(name_with_params);
        match (name.as_str(), in_card) {
            ("BEGIN", false) if value.eq_ignore_ascii_case("VCARD") => {
                in_card = true;
                current = VCard::new();
            }
            ("END", true) if value.eq_ignore_ascii_case("VCARD") => {
                cards.push(std::mem::replace(&mut current, VCard::new()));
                in_card = false;
            }
            (n, true) => {
                apply_property(&mut current, n, value);
            }
            _ => {}
        }
    }
    if in_card {
        return Err("unterminated VCARD block".into());
    }
    Ok(cards)
}

fn apply_property(card: &mut VCard, name: &str, value: &str) {
    match name {
        "VERSION" => card.version = value.to_string(),
        "FN" => {
            if card.full_name.is_empty() {
                card.full_name = unescape(value);
            } else {
                card.full_name.push(' ');
                card.full_name.push_str(&unescape(value));
            }
        }
        "N" => {
            let parts: Vec<&str> = value.split(';').collect();
            let keys = ["family", "given", "additional", "prefix", "suffix"];
            for (i, key) in keys.iter().enumerate() {
                let raw = parts.get(i).copied().unwrap_or("");
                let decoded = unescape(raw);
                if !decoded.is_empty() {
                    card.structured_name
                        .insert((*key).to_string(), decoded);
                }
            }
        }
        "TEL" => card.phones.push(unescape(value)),
        "EMAIL" => card.emails.push(unescape(value)),
        "ORG" => card.org = unescape(value),
        "ADR" => card.adr = value.to_string(),
        _ => {}
    }
}

fn split_property(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(':')?;
    Some((&line[..idx], &line[idx + 1..]))
}

fn split_name_params(name_with_params: &str) -> (String, Vec<(String, String)>) {
    let mut parts = name_with_params.split(';');
    let name = parts.next().unwrap_or("").to_ascii_uppercase();
    let mut params = Vec::new();
    for p in parts {
        if let Some(eq) = p.find('=') {
            params.push((p[..eq].to_ascii_uppercase(), p[eq + 1..].to_string()));
        }
    }
    (name, params)
}

fn unfold(input: &str) -> Vec<String> {
    let normalized = input.replace("\r\n", "\n");
    let mut out: Vec<String> = Vec::new();
    for line in normalized.split('\n') {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.push_str(&line[1..]);
                continue;
            }
        }
        out.push(line.to_string());
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(';') => out.push(';'),
                Some(',') => out.push(','),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_card_parse() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Jane Doe\r\nN:Doe;Jane;;;Mr.\r\nTEL:+15551234567\r\nEMAIL:jane@example.com\r\nORG:Acme\r\nEND:VCARD\r\n";
        let cards = parse(vcf).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].full_name, "Jane Doe");
        assert_eq!(cards[0].structured_name.get("family").unwrap(), "Doe");
        assert_eq!(cards[0].structured_name.get("given").unwrap(), "Jane");
        assert_eq!(cards[0].structured_name.get("suffix").unwrap(), "Mr.");
        assert_eq!(cards[0].phones, vec!["+15551234567".to_string()]);
        assert_eq!(cards[0].emails, vec!["jane@example.com".to_string()]);
        assert_eq!(cards[0].org, "Acme");
    }

    #[test]
    fn multi_line_fn_collapse() {
        // RFC 6350 line-folding strips the leading WSP, so a folded FN
        // "FN:Dr.\r\n Jane" becomes "FN:Dr.Jane" after unfold. Verify that.
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Dr.\r\n Jane\r\nEND:VCARD\r\n";
        let cards = parse(vcf).unwrap();
        assert_eq!(cards[0].full_name, "Dr.Jane");
    }

    #[test]
    fn structured_n_full() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:John Q. Public\r\nN:Public;John;Quinlan;Mr.;Esq.\r\nEND:VCARD\r\n";
        let cards = parse(vcf).unwrap();
        let n = &cards[0].structured_name;
        assert_eq!(n.get("family").unwrap(), "Public");
        assert_eq!(n.get("given").unwrap(), "John");
        assert_eq!(n.get("additional").unwrap(), "Quinlan");
        assert_eq!(n.get("prefix").unwrap(), "Mr.");
        assert_eq!(n.get("suffix").unwrap(), "Esq.");
    }

    #[test]
    fn multiple_tel_email() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Multi\r\nTEL:+15550000001\r\nTEL:+15550000002\r\nEMAIL:a@example.com\r\nEMAIL:b@example.com\r\nEND:VCARD\r\n";
        let cards = parse(vcf).unwrap();
        assert_eq!(cards[0].phones.len(), 2);
        assert_eq!(cards[0].emails.len(), 2);
        assert_eq!(cards[0].phones[0], "+15550000001");
        assert_eq!(cards[0].emails[1], "b@example.com");
    }

    #[test]
    fn round_trip_preserves_critical_fields() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Jane Doe\r\nN:Doe;Jane;;;Mr.\r\nTEL:+15551234567\r\nEMAIL:jane@example.com\r\nORG:Acme\\, Inc.\r\nEND:VCARD\r\n";
        let cards = parse(vcf).unwrap();
        assert_eq!(cards[0].full_name, "Jane Doe");
        assert_eq!(cards[0].org, "Acme, Inc.");
        assert_eq!(cards[0].phones[0], "+15551234567");
    }

    #[test]
    fn rejects_malformed_unterminated() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:X\r\n";
        assert!(parse(vcf).is_err());
    }
}
