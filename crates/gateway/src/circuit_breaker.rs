//! Per-upstream circuit breaker (resilience pattern).
//! State machine: Closed → Open → Half-Open → Closed
//! Prevents cascading failures when a provider is down.

use std::time::{Duration, Instant};

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation; requests pass through.
    Closed,
    /// Provider is failing; fast-fail, route to fallback.
    Open,
    /// Probing recovery; allow limited requests.
    HalfOpen,
}

/// Per-upstream circuit breaker.
/// Transitions: Closed → Open (on N failures) → Half-Open (on timeout) → Closed (on success) or Open (on failure).
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: usize,
    success_count: usize,
    failure_threshold: usize,
    success_threshold: usize,
    timeout: Duration,
    last_failure_time: Option<Instant>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            failure_threshold: 5, // Open after 5 consecutive failures
            success_threshold: 2, // Close after 2 successes in Half-Open
            timeout: Duration::from_secs(60), // Wait 60s before transitioning to Half-Open
            last_failure_time: None,
        }
    }

    /// Get the current state.
    pub fn state(&self) -> CircuitState {
        // Check if we should transition from Open → Half-Open (timeout expired)
        if self.state == CircuitState::Open {
            if let Some(last_failure) = self.last_failure_time {
                if last_failure.elapsed() >= self.timeout {
                    return CircuitState::HalfOpen;
                }
            }
        }
        self.state
    }

    /// Check if the circuit is open (fail fast).
    pub fn is_open(&self) -> bool {
        self.state() == CircuitState::Open
    }

    /// Record a successful request.
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                // Success in Half-Open state; increment success count
                self.success_count += 1;
                if self.success_count >= self.success_threshold {
                    // Transition back to Closed
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitState::Open => {
                // Can't succeed if Open; ignore
            }
        }
    }

    /// Record a failed request.
    pub fn record_failure(&mut self) {
        match self.state {
            CircuitState::Closed => {
                // Increment failure count; transition to Open if threshold reached
                self.failure_count += 1;
                self.last_failure_time = Some(Instant::now());
                if self.failure_count >= self.failure_threshold {
                    self.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in Half-Open → back to Open
                self.state = CircuitState::Open;
                self.failure_count = 0;
                self.success_count = 0;
                self.last_failure_time = Some(Instant::now());
            }
            CircuitState::Open => {
                // Update timeout for retry window
                self.last_failure_time = Some(Instant::now());
            }
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starts_closed() {
        let breaker = CircuitBreaker::new();
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(!breaker.is_open());
    }

    #[test]
    fn test_closed_state_requests_pass() {
        let breaker = CircuitBreaker::new();
        assert_eq!(breaker.state(), CircuitState::Closed);
        // In Closed state, all requests pass through (is_open() == false)
    }

    #[test]
    fn test_failures_transition_to_open() {
        let mut breaker = CircuitBreaker::new();
        assert_eq!(breaker.state(), CircuitState::Closed);

        // Record 5 failures (threshold)
        for _ in 0..5 {
            breaker.record_failure();
        }

        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(breaker.is_open());
    }

    #[test]
    fn test_success_in_closed_resets_failures() {
        let mut breaker = CircuitBreaker::new();
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.failure_count, 3);

        breaker.record_success();
        assert_eq!(breaker.failure_count, 0); // Reset on success
    }

    #[test]
    fn test_open_to_half_open_transition() {
        let breaker = CircuitBreaker {
            state: CircuitState::Open,
            timeout: Duration::from_millis(1),
            last_failure_time: Some(Instant::now() - Duration::from_secs(1)),
            ..Default::default()
        };

        // After timeout, should transition to Half-Open
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_half_open_success_closes() {
        let mut breaker = CircuitBreaker {
            state: CircuitState::HalfOpen,
            success_threshold: 2,
            ..Default::default()
        };

        breaker.record_success();
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_failure_reopens() {
        let mut breaker = CircuitBreaker {
            state: CircuitState::HalfOpen,
            ..Default::default()
        };

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[test]
    fn test_failure_threshold_configurable() {
        let breaker = CircuitBreaker {
            failure_threshold: 3,
            ..Default::default()
        };
        assert_eq!(breaker.failure_threshold, 3);
    }
}
