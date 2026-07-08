use std::collections::HashSet;

#[derive(Debug,Clone)]
pub struct FlagSet { flags: HashSet<String> }
impl FlagSet {
    pub fn new() -> Self { Self { flags: HashSet::new() } }
    pub fn set(&mut self, name: impl Into<String>) -> bool { self.flags.insert(name.into()) }
    pub fn unset(&mut self, name: &str) -> bool { self.flags.remove(name) }
    pub fn has(&self, name: &str) -> bool { self.flags.contains(name) }
    pub fn count(&self) -> usize { self.flags.len() }
    pub fn is_empty(&self) -> bool { self.flags.is_empty() }
    pub fn clear(&mut self) { self.flags.clear(); }
    pub fn iter(&self) -> impl Iterator<Item = &str> { self.flags.iter().map(|s| s.as_str()) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_new_empty() { let f = FlagSet::new(); assert!(f.is_empty()); }
    #[test] fn test_set_get() { let mut f = FlagSet::new(); assert!(f.set("a")); assert!(f.has("a")); }
    #[test] fn test_set_idempotent() { let mut f = FlagSet::new(); f.set("a"); assert!(!f.set("a")); }
    #[test] fn test_unset() { let mut f = FlagSet::new(); f.set("a"); assert!(f.unset("a")); assert!(!f.unset("a")); }
    #[test] fn test_clear() { let mut f = FlagSet::new(); f.set("a"); f.set("b"); f.clear(); assert!(f.is_empty()); }
    #[test] fn test_count() { let mut f = FlagSet::new(); f.set("a"); f.set("b"); f.set("c"); assert_eq!(f.count(), 3); }
}
