pub fn matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    match_recursive(&p, &t, 0, 0)
}
fn match_recursive(p: &[char], t: &[char], mut pi: usize, mut ti: usize) -> bool {
    while pi < p.len() {
        let c = p[pi];
        let next = if pi + 1 < p.len() { Some(p[pi + 1]) } else { None };
        match c {
            '*' => {
                while pi + 1 < p.len() && p[pi + 1] == '*' { pi += 1; }
                let rest = &p[pi + 1..];
                for i in ti..=t.len() { if match_recursive(rest, t, 0, i) { return true; } }
                return false;
            }
            '.' => { if ti >= t.len() { return false; } pi += 1; ti += 1; }
            c if next == Some('*') => {
                pi += 2;
                for i in ti..=t.len() { if match_recursive(p, t, pi, i) { return true; } }
                return false;
            }
            c => { if ti >= t.len() || t[ti] != *c { return false; } pi += 1; ti += 1; }
        }
    }
    ti == t.len()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn literal() { assert!(matches("abc", "abc")); assert!(!matches("abc", "abd")); }
    #[test] fn star() { assert!(matches("a*", "aaa")); assert!(matches("a*", "")); }
    #[test] fn dot() { assert!(matches("a.c", "abc")); assert!(!matches("a.c", "ac")); }
    #[test] fn char_star() { assert!(matches("a*c", "abc")); assert!(matches("a*c", "c")); }
    #[test] fn empty() { assert!(matches("", "")); assert!(!matches("", "x")); }
}
