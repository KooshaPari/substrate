use std::time::{Duration, Instant};

#[derive(Debug,PartialEq)]
pub enum CbState { Closed, Open, HalfOpen }

pub struct CircuitBreaker {
    failure_threshold: u32,
    cooldown: Duration,
    failures: u32,
    state: CbState,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, cooldown: Duration) -> Self {
        Self { failure_threshold, cooldown, failures: 0, state: CbState::Closed, opened_at: None }
    }
    pub fn record_success(&mut self) {
        self.failures = 0;
        self.state = CbState::Closed;
        self.opened_at = None;
    }
    pub fn record_failure(&mut self) {
        self.failures += 1;
        if self.failures >= self.failure_threshold {
            self.state = CbState::Open;
            self.opened_at = Some(Instant::now());
        }
    }
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CbState::Closed => true,
            CbState::Open => {
                if let Some(t) = self.opened_at {
                    if t.elapsed() >= self.cooldown {
                        self.state = CbState::HalfOpen;
                        true
                    } else { false }
                } else { false }
            }
            CbState::HalfOpen => true,
        }
    }
    pub fn state(&self) -> CbState { self.state.clone_if_cloneable_or_default() }
}

trait CbStateClone { fn clone_if_cloneable_or_default(&self) -> CbState; }
impl CbStateClone for CbState {
    fn clone_if_cloneable_or_default(&self) -> CbState { match self {
        CbState::Closed => CbState::Closed,
        CbState::Open => CbState::Open,
        CbState::HalfOpen => CbState::HalfOpen,
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn closed_initially() { let cb = CircuitBreaker::new(3, Duration::from_secs(60)); assert_eq!(cb.state(), CbState::Closed); }
    #[test] fn opens_after_threshold() { let mut cb = CircuitBreaker::new(2, Duration::from_secs(60)); cb.record_failure(); cb.record_failure(); assert_eq!(cb.state(), CbState::Open); }
    #[test] fn blocks_when_open() { let mut cb = CircuitBreaker::new(1, Duration::from_secs(60)); cb.record_failure(); assert!(!cb.can_execute()); }
    #[test] fn success_resets() { let mut cb = CircuitBreaker::new(3, Duration::from_secs(60)); cb.record_failure(); cb.record_success(); assert_eq!(cb.failures, 0); }
}
