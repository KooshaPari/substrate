//! `SqliteMemoryStore`: persistent [`MemoryPort`] backed by SQLite.

use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use substrate_core::memory_port::{MemoryEntry, MemoryPort};

use crate::error::StoreError;
use crate::schema;

/// Durable memory store with full history in SQLite.
pub struct SqliteMemoryStore {
    conn: Mutex<Connection>,
}

impl SqliteMemoryStore {
    /// Open (or create) a store at the given file path.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open a transient in-memory database (useful in tests).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl MemoryPort for SqliteMemoryStore {
    type Error = StoreError;

    fn append(&self, key: &str, content: &str) -> Result<Uuid, Self::Error> {
        let id = Uuid::new_v4();
        self.append_with_id(id, key, content)?;
        Ok(id)
    }

    fn get(&self, key: &str) -> Result<Option<String>, Self::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT content FROM memory WHERE mem_key = ?1 ORDER BY created_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn recent(&self, limit: usize) -> Result<Vec<MemoryEntry>, Self::Error> {
        self.history_limited(limit)
    }

    fn history(&self) -> Result<Vec<MemoryEntry>, Self::Error> {
        self.history_limited(usize::MAX)
    }
}

impl SqliteMemoryStore {
    /// Append an entry with a caller-supplied id (for composed stores).
    pub fn append_with_id(&self, id: Uuid, key: &str, content: &str) -> Result<(), StoreError> {
        let created_at = Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memory (id, mem_key, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.to_string(), key, content, created_at],
        )?;
        Ok(())
    }

    fn history_limited(&self, limit: usize) -> Result<Vec<MemoryEntry>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, mem_key, content, created_at FROM memory ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let id_str: String = row.get(0)?;
            let id = Uuid::parse_str(&id_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(MemoryEntry {
                id,
                key: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}
