//! Aho-Corasick multi-pattern string matching (Aho & Corasick, 1975).
//!
//! Builds a goto + failure automaton from a list of literal patterns and
//! reports every occurrence (byte offset) in a haystack. Patterns are
//! treated as byte strings (UTF-8 unspecified). The implementation uses
//! the deterministic finite-automaton formulation with O(n + m + z)
//! complexity where n = haystack length, m = total pattern length, and
//! z = number of output matches.
//!
//! Reference: Alfred V. Aho, Margaret J. Corasick, "Efficient String
//! Matching: An Aid to Bibliographic Search", Communications of the ACM,
//! 18(6):333-340, June 1975.

/// A single match: which pattern was found (0-indexed slot) and at which
/// byte offset in the haystack it ends. We report the end index so that
/// callers can recover the start as `end - patterns[id].len()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// Index into the original pattern vector.
    pub pattern_id: usize,
    /// Byte offset just past the final byte of the match (exclusive end).
    pub end: usize,
}

/// Internal goto state. Failure links are stored separately to keep the
/// automaton compact: most states live in a flat vector indexed by `state`.
#[derive(Debug, Default)]
struct Node {
    /// `goto[s][b]` is the next state when in state `s` reading byte `b`.
    /// Stored as `Vec<(u8, u32)>` to limit memory when alphabets are small.
    goto: Vec<(u8, u32)>,
    /// Failure link (state to fall back to on a mismatch).
    fail: u32,
    /// The pattern ids that end at this state, or empty for non-terminal.
    output: Vec<usize>,
}

/// Pre-built Aho-Corasick automaton.
#[derive(Debug)]
pub struct AcAutomaton {
    nodes: Vec<Node>,
    patterns: Vec<Vec<u8>>,
}

impl AcAutomaton {
    /// Build an automaton from a slice of byte-string patterns.
    pub fn new(patterns: &[&[u8]]) -> Self {
        let mut nodes: Vec<Node> = Vec::new();
        nodes.push(Node::default()); // root at index 0

        let mut patterns_owned: Vec<Vec<u8>> = patterns.iter().map(|p| p.to_vec()).collect();

        // 1. Build the goto graph (a trie).
        for (id, pat) in patterns_owned.iter().enumerate() {
            let mut state = 0u32;
            for &b in pat {
                let next: Option<u32> = nodes[state as usize]
                    .goto
                    .iter()
                    .find(|(k, _)| *k == b)
                    .map(|(_, v)| *v);
                state = match next {
                    Some(s) => s,
                    None => {
                        let s = nodes.len() as u32;
                        nodes.push(Node::default());
                        nodes[state as usize].goto.push((b, s));
                        s
                    }
                };
            }
            nodes[state as usize].output.push(id);
        }

        // 2. Compute failure links by BFS over the trie.
        let mut queue: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
        // Initialize: children of the root fail back to root.
        let root_children: Vec<(u8, u32)> = nodes[0].goto.iter().copied().collect();
        for &(_, s) in root_children.iter() {
            nodes[s as usize].fail = 0;
            queue.push_back(s);
        }
        while let Some(u) = queue.pop_front() {
            // Pull out the (byte, child) pairs at node u before any mutation
            // of `nodes` so we can iterate without a borrow conflict.
            let pairs: Vec<(u8, u32)> = nodes[u as usize].goto.iter().copied().collect();
            let parent_fail = nodes[u as usize].fail;
            for (b, v) in pairs {
                // Compute failure for child v via parent u.
                let mut f = parent_fail;
                let mut child_fail: u32 = 0;
                loop {
                    let ns = nodes[f as usize].goto.iter().find(|(k, _)| *k == b).map(|(_, s)| *s);
                    if let Some(s) = ns {
                        child_fail = s;
                        break;
                    }
                    if f == 0 {
                        child_fail = 0;
                        break;
                    }
                    f = nodes[f as usize].fail;
                }
                nodes[v as usize].fail = child_fail;
                // Union outputs: any patterns ending at fail-state also end at v.
                let fs = child_fail as usize;
                let fs_outputs: Vec<usize> = nodes[fs].output.iter().copied().collect();
                for pid in fs_outputs {
                    if !nodes[v as usize].output.contains(&pid) {
                        nodes[v as usize].output.push(pid);
                    }
                }
                queue.push_back(v);
            }
        }

        Self {
            nodes,
            patterns: patterns_owned,
        }
    }

