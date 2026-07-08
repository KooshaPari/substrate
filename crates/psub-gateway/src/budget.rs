//! Per-session token and cost budget tracking.
//!
//! Limits are read from environment variables at `BudgetConfig` construction time:
//! - `SUBSTRATE_MAX_TOKENS_PER_SESSION` — optional `u64` token cap per session.
//! - `SUBSTRATE_MAX_COST_USD_PER_SESSION` — optional `f64` USD cap per session.
//!
//! When a session exceeds either limit, `check_budget` returns
//! [`BudgetExceeded`] which the caller should surface as an HTTP 429.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Budget limits applied to every session.  `None` means no limit.
#[derive(Debug, Clone)]
pub struct BudgetConfig {
    pub max_tokens_per_session: Option<u64>,
    pub max_cost_usd_per_session: Option<f64>,
}

impl BudgetConfig {
    /// Read limits from environment variables.
    ///
    /// Variables that are absent or unparseable are silently treated as unlimited.
    pub fn from_env() -> Self {
        let max_tokens_per_session = std::env::var("SUBSTRATE_MAX_TOKENS_PER_SESSION")
            .ok()
            .and_then(|v| v.parse::<u64>().ok());
        let max_cost_usd_per_session = std::env::var("SUBSTRATE_MAX_COST_USD_PER_SESSION")
            .ok()
            .and_then(|v| v.parse::<f64>().ok());
        Self {
            max_tokens_per_session,
            max_cost_usd_per_session,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-session state
// ---------------------------------------------------------------------------

/// Accumulated usage for a single session.
#[derive(Debug, Clone)]
pub struct SessionBudget {
    pub session_id: String,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub started_at: Instant,
}

impl SessionBudget {
    fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            tokens_used: 0,
            cost_usd: 0.0,
            started_at: Instant::now(),
        }
    }
}

/// JSON-serialisable view returned by the `/budget/:session_id` endpoint.
#[derive(Debug, Serialize)]
pub struct SessionBudgetSnapshot {
    pub session_id: String,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub elapsed_secs: f64,
}

impl From<&SessionBudget> for SessionBudgetSnapshot {
    fn from(b: &SessionBudget) -> Self {
        Self {
            session_id: b.session_id.clone(),
            tokens_used: b.tokens_used,
            cost_usd: b.cost_usd,
            elapsed_secs: b.started_at.elapsed().as_secs_f64(),
        }
    }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Thread-safe, cheaply-cloneable store keyed by `session_id`.
#[derive(Debug, Clone, Default)]
pub struct BudgetStore {
    inner: Arc<Mutex<HashMap<String, SessionBudget>>>,
}

impl BudgetStore {
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Returned by `check_budget` when a session has exceeded its allowed budget.
#[derive(Debug, Clone, PartialEq)]
pub enum BudgetExceeded {
    Tokens { used: u64, limit: u64 },
    Cost { used: f64, limit: f64 },
}

impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BudgetExceeded::Tokens { used, limit } => {
                write!(f, "token budget exceeded: {used} used, limit {limit}")
            }
            BudgetExceeded::Cost { used, limit } => {
                write!(
                    f,
                    "cost budget exceeded: ${used:.4} used, limit ${limit:.4}"
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// Check whether `session_id` has exceeded `config` limits.
///
/// If the session has never been seen it is initialised with zero usage and
/// the check always passes (a brand-new session cannot be over budget).
pub fn check_budget(
    store: &BudgetStore,
    session_id: &str,
    config: &BudgetConfig,
) -> Result<(), BudgetExceeded> {
    let map = store.inner.lock().expect("budget lock poisoned");
    let session = match map.get(session_id) {
        Some(s) => s,
        None => return Ok(()), // not yet recorded — definitely under budget
    };
    if let Some(limit) = config.max_tokens_per_session {
        if session.tokens_used >= limit {
            return Err(BudgetExceeded::Tokens {
                used: session.tokens_used,
                limit,
            });
        }
    }
    if let Some(limit) = config.max_cost_usd_per_session {
        if session.cost_usd >= limit {
            return Err(BudgetExceeded::Cost {
                used: session.cost_usd,
                limit,
            });
        }
    }
    Ok(())
}

/// Accumulate `tokens` and `cost_usd` against `session_id`, creating the entry
/// if it does not yet exist.
pub fn record_usage(store: &BudgetStore, session_id: &str, tokens: u64, cost_usd: f64) {
    let mut map = store.inner.lock().expect("budget lock poisoned");
    let session = map
        .entry(session_id.to_owned())
        .or_insert_with(|| SessionBudget::new(session_id));
    session.tokens_used = session.tokens_used.saturating_add(tokens);
    session.cost_usd += cost_usd;
}

/// Return a snapshot of the session, or `None` if it has never been seen.
pub fn get_session(store: &BudgetStore, session_id: &str) -> Option<SessionBudgetSnapshot> {
    let map = store.inner.lock().expect("budget lock poisoned");
    map.get(session_id).map(SessionBudgetSnapshot::from)
}

/// Remove all budget state for `session_id`.
///
/// Returns `true` if an entry existed and was removed, `false` if the session
/// was not found (idempotent: the session is in the "no budget" state either way).
pub fn reset_session(store: &BudgetStore, session_id: &str) -> bool {
    let mut map = store.inner.lock().expect("budget lock poisoned");
    map.remove(session_id).is_some()
}

// ---------------------------------------------------------------------------
// Cost estimation
// ---------------------------------------------------------------------------

/// Estimate cost in USD for a single request.
///
/// Rates (per 1 000 tokens):
/// - `openai` — `gpt-4*` prefix: input $0.03, output $0.06; otherwise $0.001/$0.002
/// - all others — $0.001 input, $0.001 output
pub fn estimate_cost(provider: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let (input_rate, output_rate) = match provider.to_ascii_lowercase().as_str() {
        p if p.starts_with("openai") => {
            // Distinguish GPT-4 vs cheaper models by checking the full provider
            // string (e.g. "openai/gpt-4", "openai/gpt-4o", "openai/gpt-3.5-turbo").
            if p.contains("gpt-4") {
                (0.030_f64, 0.060_f64) // $0.03/$0.06 per 1k
            } else {
                (0.001_f64, 0.002_f64)
            }
        }
        _ => (0.001_f64, 0.001_f64),
    };
    (input_tokens as f64 / 1_000.0) * input_rate + (output_tokens as f64 / 1_000.0) * output_rate
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn unlimited() -> BudgetConfig {
        BudgetConfig {
            max_tokens_per_session: None,
            max_cost_usd_per_session: None,
        }
    }

    // 1. Fresh session always passes check_budget regardless of config.
    #[test]
    fn check_budget_passes_for_unknown_session() {
        let store = BudgetStore::new();
        let config = BudgetConfig {
            max_tokens_per_session: Some(100),
            max_cost_usd_per_session: Some(0.01),
        };
        assert!(check_budget(&store, "sess-new", &config).is_ok());
    }

    // 2. Token limit exceeded after record_usage pushes session over.
    #[test]
    fn check_budget_rejects_when_tokens_exceeded() {
        let store = BudgetStore::new();
        let config = BudgetConfig {
            max_tokens_per_session: Some(500),
            max_cost_usd_per_session: None,
        };
        record_usage(&store, "sess-tok", 600, 0.0);
        let err = check_budget(&store, "sess-tok", &config).unwrap_err();
        assert!(matches!(
            err,
            BudgetExceeded::Tokens {
                used: 600,
                limit: 500
            }
        ));
    }

    // 3. Cost limit exceeded.
    #[test]
    fn check_budget_rejects_when_cost_exceeded() {
        let store = BudgetStore::new();
        let config = BudgetConfig {
            max_tokens_per_session: None,
            max_cost_usd_per_session: Some(1.0),
        };
        record_usage(&store, "sess-cost", 0, 1.5);
        let err = check_budget(&store, "sess-cost", &config).unwrap_err();
        assert!(matches!(err, BudgetExceeded::Cost { .. }));
    }

    // 4. record_usage accumulates across multiple calls.
    #[test]
    fn record_usage_accumulates() {
        let store = BudgetStore::new();
        record_usage(&store, "sess-acc", 100, 0.10);
        record_usage(&store, "sess-acc", 200, 0.20);
        let snap = get_session(&store, "sess-acc").unwrap();
        assert_eq!(snap.tokens_used, 300);
        assert!((snap.cost_usd - 0.30).abs() < 1e-9);
    }

    // 5. estimate_cost — GPT-4 rates.
    #[test]
    fn estimate_cost_openai_gpt4() {
        // 1000 input @ $0.03/k + 500 output @ $0.06/k = $0.03 + $0.03 = $0.06
        let cost = estimate_cost("openai/gpt-4", 1_000, 500);
        assert!((cost - 0.06).abs() < 1e-9, "cost={cost}");
    }

    // 6. estimate_cost — unknown provider uses default rate.
    #[test]
    fn estimate_cost_default_provider() {
        // 2000 input @ $0.001/k + 1000 output @ $0.001/k = $0.002 + $0.001 = $0.003
        let cost = estimate_cost("anthropic/claude-3", 2_000, 1_000);
        assert!((cost - 0.003).abs() < 1e-9, "cost={cost}");
    }

    // 7. get_session returns None for an unknown session.
    #[test]
    fn get_session_none_for_unknown() {
        let store = BudgetStore::new();
        assert!(get_session(&store, "no-such-session").is_none());
    }

    // 8. Unlimited config never blocks regardless of usage.
    #[test]
    fn check_budget_passes_with_unlimited_config() {
        let store = BudgetStore::new();
        record_usage(&store, "sess-unl", 1_000_000, 9999.0);
        assert!(check_budget(&store, "sess-unl", &unlimited()).is_ok());
    }
}
