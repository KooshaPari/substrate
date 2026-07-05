pub fn count_chars(s: &str) -> usize { s.chars().count() }
pub fn count_bytes(s: &str) -> usize { s.len() }
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    s.chars().take(max).collect()
}
pub fn is_ascii(s: &str) -> bool { s.is_ascii() }
pub fn strip_prefix_case(s: &str, prefix: &str) -> Option<&str> {
    if s.len() < prefix.len() { return None; }
    let (s_head, p_head) = s.split_at(prefix.len());
    if s_head.eq_ignore_ascii_case(p_head) { Some(&s[prefix.len()..]) } else { None }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn chars_bytes() { assert_eq!(count_chars("hello"), 5); assert_eq!(count_bytes("hello"), 5); assert_eq!(count_chars("héllo"), 5); assert_eq!(count_bytes("héllo"), 6); }
    #[test] fn truncate() { assert_eq!(truncate_chars("hello world", 5), "hello"); }
    #[test] fn truncate_short() { assert_eq!(truncate_chars("hi", 10), "hi"); }
    #[test] fn ascii_check() { assert!(is_ascii("hello")); assert!(!is_ascii("héllo")); }
    #[test] fn prefix_case_insensitive() { assert_eq!(strip_prefix_case("Hello-World", "hello"), Some("-World")); assert_eq!(strip_prefix_case("HELLO", "hello"), Some("")); }
    #[test] fn prefix_mismatch() { assert_eq!(strip_prefix_case("world", "hello"), None); }
}
