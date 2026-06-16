//! `SqliteEventStore`: append-only event log with global monotonic ordering.

use std::marker::PhantomData;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection, TransactionBehavior};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use substrate_core::event_store_port::{EventEnvelope, EventStorePort};

use crate::error::StoreError;
use crate::schema;

/// SQLite-backed [`EventStorePort`] with `BEGIN IMMEDIATE` sequence allocation.
pub struct SqliteEventStore<E> {
    conn: Mutex<Connection>,
    _event: PhantomData<E>,
}

impl<E> SqliteEventStore<E>
where
    E: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    /// Open (or create) an event store at the given file path.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            _event: PhantomData,
        })
    }

    /// Open a transient in-memory store (useful in tests).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            _event: PhantomData,
        })
    }

    /// Return all events across aggregates ordered by `global_seq` (for tests).
    pub fn load_all_global(&self) -> Result<Vec<EventEnvelope<E>>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT aggregate_id, aggregate_seq, global_seq, payload, occurred_at \
             FROM event_log ORDER BY global_seq ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (aggregate_id, aggregate_seq, global_seq, payload, occurred_at) = row?;
            Ok(EventEnvelope {
                aggregate_id: Uuid::parse_str(&aggregate_id).unwrap_or_else(|_| Uuid::nil()),
                aggregate_seq: aggregate_seq as u64,
                global_seq: global_seq as u64,
                event: serde_json::from_str(&payload)?,
                occurred_at,
            })
        })
        .collect()
    }
}

impl<E> EventStorePort for SqliteEventStore<E>
where
    E: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    type Error = StoreError;
    type Event = E;

    fn append(
        &self,
        aggregate_id: Uuid,
        expected_seq: u64,
        event: &Self::Event,
    ) -> Result<EventEnvelope<Self::Event>, Self::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let current: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM event_log WHERE aggregate_id = ?1",
                params![aggregate_id.to_string()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if current as u64 != expected_seq {
            tx.rollback()?;
            return Err(StoreError::DuplicateEventSeq {
                aggregate_id: aggregate_id.to_string(),
                expected: expected_seq,
            });
        }

        let global_seq: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(global_seq), -1) + 1 FROM event_log",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let payload = serde_json::to_string(event)?;
        let occurred_at = Utc::now().timestamp();

        tx.execute(
            "INSERT INTO event_log (aggregate_id, aggregate_seq, global_seq, payload, occurred_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                aggregate_id.to_string(),
                expected_seq as i64,
                global_seq,
                payload,
                occurred_at,
            ],
        )?;

        tx.commit()?;

        Ok(EventEnvelope {
            aggregate_id,
            aggregate_seq: expected_seq,
            global_seq: global_seq as u64,
            event: event.clone(),
            occurred_at,
        })
    }

    fn load(&self, aggregate_id: Uuid) -> Result<Vec<EventEnvelope<Self::Event>>, Self::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT aggregate_id, aggregate_seq, global_seq, payload, occurred_at \
             FROM event_log WHERE aggregate_id = ?1 ORDER BY aggregate_seq ASC",
        )?;
        let rows = stmt.query_map(params![aggregate_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (aggregate_id, aggregate_seq, global_seq, payload, occurred_at) = row?;
            Ok(EventEnvelope {
                aggregate_id: Uuid::parse_str(&aggregate_id).unwrap_or_else(|_| Uuid::nil()),
                aggregate_seq: aggregate_seq as u64,
                global_seq: global_seq as u64,
                event: serde_json::from_str(&payload)?,
                occurred_at,
            })
        })
        .collect()
    }
}
