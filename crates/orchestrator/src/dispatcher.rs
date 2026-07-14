//! `Dispatcher` trait + a `MockDispatcher` used in MVP cut-line tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;
use crate::runner::DispatchOutcome;
use crate::wave::TaskSpec;

#[async_trait]
pub trait Dispatcher: Send + Sync {
    fn name(&self) -> &str;
    async fn dispatch(&self, task: &TaskSpec) -> Result<DispatchOutcome>;
}

/// Programmable dispatcher backed by a queue of outcomes; pops one per task.
pub struct MockDispatcher {
    name: String,
    outcomes: std::sync::Mutex<Vec<DispatchOutcome>>,
    observed: Arc<AtomicUsize>,
}

impl MockDispatcher {
    pub fn new(name: impl Into<String>, outcomes: Vec<DispatchOutcome>) -> Self {
        Self {
            name: name.into(),
            outcomes: std::sync::Mutex::new(outcomes),
            observed: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn call_count(&self) -> usize {
        self.observed.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Dispatcher for MockDispatcher {
    fn name(&self) -> &str {
        &self.name
    }

    async fn dispatch(&self, _task: &TaskSpec) -> Result<DispatchOutcome> {
        self.observed.fetch_add(1, Ordering::SeqCst);
        let next = self.outcomes.lock().expect("poisoned").pop();
        match next {
            Some(o) => Ok(o),
            None => Ok(DispatchOutcome::success(0, 0.0)),
        }
    }
}
