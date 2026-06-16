#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Two-tier agent memory: bounded ring buffer + persistent SQLite history.

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::Utc;
use store_sqlite::SqliteMemoryStore;
use substrate_core::memory_port::{MemoryEntry, MemoryPort};
use uuid::Uuid;

/// Error type for in-memory memory adapters.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// Underlying SQLite store failure.
    #[error("store: {0}")]
    Store(#[from] store_sqlite::StoreError),
    /// Generic memory error.
    #[error("{0}")]
    Other(String),
}

/// Bounded ring buffer keeping the most recent `capacity` entries.
pub struct RingMemory {
    capacity: usize,
    entries: Mutex<VecDeque<MemoryEntry>>,
}

impl RingMemory {
    /// Create a ring buffer that retains at most `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(VecDeque::new()),
        }
    }

    fn push_entry(&self, key: &str, content: &str) -> Uuid {
        let id = Uuid::new_v4();
        let entry = MemoryEntry {
            id,
            key: key.to_string(),
            content: content.to_string(),
            created_at: Utc::now().timestamp(),
        };
        let mut buf = self.entries.lock().unwrap();
        if self.capacity > 0 && buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(entry);
        id
    }
}

impl MemoryPort for RingMemory {
    type Error = MemoryError;

    fn append(&self, key: &str, content: &str) -> Result<Uuid, Self::Error> {
        Ok(self.push_entry(key, content))
    }

    fn get(&self, key: &str) -> Result<Option<String>, Self::Error> {
        let buf = self.entries.lock().unwrap();
        Ok(buf
            .iter()
            .rev()
            .find(|e| e.key == key)
            .map(|e| e.content.clone()))
    }

    fn recent(&self, limit: usize) -> Result<Vec<MemoryEntry>, Self::Error> {
        let buf = self.entries.lock().unwrap();
        Ok(buf.iter().rev().take(limit).cloned().collect())
    }

    fn history(&self) -> Result<Vec<MemoryEntry>, Self::Error> {
        self.recent(usize::MAX)
    }
}

/// Composes a hot [`RingMemory`] tier with a cold [`SqliteMemoryStore`] tier.
pub struct TwoTierMemory {
    ring: RingMemory,
    persistent: SqliteMemoryStore,
}

impl TwoTierMemory {
    /// Create a two-tier store with ring capacity `ring_capacity`.
    pub fn in_memory(ring_capacity: usize) -> Result<Self, MemoryError> {
        Ok(Self {
            ring: RingMemory::new(ring_capacity),
            persistent: SqliteMemoryStore::open_in_memory()?,
        })
    }
}

impl MemoryPort for TwoTierMemory {
    type Error = MemoryError;

    fn append(&self, key: &str, content: &str) -> Result<Uuid, Self::Error> {
        self.ring.append(key, content)?;
        self.persistent
            .append(key, content)
            .map_err(MemoryError::from)
    }

    fn get(&self, key: &str) -> Result<Option<String>, Self::Error> {
        if let Some(v) = self.ring.get(key)? {
            return Ok(Some(v));
        }
        self.persistent.get(key).map_err(MemoryError::from)
    }

    fn recent(&self, limit: usize) -> Result<Vec<MemoryEntry>, Self::Error> {
        self.ring.recent(limit)
    }

    fn history(&self) -> Result<Vec<MemoryEntry>, Self::Error> {
        self.persistent.history().map_err(MemoryError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_evicts_oldest_at_capacity() {
        let ring = RingMemory::new(2);
        ring.append("a", "1").unwrap();
        ring.append("b", "2").unwrap();
        ring.append("c", "3").unwrap();
        let recent = ring.recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].content, "3");
        assert_eq!(recent[1].content, "2");
        assert!(recent.iter().all(|e| e.content != "1"));
    }

    #[test]
    fn persistent_round_trip() {
        let store = SqliteMemoryStore::open_in_memory().unwrap();
        store.append("topic", "hello").unwrap();
        assert_eq!(store.get("topic").unwrap(), Some("hello".into()));
        let hist = store.history().unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].content, "hello");
    }

    #[test]
    fn two_tier_compose() {
        let mem = TwoTierMemory::in_memory(2).unwrap();
        mem.append("k", "v1").unwrap();
        mem.append("k", "v2").unwrap();
        mem.append("k", "v3").unwrap();
        assert_eq!(mem.get("k").unwrap(), Some("v3".into()));
        assert_eq!(mem.recent(10).unwrap().len(), 2);
        assert_eq!(mem.history().unwrap().len(), 3);
    }
}
