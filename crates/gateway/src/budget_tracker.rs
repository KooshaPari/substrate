use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Budget {
    pub limit: u64,
    pub used: u64,
}

pub struct BudgetTracker {
    inner: Arc<Mutex<HashMap<String, Budget>>>,
}

impl BudgetTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn set_limit(&self, key: &str, limit: u64) {
        let mut m = self.inner.lock().unwrap();
        m.entry(key.into()).or_insert(Budget { limit, used: 0 });
    }
    pub fn consume(&self, key: &str, amount: u64) -> Result<(), String> {
        let mut m = self.inner.lock().unwrap();
        let b = m
            .get_mut(key)
            .ok_or_else(|| format!("no budget for '{}'", key))?;
        if b.used + amount > b.limit {
            return Err(format!("budget exceeded for '{}'", key));
        }
        b.used += amount;
        Ok(())
    }
    pub fn remaining(&self, key: &str) -> u64 {
        self.inner
            .lock()
            .unwrap()
            .get(key)
            .map(|b| b.limit.saturating_sub(b.used))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn set_and_consume() {
        let t = BudgetTracker::new();
        t.set_limit("a", 100);
        assert!(t.consume("a", 30).is_ok());
        assert_eq!(t.remaining("a"), 70);
    }
    #[test]
    fn consume_exceeds() {
        let t = BudgetTracker::new();
        t.set_limit("a", 10);
        assert!(t.consume("a", 20).is_err());
    }
    #[test]
    fn consume_unknown_err() {
        let t = BudgetTracker::new();
        assert!(t.consume("missing", 1).is_err());
    }
    #[test]
    fn remaining_unknown_zero() {
        assert_eq!(BudgetTracker::new().remaining("x"), 0);
    }
    #[test]
    fn multiple_consume() {
        let t = BudgetTracker::new();
        t.set_limit("a", 100);
        t.consume("a", 40).unwrap();
        t.consume("a", 40).unwrap();
        assert_eq!(t.remaining("a"), 20);
    }
}
