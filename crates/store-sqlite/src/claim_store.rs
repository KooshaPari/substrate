//! `SqliteClaimStore`: atomic work-queue with fuzzy near-duplicate detection.

use std::collections::HashSet;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use strsim::levenshtein;
use uuid::Uuid;

use substrate_core::claim_port::{ClaimPort, WorkItem, WorkItemState};

use crate::error::StoreError;
use crate::schema;

/// Jaccard similarity threshold for token sets.
const JACCARD_THRESHOLD: f64 = 0.75;
/// Normalized Levenshtein similarity threshold.
const LEVENSHTEIN_THRESHOLD: f64 = 0.85;

/// SQLite-backed [`ClaimPort`] with `BEGIN IMMEDIATE` CAS claiming.
pub struct SqliteClaimStore {
    conn: Mutex<Connection>,
}

impl SqliteClaimStore {
    /// Open (or create) a claim store at the given file path.
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
}

fn token_jaccard(a: &str, b: &str) -> f64 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let ta: HashSet<&str> = a_lower.split_whitespace().collect();
    let tb: HashSet<&str> = b_lower.split_whitespace().collect();
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    let inter = ta.intersection(&tb).count();
    let union = ta.union(&tb).count();
    if union == 0 {
        return 0.0;
    }
    inter as f64 / union as f64
}

fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let dist = levenshtein(a, b);
    let max_len = a.chars().count().max(b.chars().count()).max(1);
    1.0 - (dist as f64 / max_len as f64)
}

/// Returns true when bodies are near-duplicates by token Jaccard or Levenshtein.
pub fn bodies_are_near_duplicate(a: &str, b: &str) -> bool {
    token_jaccard(a, b) >= JACCARD_THRESHOLD || levenshtein_similarity(a, b) >= LEVENSHTEIN_THRESHOLD
}

impl ClaimPort for SqliteClaimStore {
    type Error = StoreError;

    fn enqueue(&self, queue: &str, body: &str) -> Result<Uuid, StoreError> {
        if self.is_near_duplicate(queue, body)? {
            return Err(StoreError::Duplicate(format!(
                "near-duplicate in queue {queue}"
            )));
        }
        let id = Uuid::new_v4();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO work_queue (id, queue, body, state, claimed_by, created_at) \
             VALUES (?1, ?2, ?3, 'pending', NULL, ?4)",
            params![id.to_string(), queue, body, Utc::now().to_rfc3339()],
        )?;
        Ok(id)
    }

    fn claim_next(&self, queue: &str, worker_id: &str) -> Result<Option<WorkItem>, StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let row: Option<(String, String, String)> = tx
            .query_row(
                "SELECT id, queue, body FROM work_queue \
                 WHERE queue=?1 AND state='pending' \
                 ORDER BY created_at ASC LIMIT 1",
                params![queue],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let Some((id, q, body)) = row else {
            tx.rollback()?;
            return Ok(None);
        };

        let updated = tx.execute(
            "UPDATE work_queue SET state='claimed', claimed_by=?2 \
             WHERE id=?1 AND state='pending'",
            params![id, worker_id],
        )?;

        if updated != 1 {
            tx.rollback()?;
            return Ok(None);
        }

        tx.commit()?;

        Ok(Some(WorkItem {
            id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
            queue: q,
            body,
            state: WorkItemState::Claimed,
            claimed_by: Some(worker_id.to_string()),
        }))
    }

    fn is_near_duplicate(&self, queue: &str, body: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT body FROM work_queue \
             WHERE queue=?1 AND state IN ('pending', 'claimed')",
        )?;
        let rows = stmt.query_map(params![queue], |row| row.get::<_, String>(0))?;
        for row in rows {
            let existing = row?;
            if bodies_are_near_duplicate(body, &existing) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_duplicate_detection() {
        assert!(bodies_are_near_duplicate(
            "refactor auth module login flow",
            "refactor auth module login flo"
        ));
        assert!(bodies_are_near_duplicate(
            "fix the login bug in auth module",
            "fix the login bug in auth modul"
        ));
        assert!(!bodies_are_near_duplicate(
            "implement payment gateway",
            "write unit tests for scheduler"
        ));
    }
}
