//! Myers-style text diff (longest-common-subsequence based).
//!
//! Produces a shortest edit script (SES) converting one string into another
//! using only insertions and deletions. The output is a sequence of
//! [`Diff`] values describing runs that are equal, inserted, or deleted.
//!
//! Implementation: classic O(N·M) LCS dynamic programming. Suitable for
//! short to medium inputs (a few thousand characters). For larger inputs
//! prefer Myers' O(ND) algorithm or the `similar` crate.
//!
//! Reference: <https://en.wikipedia.org/wiki/Longest_common_subsequence_problem>

/// A single diff operation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOp {
    /// The text is present in both the old and new strings at this position.
    Equal,
    /// The text was added in the new string.
    Insert,
    /// The text was removed from the old string.
    Delete,
}

/// One element of the diff: which operation and the affected text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub op: DiffOp,
    pub text: String,
}

/// Compute the diff between `a` (old) and `b` (new) using LCS dynamic
/// programming. Returns a sequence of [`Diff`] elements that, when applied,
/// reconstructs both strings.
pub fn diff(a: &str, b: &str) -> Vec<Diff> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();

    // dp[i][j] = LCS length of a[..i] and b[..j].
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    // Backtrack to recover the diff. Walk from (n, m) to (0, 0).
    let mut ops_rev: Vec<(DiffOp, char)> = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            ops_rev.push((DiffOp::Equal, a[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops_rev.push((DiffOp::Insert, b[j - 1]));
            j -= 1;
        } else {
            ops_rev.push((DiffOp::Delete, a[i - 1]));
            i -= 1;
        }
    }
    ops_rev.reverse();

    // Coalesce adjacent runs of the same operation into single Diff entries.
    let mut result: Vec<Diff> = Vec::new();
    for (op, ch) in ops_rev {
        if let Some(last) = result.last_mut() {
            if last.op == op {
                last.text.push(ch);
                continue;
            }
        }
        result.push(Diff {
            op,
            text: ch.to_string(),
        });
    }
    result
}

/// Render a diff as a human-readable unified string with `+`/`-`/` ` prefixes
/// per character (newlines pass through unchanged).
pub fn to_unified(diffs: &[Diff]) -> String {
    let mut s = String::new();
    for d in diffs {
        match d.op {
            DiffOp::Equal => {
                for ch in d.text.chars() {
                    if ch == '\n' {
                        s.push('\n');
                    } else {
                        s.push(' ');
                    }
                }
            }
            DiffOp::Insert => {
                for ch in d.text.chars() {
                    s.push('+');
                    if ch == '\n' {
                        s.push('\n');
                    } else {
                        s.push(ch);
                    }
                }
            }
            DiffOp::Delete => {
                for ch in d.text.chars() {
                    s.push('-');
                    if ch == '\n' {
                        s.push('\n');
                    } else {
                        s.push(ch);
                    }
                }
            }
        }
    }
    s
}

/// Count the number of inserted characters in a diff.
pub fn insertions(diffs: &[Diff]) -> usize {
    diffs
        .iter()
        .filter(|d| d.op == DiffOp::Insert)
        .map(|d| d.text.chars().count())
        .sum()
}

/// Count the number of deleted characters in a diff.
pub fn deletions(diffs: &[Diff]) -> usize {
    diffs
        .iter()
        .filter(|d| d.op == DiffOp::Delete)
        .map(|d| d.text.chars().count())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reconstruct(diffs: &[Diff]) -> (String, String) {
        let mut old = String::new();
        let mut new = String::new();
        for d in diffs {
            match d.op {
                DiffOp::Equal => {
                    old.push_str(&d.text);
                    new.push_str(&d.text);
                }
                DiffOp::Delete => old.push_str(&d.text),
                DiffOp::Insert => new.push_str(&d.text),
            }
        }
        (old, new)
    }

    #[test]
    fn identical_strings() {
        let d = diff("hello", "hello");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].op, DiffOp::Equal);
        assert_eq!(d[0].text, "hello");
    }

    #[test]
    fn pure_insertion() {
        let d = diff("", "abc");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].op, DiffOp::Insert);
        assert_eq!(d[0].text, "abc");
    }

    #[test]
    fn pure_deletion() {
        let d = diff("abc", "");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].op, DiffOp::Delete);
        assert_eq!(d[0].text, "abc");
    }

    #[test]
    fn round_trip_via_application() {
        let original = "The quick brown fox jumps over the lazy dog.";
        let modified = "The quick brown wolf leaps over the very lazy dog.";
        let d = diff(original, modified);
        let (old, new) = reconstruct(&d);
        assert_eq!(old, original);
        assert_eq!(new, modified);
    }

    #[test]
    fn coalesces_adjacent_runs() {
        // A fully different pair should collapse to one Delete + one Insert.
        let d = diff("abc", "xyz");
        let mut deletes = 0;
        let mut inserts = 0;
        for entry in &d {
            if entry.op == DiffOp::Delete {
                assert_eq!(entry.text, "abc");
                deletes += 1;
            } else if entry.op == DiffOp::Insert {
                assert_eq!(entry.text, "xyz");
                inserts += 1;
            }
        }
        assert_eq!(deletes, 1);
        assert_eq!(inserts, 1);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn single_substitution_round_trip() {
        let d = diff("cat", "cut");
        let (old, new) = reconstruct(&d);
        assert_eq!(old, "cat");
        assert_eq!(new, "cut");
    }

    #[test]
    fn insertion_at_start() {
        let d = diff("world", "hello world");
        let (old, new) = reconstruct(&d);
        assert_eq!(old, "world");
        assert_eq!(new, "hello world");
    }

    #[test]
    fn deletion_at_end() {
        let d = diff("hello world", "hello");
        let (old, new) = reconstruct(&d);
        assert_eq!(old, "hello world");
        assert_eq!(new, "hello");
    }

    #[test]
    fn insertions_and_deletions_count() {
        let d = diff("cat", "cut");
        // 'a' deleted, 'u' inserted; the LCS path yields 1 insertion + 1 deletion.
        assert_eq!(insertions(&d), 1);
        assert_eq!(deletions(&d), 1);
    }

    #[test]
    fn unified_format_prefixes() {
        let d = diff("cat", "cut");
        let u = to_unified(&d);
        for line in u.lines() {
            assert!(
                line.starts_with(' ') || line.starts_with('+') || line.starts_with('-'),
                "line missing prefix: {:?}",
                line
            );
        }
    }

    #[test]
    fn unicode_round_trip() {
        let old = "café résumé";
        let new = "cafés résumé";
        let d = diff(old, new);
        let (o, n) = reconstruct(&d);
        assert_eq!(o, old);
        assert_eq!(n, new);
    }
}
