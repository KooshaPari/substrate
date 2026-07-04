//! Provider routing table for the gateway.
//!
//! [`ProviderRouter`] selects an upstream provider URL via a round-robin
//! cursor over the set of providers currently marked `healthy`. Unhealthy
//! providers are skipped during selection but can be recovered via
//! [`ProviderRouter::mark_healthy`]. The cursor is shared across clones
//! (`Arc<AtomicUsize>`) so multiple callers see a single rotation.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ProviderEntry {
    pub name: String,
    pub url: String,
    pub healthy: bool,
}

pub struct ProviderRouter {
    providers: Vec<ProviderEntry>,
    cursor: Arc<AtomicUsize>,
}

impl ProviderRouter {
    pub fn new(providers: Vec<ProviderEntry>) -> Self {
        Self { providers, cursor: Arc::new(AtomicUsize::new(0)) }
    }

    pub fn next(&self) -> Option<&ProviderEntry> {
        let healthy: Vec<&ProviderEntry> = self.providers.iter().filter(|p| p.healthy).collect();
        if healthy.is_empty() { return None; }
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % healthy.len();
        Some(healthy[idx])
    }

    pub fn mark_unhealthy(&mut self, name: &str) {
        if let Some(p) = self.providers.iter_mut().find(|p| p.name == name) {
            p.healthy = false;
        }
    }

    pub fn mark_healthy(&mut self, name: &str) {
        if let Some(p) = self.providers.iter_mut().find(|p| p.name == name) {
            p.healthy = true;
        }
    }

    pub fn healthy_count(&self) -> usize {
        self.providers.iter().filter(|p| p.healthy).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> ProviderRouter {
        ProviderRouter::new(vec![
            ProviderEntry { name: "a".into(), url: "http://a".into(), healthy: true },
            ProviderEntry { name: "b".into(), url: "http://b".into(), healthy: true },
        ])
    }

    #[test]
    fn next_round_robins() {
        let r = router();
        let first = r.next().unwrap().name.clone();
        let second = r.next().unwrap().name.clone();
        assert_ne!(first, second);
    }
    #[test]
    fn next_skips_unhealthy() {
        let mut r = router();
        r.mark_unhealthy("a");
        for _ in 0..4 { assert_eq!(r.next().unwrap().name, "b"); }
    }
    #[test]
    fn next_none_when_all_unhealthy() {
        let mut r = router();
        r.mark_unhealthy("a"); r.mark_unhealthy("b");
        assert!(r.next().is_none());
    }
    #[test]
    fn mark_healthy_recovers() {
        let mut r = router();
        r.mark_unhealthy("a"); r.mark_healthy("a");
        assert_eq!(r.healthy_count(), 2);
    }
    #[test]
    fn healthy_count() { assert_eq!(router().healthy_count(), 2); }
}