//! Typed state-machine circuit breaker (v2).
//!
//! Three-state breaker with lazy recovery: callers must invoke `state()` or
//! `should_allow()` to refresh an `Open` whose `reset_timeout_ms` has elapsed.
//! On a probe success the breaker closes; on probe failure it re-opens with a
//! fresh reset window.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout_ms: u64,
    state: State,
    failures: u32,
    opened_at_ms: u64,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout_ms: u64) -> Self {
        Self {
            failure_threshold,
            reset_timeout_ms,
            state: State::Closed,
            failures: 0,
            opened_at_ms: 0,
        }
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.state = State::Closed;
        self.opened_at_ms = 0;
    }

    pub fn record_failure(&mut self) {
        // If we're in HalfOpen, a single failure trips us back to Open.
        if self.state == State::HalfOpen {
            self.state = State::Open;
            self.opened_at_ms = current_ms();
            self.failures = self.failure_threshold;
            return;
        }
        self.failures = self.failures.saturating_add(1);
        if self.failures >= self.failure_threshold {
            self.state = State::Open;
            self.opened_at_ms = current_ms();
        }
    }

    pub fn state(&mut self) -> State {
        self.refresh();
        self.state
    }

    pub fn should_allow(&mut self) -> bool {
        self.refresh();
        match self.state {
            State::Closed | State::HalfOpen => true,
            State::Open => false,
        }
    }

    fn refresh(&mut self) {
        if self.state == State::Open {
            let now = current_ms();
            if now.saturating_sub(self.opened_at_ms) >= self.reset_timeout_ms {
                self.state = State::HalfOpen;
            }
        }
    }
}

fn current_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_to_open_after_threshold() {
        let mut cb = CircuitBreaker::new(3, 1000);
        assert_eq!(cb.state(), State::Closed);
        assert!(cb.should_allow());
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), State::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), State::Open);
        assert!(!cb.should_allow());
    }

    #[test]
    fn half_open_after_timeout() {
        let mut cb = CircuitBreaker::new(1, 10);
        cb.record_failure();
        assert_eq!(cb.state(), State::Open);
        std::thread::sleep(std::time::Duration::from_millis(15));
        assert_eq!(cb.state(), State::HalfOpen);
        assert!(cb.should_allow());
    }

    #[test]
    fn half_open_success_closes() {
        let mut cb = CircuitBreaker::new(1, 5);
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(cb.state(), State::HalfOpen);
        cb.record_success();
        assert_eq!(cb.state(), State::Closed);
        assert_eq!(cb.failures, 0);
    }

    #[test]
    fn half_open_failure_reopens() {
        let mut cb = CircuitBreaker::new(1, 5);
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(cb.state(), State::HalfOpen);
        cb.record_failure();
        assert_eq!(cb.state(), State::Open);
    }

    #[test]
    fn success_resets_failure_count() {
        let mut cb = CircuitBreaker::new(5, 1000);
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.failures, 0);
        assert_eq!(cb.state(), State::Closed);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), State::Closed);
    }
}