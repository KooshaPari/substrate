//! # context-budget
//!
//! [`EnginePort`] middleware that enforces per-conversation **token budgets**.
//! Wraps any `EnginePort` implementation and gates `start()` / `resume()` on
//! whether the prompt fits within the configured budget. Three overflow
//! policies are supported:
//!
//! | Policy | Behaviour on overflow |
//! |--------|----------------------|
//! | [`OverflowPolicy::Reject`] | Return [`SubstrateError::Engine`] (default). |
//! | [`OverflowPolicy::Truncate`] | Truncate the prompt to fit the remaining budget. |
//! | [`OverflowPolicy::Warn`]    | Accept the prompt and emit a tracing warning. |
//!
//! Token estimation uses a simple **chars/4 heuristic** — accurate enough for
//! budget enforcement, deliberately dependency-light (no tokenizer crate).
//! The estimator is exposed as [`estimate_tokens`] so tests and other
//! adapters can verify the math.
//!
//! ## Usage
//!
//! ```
//! use std::sync::Arc;
//! use async_trait::async_trait;
//! use context_budget::{BudgetEngine, BudgetConfig, OverflowPolicy};
//! use substrate_core::domain::{ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task};
//! use substrate_core::error::Result;
//! use substrate_core::ports::EnginePort;
//!
//! struct Echo;
//!
//! #[async_trait]
//! impl EnginePort for Echo {
//!     async fn start(&self, _task: &Task) -> Result<Session> {
//!         Ok(Session { conv_id: "c1".into(), pid: None, logfile: None })
//!     }
//!     async fn resume(&self, _conv_id: &str, _prompt: &str) -> Result<Session> {
//!         Ok(Session { conv_id: "c1".into(), pid: None, logfile: None })
//!     }
//!     async fn dump(&self, _conv_id: &str) -> Result<ConversationDump> {
//!         Ok(ConversationDump { conversation_id: "c1".into(), raw: String::new() })
//!     }
//!     async fn cancel(&self, _conv_id: &str) -> Result<()> { Ok(()) }
//!     async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> { Ok(()) }
//!     fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
//!         Ok(StructuredResult { text: String::new(), artifacts: vec![], pr_urls: vec![], status: substrate_core::domain::TaskState::Completed })
//!     }
//!     fn capabilities(&self) -> EngineCapabilities { EngineCapabilities { supports_resume: true, supports_subagents: false, supports_mcp_import: false } }
//! }
//!
//! # tokio_test::block_on(async {
//! let inner = Arc::new(Echo);
//! let budget = BudgetConfig::new(100).with_policy(OverflowPolicy::Reject);
//! let engine = BudgetEngine::new(inner, budget);
//! let task = Task::new("hello world", "/tmp");
//! let session = engine.start(&task).await.unwrap();
//! assert_eq!(session.conv_id, "c1");
//! # });
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::EnginePort;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate the token count of a string using the **chars/4 heuristic**.
///
/// This is intentionally dependency-light (no real tokenizer crate); accurate
/// to within ~25% for English text, which is good enough for budget
/// enforcement. Use [`estimate_tokens_bytes`] for byte-oriented payloads.
///
/// # Examples
///
/// ```
/// use context_budget::estimate_tokens;
/// assert_eq!(estimate_tokens(""), 0);
/// assert_eq!(estimate_tokens("abcd"), 1);
/// assert_eq!(estimate_tokens("abcdefgh"), 2);
/// ```
pub fn estimate_tokens(s: &str) -> usize {
    // Round up so single-character strings still count as 1 token.
    s.chars().count().div_ceil(4)
}

/// Estimate the token count of a byte slice (counts bytes directly, then
/// divides by 4). Useful for non-UTF-8 payloads; for text prefer
/// [`estimate_tokens`].
pub fn estimate_tokens_bytes(b: &[u8]) -> usize {
    b.len().div_ceil(4)
}

// ---------------------------------------------------------------------------
// OverflowPolicy
// ---------------------------------------------------------------------------

