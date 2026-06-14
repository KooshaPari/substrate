//! # substrate-app
//!
//! The application/use-case layer. [`DispatchService`] orchestrates the
//! Phase-0 dispatch flow purely against the core ports — it knows nothing
//! about files, processes, or the forge CLI. The composition root
//! (`driver-cli`) supplies concrete adapters.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use substrate_core::domain::{StructuredResult, Task, TaskState};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::{DispatchApi, EnginePort, StorePort, TransportPort};
use uuid::Uuid;

/// Orchestrates dispatch over the three driven ports.
///
/// Generic over the concrete [`EnginePort`], [`StorePort`], and
/// [`TransportPort`] implementations so the use-case is testable with fakes
/// and reusable across adapters.
pub struct DispatchService<E, S, T> {
    engine: Arc<E>,
    store: Arc<S>,
    #[allow(dead_code)]
    transport: Arc<T>,
}

impl<E, S, T> DispatchService<E, S, T>
where
    E: EnginePort,
    S: StorePort,
    T: TransportPort,
{
    /// Wire the service from its ports.
    pub fn new(engine: Arc<E>, store: Arc<S>, transport: Arc<T>) -> Self {
        DispatchService {
            engine,
            store,
            transport,
        }
    }
}

#[async_trait]
impl<E, S, T> DispatchApi for DispatchService<E, S, T>
where
    E: EnginePort,
    S: StorePort,
    T: TransportPort,
{
    async fn dispatch(&self, mut task: Task) -> Result<StructuredResult> {
        // 1. Persist the submitted task.
        self.store.persist(&task).await?;

        // 2. Move to Working and start the engine.
        task.advance(TaskState::Working)?;
        self.store.persist(&task).await?;
        let session = self.engine.start(&task).await?;

        // 3. Dump and normalize.
        let dump = self.engine.dump(&session.conv_id).await?;
        let result = self.engine.extract_result(&dump)?;

        // 4. Reflect the engine's terminal status onto the task.
        if task.state != result.status {
            task.advance(result.status)?;
        }
        self.store.persist(&task).await?;
        self.store.persist_result(&task.id, &result).await?;

        Ok(result)
    }

    async fn get(&self, id: &Uuid) -> Result<Task> {
        self.store.load(id).await
    }

    async fn cancel(&self, id: &Uuid) -> Result<()> {
        let mut task = self.store.load(id).await?;
        if task.state.is_terminal() {
            return Err(SubstrateError::InvalidTransition {
                from: task.state,
                to: TaskState::Cancelled,
            });
        }
        self.engine.cancel(&id.to_string()).await?;
        task.advance(TaskState::Cancelled)?;
        self.store.persist(&task).await?;
        Ok(())
    }
}
