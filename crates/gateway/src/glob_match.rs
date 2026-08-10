pub fn matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    match_recursive(&p, &n)
}
fn match_recursive(p: &[char], n: &[char]) -> bool {
    if p.is_empty() {
        return n.is_empty();
    }
    match p[0] {
        '*' => {
            if p.len() == 1 {
                return true;
            }
            for i in 0..=n.len() {
                if match_recursive(&p[1..], &n[i..]) {
                    return true;
                }
            }
            false
        }
        '?' => !n.is_empty() && match_recursive(&p[1..], &n[1..]),
        c => !n.is_empty() && c == n[0] && match_recursive(&p[1..], &n[1..]),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal() {
        assert!(matches("abc", "abc"));
        assert!(!matches("abc", "abd"));
    }
    #[test]
    fn star() {
        assert!(matches("a*", "abc"));
        assert!(matches("*", "anything"));
        assert!(matches("*x", "abcx"));
    }
    #[test]
    fn question() {
        assert!(matches("a?c", "abc"));
        assert!(!matches("a?c", "ac"));
    }
    #[test]
    fn empty() {
        assert!(matches("", ""));
        assert!(!matches("", "x"));
        assert!(!matches("x", ""));
    }
    #[test]
    fn combined() {
        assert!(matches("a*b?c", "axxxbyc"));
        assert!(!matches("a*b?c", "axxxb"));
    }
}
