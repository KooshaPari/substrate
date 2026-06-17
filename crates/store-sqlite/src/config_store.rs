//! `SqliteConfigStore`: thin key-value config persistence for gateway management.

use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::StoreError;
use crate::schema;

/// A key-value config entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigEntry {
    /// Config key.
    pub key: String,
    /// Config value (opaque string).
    pub value: String,
}

/// SQLite-backed config store for gateway management endpoints.
pub struct SqliteConfigStore {
    conn: Mutex<Connection>,
}

impl SqliteConfigStore {
    /// Open (or create) a store at the given file path.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open a transient in-memory store (useful in tests).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Upsert a config key.
    pub fn set(&self, key: &str, value: &str) -> Result<ConfigEntry, StoreError> {
        let updated_at = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO gateway_config (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, updated_at],
        )?;
        Ok(ConfigEntry {
            key: key.to_string(),
            value: value.to_string(),
        })
    }

    /// Fetch a config key, if present.
    pub fn get(&self, key: &str) -> Result<Option<ConfigEntry>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM gateway_config WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ConfigEntry {
                key: row.get(0)?,
                value: row.get(1)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete a config key. Returns true if a row was removed.
    pub fn delete(&self, key: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM gateway_config WHERE key = ?1", params![key])?;
        Ok(n > 0)
    }

    /// List all config entries ordered by key.
    pub fn list(&self) -> Result<Vec<ConfigEntry>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM gateway_config ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok(ConfigEntry {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}
