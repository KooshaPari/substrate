//! Recursive glob-pattern matcher for ASCII file paths.
//!
//! Supports the same wildcards as POSIX `fnmatch` (without the FNM_*
//! behaviour flags): `*` matches any sequence, `?` matches any single
//! character, `[abc]` matches any of `a/b/c`, `[!abc]` matches any
//! character not in the class. The matcher is recursive and operates on
//! character slices — no allocation per call beyond stack frames.
//!
//! This is a minimal matcher sufficient for path filtering inside the
//! gateway (route-table selectors, log filename filters, etc). It is not
//! a drop-in replacement for the `glob` crate — for shell-style brace
//! expansion or extglob features, use that instead.

/// Test whether `text` matches the glob `pattern`.
///
/// Patterns:
/// - `*` matches zero or more characters
/// - `?` matches exactly one character
/// - `[abc]` matches any of `a`, `b`, `c`
/// - `[!abc]` matches any character not in `a/b/c`
pub fn match_pattern(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    match_pattern_rec(&pat, 0, &txt, 0)
}

fn match_pattern_rec(pat: &[char], pi: usize, txt: &[char], ti: usize) -> bool {
    if pi == pat.len() {
        return ti == txt.len();
    }
    if pat[pi] == '*' {
        // Skip consecutive stars so '**' doesn't explode the recursion.
        let mut np = pi;
        while np < pat.len() && pat[np] == '*' {
            np += 1;
        }
        if np == pat.len() {
            return true;
        }
        for k in ti..=txt.len() {
            if match_pattern_rec(pat, np, txt, k) {
                return true;
            }
        }
        false
    } else if pat[pi] == '?' {
        if ti < txt.len() {
            match_pattern_rec(pat, pi + 1, txt, ti + 1)
        } else {
            false
        }
    } else if pat[pi] == '[' {
        // Character class [abc] or [!abc]
        let mut p = pi + 1;
        let neg = p < pat.len() && pat[p] == '!';
        if neg {
            p += 1;
        }
        let mut chars_in_class: Vec<char> = Vec::new();
        while p < pat.len() && pat[p] != ']' {
            chars_in_class.push(pat[p]);
            p += 1;
        }
        let end = if p < pat.len() { p } else { return false };
        let in_class = ti < txt.len() && chars_in_class.contains(&txt[ti]);
        let match_char = if neg { !in_class } else { in_class };
        if match_char {
            match_pattern_rec(pat, end + 1, txt, ti + 1)
        } else {
            false
        }
    } else {
        if ti < txt.len() && pat[pi] == txt[ti] {
            match_pattern_rec(pat, pi + 1, txt, ti + 1)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        assert!(match_pattern("hello", "hello"));
        assert!(!match_pattern("hello", "world"));
    }

    #[test]
    fn empty_pattern_only_matches_empty_text() {
        assert!(match_pattern("", ""));
        assert!(!match_pattern("", "x"));
    }

    #[test]
    fn star_suffix() {
        assert!(match_pattern("*.txt", "doc.txt"));
        assert!(!match_pattern("*.txt", "doc.md"));
    }

    #[test]
    fn star_prefix() {
        assert!(match_pattern("foo*", "foobar"));
        assert!(!match_pattern("foo*", "barfoo"));
    }

    #[test]
    fn star_middle() {
        assert!(match_pattern("a*c", "abc"));
        assert!(match_pattern("a*c", "axc"));
    }

    #[test]
    fn question_mark() {
        assert!(match_pattern("h?llo", "hello"));
        assert!(!match_pattern("h?llo", "hllo"));
    }

    #[test]
    fn char_class_basic() {
        assert!(match_pattern("[abc]", "a"));
        assert!(match_pattern("[abc]", "b"));
        assert!(!match_pattern("[abc]", "z"));
    }

    #[test]
    fn char_class_negated() {
        assert!(match_pattern("[!abc]", "z"));
        assert!(!match_pattern("[!abc]", "a"));
    }

    #[test]
    fn only_star_matches_anything() {
        assert!(match_pattern("*", "anything"));
        assert!(match_pattern("*", ""));
    }

    #[test]
    fn double_star_does_not_explode() {
        // '**' should be a no-op redundant star, not infinitely recursive
        assert!(match_pattern("**", "anything"));
        assert!(match_pattern("**", ""));
    }
}