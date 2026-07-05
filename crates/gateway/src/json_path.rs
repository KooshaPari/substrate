pub fn find_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{}\":", key);
    if let Some(pos) = json.find(&needle) {
        let rest = json[pos + needle.len()..].trim_start();
        if rest.starts_with('"') {
            if let Some(end) = rest[1..].find('"') { return Some(&rest[1..1+end]); }
        } else {
            for (i, c) in rest.char_indices() {
                if c == ',' || c == '}' || c == ']' { return Some(rest[..i].trim()); }
            }
            return Some(rest.trim());
        }
    }
    None
}

pub fn count_keys(json: &str, key: &str) -> usize {
    let needle = format!("\"{}\":", key);
    let mut count = 0; let mut idx = 0;
    while let Some(pos) = json[idx..].find(&needle) {
        count += 1; idx += pos + needle.len();
    }
    count
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn string_value() { assert_eq!(find_value(r#"{"name":"alice"}"#, "name"), Some("alice")); }
    #[test] fn number_value() { assert_eq!(find_value(r#"{"age":30}"#, "age"), Some("30")); }
    #[test] fn bool_value() { assert_eq!(find_value(r#"{"active":true}"#, "active"), Some("true")); }
    #[test] fn missing() { assert_eq!(find_value(r#"{"name":"alice"}"#, "age"), None); }
    #[test] fn count_multiple() { assert_eq!(count_keys(r#"{"id":1,"name":"a","id":2}"#, "id"), 2); }
}