    /// Run the automaton over `haystack`; returns every match in left-to-right
    /// order. Each match reports the exclusive byte offset and pattern id.
    pub fn find(&self, haystack: &[u8]) -> Vec<Match> {
        let mut out = Vec::new();
        let mut state = 0u32;
        for (i, &b) in haystack.iter().enumerate() {
            loop {
                let next: Option<u32> = self.nodes[state as usize]
                    .goto
                    .iter()
                    .find(|(k, _)| *k == b)
                    .map(|(_, v)| *v);
                if let Some(s) = next {
                    state = s;
                    break;
                }
                if state == 0 {
                    break;
                }
                state = self.nodes[state as usize].fail;
            }
            let outputs: Vec<usize> = self.nodes[state as usize].output.iter().copied().collect();
            for pid in outputs {
                out.push(Match {
                    pattern_id: pid,
                    end: i + 1,
                });
            }
        }
        out
    }

    /// Read-only access to the pattern bytes (after cloning).
    pub fn pattern(&self, id: usize) -> &[u8] {
        &self.patterns[id]
    }

    /// Number of distinct patterns registered.
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_pattern_single_match() {
        let ac = AcAutomaton::new(&[&b"world"[..]]);
        let m = ac.find(b"hello world");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].pattern_id, 0);
        assert_eq!(m[0].end, 11);
        assert_eq!(&b"hello world"[m[0].end - 5..m[0].end], b"world");
    }

    #[test]
    fn no_match() {
        let ac = AcAutomaton::new(&[&b"abc"[..]]);
        assert!(ac.find(b"xyzxyz").is_empty());
    }

    #[test]
    fn overlapping_patterns() {
        // "aba" and "bab" both occur within "ababab".
        let ac = AcAutomaton::new(&[&b"aba"[..], &b"bab"[..]]);
        let m = ac.find(b"ababab");
        // aba at end=3, bab at end=4, aba at end=5, bab at end=6.
        let ends: Vec<usize> = m.iter().map(|mm| mm.end).collect();
        assert!(ends.contains(&3));
        assert!(ends.contains(&4));
        assert!(ends.contains(&5));
        assert!(ends.contains(&6));
    }

    #[test]
    fn failure_link_share_outputs() {
        // "he" and "she" -> "he" must appear whenever we are inside "she".
        let ac = AcAutomaton::new(&[&b"he"[..], &b"she"[..]]);
        let m = ac.find(b"shesellsseashells");
        // "she" at indices 2, 5 ... and "he" via failure link at 3, 6 ...
        let has_she = m.iter().any(|x| x.pattern_id == 1 && x.end == 3);
        let has_he_after_she = m.iter().any(|x| x.pattern_id == 0 && x.end == 3);
        assert!(has_she && has_he_after_she);
    }

    #[test]
    fn empty_haystack_yields_nothing() {
        let ac = AcAutomaton::new(&[&b"foo"[..]]);
        assert!(ac.find(b"").is_empty());
    }

    #[test]
    fn pattern_count_roundtrip() {
        let ac = AcAutomaton::new(&[&b"alpha"[..], &b"beta"[..], &b"gamma"[..]]);
        assert_eq!(ac.pattern_count(), 3);
    }

    #[test]
    fn multiple_disjoint_matches() {
        let ac = AcAutomaton::new(&[&b"foo"[..], &b"bar"[..]]);
        let m = ac.find(b"foo and bar");
        // foo at end=3, bar at end=11.
        let mut ends: Vec<(usize, usize)> = m.iter().map(|x| (x.pattern_id, x.end)).collect();
        ends.sort();
        assert_eq!(ends, vec![(0, 3), (1, 11)]);
    }

    #[test]
    fn single_byte_pattern() {
        let ac = AcAutomaton::new(&[&b"x"[..]]);
        let m = ac.find(b"axbxcx");
        assert_eq!(m.len(), 3);
        assert_eq!(m.iter().map(|x| x.end).collect::<Vec<_>>(), vec![2, 4, 6]);
    }
}
