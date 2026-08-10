use std::time::{Duration, Instant};

pub struct ConnChecker {
    max_age: Duration,
    last_ok: Option<Instant>,
}
impl ConnChecker {
    pub fn new(max_age: Duration) -> Self {
        Self {
            max_age,
            last_ok: None,
        }
    }
    pub fn mark_ok(&mut self) {
        self.last_ok = Some(Instant::now());
    }
    pub fn mark_fail(&mut self) {
        self.last_ok = None;
    }
    pub fn is_healthy(&self) -> bool {
        match self.last_ok {
            Some(t) => t.elapsed() <= self.max_age,
            None => false,
        }
    }
    pub fn age(&self) -> Option<Duration> {
        self.last_ok.map(|t| t.elapsed())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh() {
        let mut c = ConnChecker::new(Duration::from_secs(60));
        c.mark_ok();
        assert!(c.is_healthy());
    }
    #[test]
    fn never_marked() {
        let c = ConnChecker::new(Duration::from_secs(60));
        assert!(!c.is_healthy());
    }
    #[test]
    fn failed() {
        let mut c = ConnChecker::new(Duration::from_secs(60));
        c.mark_ok();
        c.mark_fail();
        assert!(!c.is_healthy());
    }
    #[test]
    fn expired() {
        let mut c = ConnChecker::new(Duration::from_millis(50));
        c.mark_ok();
        std::thread::sleep(Duration::from_millis(60));
        assert!(!c.is_healthy());
    }
    #[test]
    fn age_some() {
        let mut c = ConnChecker::new(Duration::from_secs(60));
        c.mark_ok();
        assert!(c.age().is_some());
    }
}