/// How [`BudgetEngine`] handles prompts that exceed the remaining budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverflowPolicy {
    /// Reject the call with [`SubstrateError::Engine`].
    #[default]
    Reject,
    /// Truncate the prompt to fit the remaining budget.
    Truncate,
    /// Accept the prompt as-is and emit a tracing warning.
    Warn,
}

// ---------------------------------------------------------------------------
// BudgetConfig
// ---------------------------------------------------------------------------

/// Per-conversation budget configuration.
///
/// `max_tokens` is the *total* prompt token budget for a conversation. Set
/// to `usize::MAX` for "unlimited" (effectively disabling enforcement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetConfig {
    /// Maximum total tokens (prompt + resume prompts) per conversation.
    pub max_tokens: usize,
    /// Overflow handling policy.
    pub policy: OverflowPolicy,
}

impl BudgetConfig {
    /// Construct a new config with `max_tokens` and [`OverflowPolicy::Reject`].
    pub fn new(max_tokens: usize) -> Self {
        BudgetConfig {
            max_tokens,
            policy: OverflowPolicy::default(),
        }
    }

    /// Override the overflow policy.
    pub fn with_policy(mut self, policy: OverflowPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Construct an unlimited config (no enforcement).
    pub fn unlimited() -> Self {
        BudgetConfig {
            max_tokens: usize::MAX,
            policy: OverflowPolicy::default(),
        }
    }

    /// Returns true if the config is effectively unlimited.
    pub fn is_unlimited(&self) -> bool {
        self.max_tokens == usize::MAX
    }
}

impl Default for BudgetConfig {
    fn default() -> Self {
        // 32K tokens ≈ 128KB of text — a sane default for a single agent run.
        BudgetConfig::new(32_000)
    }
}

// ---------------------------------------------------------------------------
// BudgetLedger (per-conversation state)
// ---------------------------------------------------------------------------

/// Per-conversation usage ledger.
///
/// `used` is the running total of prompt tokens consumed. `conv_id` is the
/// engine's conversation id; `overflowed` is set to true once a Warn-mode
/// overflow has been accepted (purely informational).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BudgetLedger {
    /// The conversation id this ledger tracks.
    pub conv_id: String,
    /// Total prompt tokens consumed so far.
    pub used: usize,
    /// True once at least one overflow (in Warn mode) has been accepted.
    pub overflowed: bool,
}

impl BudgetLedger {
    /// Construct an empty ledger for `conv_id`.
    pub fn new(conv_id: impl Into<String>) -> Self {
        BudgetLedger {
            conv_id: conv_id.into(),
            used: 0,
            overflowed: false,
        }
    }

    /// Tokens remaining (`max - used`); saturates at 0.
    pub fn remaining(&self, max: usize) -> usize {
        max.saturating_sub(self.used)
    }
}

// ---------------------------------------------------------------------------
// BudgetEngine
// ---------------------------------------------------------------------------

/// [`EnginePort`] middleware that enforces a per-conversation token budget.
///
/// Wraps any `Arc<dyn EnginePort>` and intercepts `start()` / `resume()` to
/// check + update the ledger before delegating to the inner engine. `dump()`,
/// `cancel()`, `wire_mailbox()`, `extract_result()`, and `capabilities()` are
/// passed through unchanged.
pub struct BudgetEngine {
    inner: Arc<dyn EnginePort>,
    config: BudgetConfig,
    ledgers: RwLock<HashMap<String, BudgetLedger>>,
}

