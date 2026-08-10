use std::collections::HashMap;

#[derive(Default)]
pub struct Trie {
    children: HashMap<char, Trie>,
    end: bool,
}
impl Trie {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, word: &str) {
        let mut node = self;
        for c in word.chars() {
            node = node.children.entry(c).or_default();
        }
        node.end = true;
    }
    pub fn contains(&self, word: &str) -> bool {
        let mut node = self;
        for c in word.chars() {
            match node.children.get(&c) {
                Some(n) => node = n,
                None => return false,
            }
        }
        node.end
    }
    pub fn starts_with(&self, prefix: &str) -> bool {
        let mut node = self;
        for c in prefix.chars() {
            match node.children.get(&c) {
                Some(n) => node = n,
                None => return false,
            }
        }
        true
    }
    pub fn remove(&mut self, word: &str) -> bool {
        Self::remove_rec(self, word.chars().collect::<Vec<_>>().as_slice())
    }
    fn remove_rec(node: &mut Trie, chars: &[char]) -> bool {
        if chars.is_empty() {
            if !node.end {
                return false;
            }
            node.end = false;
            return node.children.is_empty();
        }
        let c = chars[0];
        let remove_child = {
            let child = match node.children.get_mut(&c) {
                Some(n) => n,
                None => return false,
            };
            if Self::remove_rec(child, &chars[1..]) {
                true
            } else {
                return false;
            }
        };
        if remove_child {
            node.children.remove(&c);
        }
        node.children.is_empty() && !node.end
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insert_contains() {
        let mut t = Trie::new();
        t.insert("hello");
        assert!(t.contains("hello"));
        assert!(!t.contains("hell"));
    }
    #[test]
    fn starts_with() {
        let mut t = Trie::new();
        t.insert("hello");
        t.insert("help");
        assert!(t.starts_with("hel"));
        assert!(!t.starts_with("hex"));
    }
    #[test]
    fn remove() {
        let mut t = Trie::new();
        t.insert("hello");
        assert!(t.remove("hello"));
        assert!(!t.contains("hello"));
    }
    #[test]
    fn remove_missing() {
        let mut t = Trie::new();
        t.insert("hello");
        assert!(!t.remove("help"));
        assert!(t.contains("hello"));
    }
    #[test]
    fn multiple_words() {
        let mut t = Trie::new();
        t.insert("foo");
        t.insert("bar");
        assert!(t.contains("foo"));
        assert!(t.contains("bar"));
    }
}
