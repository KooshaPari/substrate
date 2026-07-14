use std::collections::HashMap;

pub struct IniDoc {
    pub sections: HashMap<String, HashMap<String, String>>,
}
impl IniDoc {
    pub fn new() -> Self {
        Self {
            sections: HashMap::new(),
        }
    }
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(section)
            .and_then(|s| s.get(key))
            .map(|v| v.as_str())
    }
    pub fn set(
        &mut self,
        section: impl Into<String>,
        key: impl Into<String>,
        val: impl Into<String>,
    ) {
        self.sections
            .entry(section.into())
            .or_default()
            .insert(key.into(), val.into());
    }
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }
    pub fn keys_in(&self, section: &str) -> usize {
        self.sections.get(section).map(|s| s.len()).unwrap_or(0)
    }
}
pub fn parse(s: &str) -> Result<IniDoc, String> {
    let mut doc = IniDoc::new();
    let mut current = String::from("default");
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].to_string();
        } else if let Some((k, v)) = line.split_once('=') {
            doc.set(&current, k.trim(), v.trim());
        } else {
            return Err(format!("invalid line: {}", line));
        }
    }
    Ok(doc)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_simple() {
        let d = parse("a=1\nb=2").unwrap();
        assert_eq!(d.get("default", "a"), Some("1"));
    }
    #[test]
    fn parse_sections() {
        let d = parse("[s1]\nx=1\n[s2]\ny=2").unwrap();
        assert_eq!(d.get("s1", "x"), Some("1"));
        assert_eq!(d.get("s2", "y"), Some("2"));
    }
    #[test]
    fn skip_comments() {
        let d = parse("; c\n# h\nk=v").unwrap();
        assert_eq!(d.get("default", "k"), Some("v"));
    }
    #[test]
    fn invalid_line() {
        assert!(parse("badline").is_err());
    }
    #[test]
    fn section_count() {
        let d = parse("[a]\nx=1\n[b]\ny=2").unwrap();
        assert_eq!(d.section_count(), 2);
    }
    #[test]
    fn keys_in_section() {
        let d = parse("[s]\na=1\nb=2\nc=3").unwrap();
        assert_eq!(d.keys_in("s"), 3);
    }
}