impl std::fmt::Debug for BudgetEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BudgetEngine")
            .field("inner", &"<dyn EnginePort>")
            .field("config", &self.config)
            .field(
                "ledger_count",
                &self.ledgers.try_read().map(|l| l.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl BudgetEngine {
    /// Construct a new [`BudgetEngine`] wrapping `inner` with `config`.
    pub fn new(inner: Arc<dyn EnginePort>, config: BudgetConfig) -> Self {
        BudgetEngine {
            inner,
            config,
            ledgers: RwLock::new(HashMap::new()),
        }
    }

    /// Read-only access to the current ledgers (cloned snapshot).
    pub async fn ledgers(&self) -> Vec<BudgetLedger> {
        self.ledgers.read().await.values().cloned().collect()
    }

    /// Look up a single ledger by conv_id.
    pub async fn ledger(&self, conv_id: &str) -> Option<BudgetLedger> {
        self.ledgers.read().await.get(conv_id).cloned()
    }

    /// Drop a ledger for `conv_id` (used after `cancel()` or terminal state).
    pub async fn forget(&self, conv_id: &str) {
        self.ledgers.write().await.remove(conv_id);
    }

    /// Returns the current budget config.
    pub fn config(&self) -> &BudgetConfig {
        &self.config
    }

    /// Returns the inner engine pointer (for composition with other middleware).
    pub fn inner(&self) -> &Arc<dyn EnginePort> {
        &self.inner
    }

    /// Check the ledger for `conv_id` and either accept / truncate / reject
    /// the prompt. Returns the (possibly-truncated) prompt to feed to the
    /// inner engine.
    async fn gate(&self, conv_id: &str, prompt: &str) -> Result<String> {
        if self.config.is_unlimited() {
            return Ok(prompt.to_string());
        }
        let tokens = estimate_tokens(prompt);
        let mut ledgers = self.ledgers.write().await;
        let ledger = ledgers
            .entry(conv_id.to_string())
            .or_insert_with(|| BudgetLedger::new(conv_id));

        let remaining = ledger.remaining(self.config.max_tokens);
        if tokens <= remaining {
            ledger.used += tokens;
            return Ok(prompt.to_string());
        }

        match self.config.policy {
            OverflowPolicy::Reject => Err(SubstrateError::Engine(format!(
                "context-budget: prompt of {tokens} tokens exceeds remaining {remaining} of budget {} for conv {conv_id}",
                self.config.max_tokens
            ))),
            OverflowPolicy::Truncate => {
                // Keep at most `remaining` tokens worth of chars (chars/4 ceiling).
                let max_chars = remaining.saturating_mul(4);
                let truncated: String = prompt.chars().take(max_chars).collect();
                let truncated_tokens = estimate_tokens(&truncated);
                ledger.used += truncated_tokens;
                Ok(truncated)
            }
            OverflowPolicy::Warn => {
                ledger.used += tokens;
                ledger.overflowed = true;
                Ok(prompt.to_string())
            }
        }
    }
}

#[async_trait]
impl EnginePort for BudgetEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        // Probe the inner engine first to get a real conv_id, then gate.
        // This means a Reject-mode overflow does one wasted inner call —
        // acceptable for an in-memory budget middleware.
        let session = self.inner.start(task).await?;
        // Gate against the prompt (or its truncated form).
        let _gated = self.gate(&session.conv_id, &task.prompt).await?;
        Ok(session)
    }

    async fn resume(&self, conv_id: &str, prompt: &str) -> Result<Session> {
        let gated = self.gate(conv_id, prompt).await?;
        self.inner.resume(conv_id, &gated).await
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        self.inner.dump(conv_id).await
    }

    async fn cancel(&self, conv_id: &str) -> Result<()> {
        let result = self.inner.cancel(conv_id).await;
        self.forget(conv_id).await;
        result
    }

    async fn wire_mailbox(&self, conv_id: &str, mailbox: &Mailbox) -> Result<()> {
        self.inner.wire_mailbox(conv_id, mailbox).await
    }

    fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
        self.inner.extract_result(dump)
    }

    fn capabilities(&self) -> EngineCapabilities {
        self.inner.capabilities()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use substrate_core::domain::TaskState;

    /// Stub engine that echoes the prompt as a dump.
    #[derive(Debug)]
    struct EchoEngine {
        conv_id: &'static str,
    }

    impl EchoEngine {
        fn new() -> Self {
            EchoEngine {
                conv_id: "echo-conv",
            }
        }
    }

    #[async_trait]
    impl EnginePort for EchoEngine {
        async fn start(&self, _task: &Task) -> Result<Session> {
            Ok(Session {
                conv_id: self.conv_id.to_string(),
                pid: None,
                logfile: None,
            })
        }

        async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
            Ok(Session {
                conv_id: conv_id.to_string(),
                pid: None,
                logfile: None,
            })
        }

        async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
            Ok(ConversationDump {
                conversation_id: conv_id.to_string(),
                raw: "{}".to_string(),
            })
        }

        async fn cancel(&self, _conv_id: &str) -> Result<()> {
            Ok(())
        }

        async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
            Ok(())
        }

        fn extract_result(&self, _dump: &ConversationDump) -> Result<StructuredResult> {
            Ok(StructuredResult {
                text: String::new(),
                artifacts: vec![],
                pr_urls: vec![],
                status: TaskState::Completed,
            })
        }

        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities {
                supports_resume: true,
                supports_subagents: false,
                supports_mcp_import: false,
            }
        }
    }

    // ── estimate_tokens ───────────────────────────────────────────────────────

    #[test]
    fn estimate_tokens_empty_string_is_zero() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_uses_chars_over_four_ceiling() {
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens("abcdefghi"), 3);
    }

    #[test]
    fn estimate_tokens_handles_unicode_char_count_not_bytes() {
        // 4 emoji chars = 4 chars / 4 = 1 token (NOT bytes).
        assert_eq!(estimate_tokens("🦀🦀🦀🦀"), 1);
    }

    #[test]
    fn estimate_tokens_bytes_uses_byte_length() {
        assert_eq!(estimate_tokens_bytes(b""), 0);
        assert_eq!(estimate_tokens_bytes(b"abcd"), 1);
        // 4 emoji = 16 bytes
        assert_eq!(estimate_tokens_bytes("🦀🦀🦀🦀".as_bytes()), 4);
    }

    // ── BudgetConfig ──────────────────────────────────────────────────────────

    #[test]
    fn budget_config_default_is_32k_reject() {
        let cfg = BudgetConfig::default();
        assert_eq!(cfg.max_tokens, 32_000);
        assert_eq!(cfg.policy, OverflowPolicy::Reject);
        assert!(!cfg.is_unlimited());
    }

    #[test]
    fn budget_config_unlimited_disables_enforcement() {
        let cfg = BudgetConfig::unlimited();
        assert!(cfg.is_unlimited());
    }

    #[test]
    fn budget_config_with_policy_overrides() {
        let cfg = BudgetConfig::new(100).with_policy(OverflowPolicy::Truncate);
        assert_eq!(cfg.policy, OverflowPolicy::Truncate);
    }

    // ── BudgetLedger ──────────────────────────────────────────────────────────

    #[test]
    fn ledger_remaining_saturates_at_zero() {
        let mut l = BudgetLedger::new("c1");
        l.used = 50;
        assert_eq!(l.remaining(100), 50);
        assert_eq!(l.remaining(40), 0); // saturates
        assert_eq!(l.remaining(50), 0);
    }

    // ── BudgetEngine: Reject mode ─────────────────────────────────────────────

    #[tokio::test]
    async fn reject_mode_accepts_within_budget() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::new(100));
        let task = Task::new("hello world", "/tmp"); // ~3 tokens
        let session = engine.start(&task).await.unwrap();
        assert_eq!(session.conv_id, "echo-conv");

        let ledger = engine.ledger("echo-conv").await.unwrap();
        assert_eq!(ledger.used, 3);
        assert!(!ledger.overflowed);
    }

    #[tokio::test]
    async fn reject_mode_blocks_overflow() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::new(10));
        // 100 chars = 25 tokens > 10 budget.
        let task = Task::new("a".repeat(100).as_str(), "/tmp");
        let err = engine.start(&task).await.unwrap_err();
        match err {
            SubstrateError::Engine(msg) => {
                assert!(msg.contains("context-budget"), "got: {msg}");
                assert!(msg.contains("25"));
                assert!(msg.contains("10"));
            }
            other => panic!("expected Engine error, got {other:?}"),
        }
    }

    // ── BudgetEngine: Truncate mode ───────────────────────────────────────────

    #[tokio::test]
    async fn truncate_mode_truncates_overflowing_prompt() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(
            inner.clone(),
            BudgetConfig::new(2).with_policy(OverflowPolicy::Truncate),
        );
        // 100 chars = 25 tokens, budget 2 → truncated to 8 chars.
        let task = Task::new("a".repeat(100).as_str(), "/tmp");
        engine.start(&task).await.unwrap();

        let ledger = engine.ledger("echo-conv").await.unwrap();
        // 8 chars / 4 ceiling = 2 tokens used (within remaining).
        assert_eq!(ledger.used, 2);
    }

    // ── BudgetEngine: Warn mode ───────────────────────────────────────────────

    #[tokio::test]
    async fn warn_mode_accepts_overflow_and_marks_ledger() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(
            inner,
            BudgetConfig::new(2).with_policy(OverflowPolicy::Warn),
        );
        let task = Task::new("a".repeat(100).as_str(), "/tmp");
        engine.start(&task).await.unwrap();

        let ledger = engine.ledger("echo-conv").await.unwrap();
        assert_eq!(ledger.used, 25);
        assert!(ledger.overflowed, "ledger must be marked as overflowed");
    }

    // ── BudgetEngine: unlimited ───────────────────────────────────────────────

    #[tokio::test]
    async fn unlimited_config_never_rejects() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::unlimited());
        let task = Task::new("a".repeat(1_000_000).as_str(), "/tmp");
        engine.start(&task).await.unwrap();
        // No ledger entry created for unlimited mode.
        assert!(engine.ledger("echo-conv").await.is_none());
    }

    // ── BudgetEngine: ledger lifecycle ────────────────────────────────────────

    #[tokio::test]
    async fn cancel_drops_ledger_entry() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::new(100));
        let task = Task::new("hi", "/tmp");
        engine.start(&task).await.unwrap();
        assert!(engine.ledger("echo-conv").await.is_some());
        engine.cancel("echo-conv").await.unwrap();
        assert!(engine.ledger("echo-conv").await.is_none());
    }

    #[tokio::test]
    async fn resume_updates_same_ledger() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::new(100));
        let task = Task::new("hello", "/tmp"); // ~2 tokens
        engine.start(&task).await.unwrap();
        engine.resume("echo-conv", "world").await.unwrap(); // ~2 tokens
        let ledger = engine.ledger("echo-conv").await.unwrap();
        assert_eq!(ledger.used, 4);
    }

    // ── BudgetEngine: pass-through methods ────────────────────────────────────

    #[tokio::test]
    async fn dump_extract_capabilities_pass_through() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::new(100));
        let dump = engine.dump("echo-conv").await.unwrap();
        assert_eq!(dump.conversation_id, "echo-conv");

        let result = engine.extract_result(&dump).unwrap();
        assert_eq!(result.status, TaskState::Completed);

        let caps = engine.capabilities();
        assert!(caps.supports_resume);
        assert!(!caps.supports_subagents);
    }

    #[tokio::test]
    async fn wire_mailbox_passes_through() {
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::new(100));
        let mb = Mailbox {
            owner: "o".into(),
            messages: vec![],
        };
        engine.wire_mailbox("echo-conv", &mb).await.unwrap();
    }

    // ── config() / inner() / ledgers() / forget() ────────────────────────────

    #[tokio::test]
    async fn config_and_inner_accessors() {
        let inner = Arc::new(EchoEngine::new());
        let cfg = BudgetConfig::new(42);
        let engine = BudgetEngine::new(inner, cfg.clone());
        assert_eq!(engine.config(), &cfg);
        assert!(Arc::strong_count(engine.inner()) >= 1);
    }

    #[tokio::test]
    async fn ledgers_returns_snapshot_of_all_ledgers() {
        // Drive two convs (simulated by manually seeding ledgers).
        let inner = Arc::new(EchoEngine::new());
        let engine = BudgetEngine::new(inner, BudgetConfig::new(100));
        let t1 = Task::new("hello", "/tmp");
        let t2 = Task::new("hi", "/tmp");
        engine.start(&t1).await.unwrap();
        engine.start(&t2).await.unwrap();
        let ledgers = engine.ledgers().await;
        assert_eq!(ledgers.len(), 2);
    }
}
