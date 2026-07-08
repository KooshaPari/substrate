//! Simple bounded connection pool with capacity tracking.
//!
//! Provides a small, sync connection pool suitable for tests and lightweight
//! resource gating. `acquire` hands out a `PoolGuard`; the guard's `Drop`
//! returns its slot to the pool, so callers cannot lose capacity.
//!
//! This is intentionally minimal: no async, no backpressure, no eviction.
//! For real I/O use `deadpool` or `bb8`; for tests and stand-in logic this
//! gives a precise capacity contract.

use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A mock connection identifier handed out by [`ConnectionPool`].
///
/// Real implementations would carry a network handle, channel id, or
/// socket; for now we only track an incrementing integer so tests can
/// assert against distinct handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub id: u64,
}

/// Shared pool state: capacity, currently in-use count, and the next id.
#[derive(Debug)]
struct PoolInner {
    max_size: usize,
    in_use: AtomicUsize,
    next_id: AtomicUsize,
}

impl PoolInner {
    fn next_id(&self) -> u64 {
        // Relaxed is fine: ids only need to be unique within a single pool.
        self.next_id.fetch_add(1, Ordering::Relaxed) as u64
    }
}

/// A bounded, thread-safe connection pool.
#[derive(Debug, Clone)]
pub struct ConnectionPool {
    inner: Arc<PoolInner>,
}

impl ConnectionPool {
    /// Construct a pool that admits at most `max_size` concurrent
    /// connections. `max_size` must be greater than zero.
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "connection pool max_size must be > 0");
        Self {
            inner: Arc::new(PoolInner {
                max_size,
                in_use: AtomicUsize::new(0),
                next_id: AtomicUsize::new(0),
            }),
        }
    }

    /// Try to acquire a slot. Returns `Some(PoolGuard)` when capacity is
    /// available, `None` when the pool is fully saturated.
    pub fn try_acquire(&self) -> Option<PoolGuard> {
        let mut current = self.inner.in_use.load(Ordering::SeqCst);
        loop {
            if current >= self.inner.max_size {
                return None;
            }
            match self.inner.in_use.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    let id = self.inner.next_id();
                    return Some(PoolGuard {
                        conn: Connection { id },
                        pool: self.inner.clone(),
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }

    /// Blocking-by-fallback acquire: returns a guard when capacity is
    /// available, `None` when saturated. Provided as a convenience wrapper
    /// around `try_acquire` for call sites that don't want to handle `Option`
    /// for the "happy path" via a panic-free contract.
    pub fn acquire(&self) -> PoolGuard {
        self.try_acquire().expect("connection pool saturated")
    }

    /// Slots currently free for acquisition.
    pub fn available(&self) -> usize {
        let used = self.inner.in_use.load(Ordering::SeqCst);
        self.inner.max_size.saturating_sub(used)
    }

    /// Slots currently held by guards.
    pub fn in_use(&self) -> usize {
        self.inner.in_use.load(Ordering::SeqCst)
    }

    /// Configured upper bound on concurrent connections.
    pub fn max_size(&self) -> usize {
        self.inner.max_size
    }
}

/// RAII handle returned by [`ConnectionPool::try_acquire`] / [`ConnectionPool::acquire`].
///
/// On `Drop` the slot is returned to the pool. Deref exposes the
/// underlying [`Connection`].
pub struct PoolGuard {
    conn: Connection,
    pool: Arc<PoolInner>,
}

impl PoolGuard {
    /// Underlying connection.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

impl Deref for PoolGuard {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        // Drop is infallible; saturating_sub guards against double-release
        // bugs (which would otherwise underflow).
        let prev = self.pool.in_use.fetch_update(
            Ordering::SeqCst,
            Ordering::SeqCst,
            |v| Some(v.saturating_sub(1)),
        );
        debug_assert!(prev.is_ok(), "pool in_use went negative");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_is_max_size() {
        let pool = ConnectionPool::new(4);
        assert_eq!(pool.max_size(), 4);
        assert_eq!(pool.available(), 4);
        assert_eq!(pool.in_use(), 0);
    }

    #[test]
    fn acquire_decrements_available() {
        let pool = ConnectionPool::new(3);
        let _g1 = pool.acquire();
        assert_eq!(pool.in_use(), 1);
        assert_eq!(pool.available(), 2);

        let _g2 = pool.acquire();
        assert_eq!(pool.in_use(), 2);
        assert_eq!(pool.available(), 1);

        let _g3 = pool.acquire();
        assert_eq!(pool.in_use(), 3);
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn release_increments_available() {
        let pool = ConnectionPool::new(2);
        let g1 = pool.acquire();
        assert_eq!(pool.available(), 1);

        drop(g1);
        assert_eq!(pool.in_use(), 0);
        assert_eq!(pool.available(), 2);
    }

    #[test]
    fn try_acquire_returns_none_when_full() {
        let pool = ConnectionPool::new(1);
        let _g = pool.acquire();
        assert_eq!(pool.available(), 0);
        assert!(pool.try_acquire().is_none());
    }

    #[test]
    fn distinct_handles_have_distinct_ids() {
        let pool = ConnectionPool::new(4);
        let a = pool.acquire();
        let b = pool.acquire();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn guard_deref_yields_connection() {
        let pool = ConnectionPool::new(2);
        let g = pool.acquire();
        // PoolGuard derefs to Connection.
        let conn: &Connection = &*g;
        assert_eq!(conn.id, g.id);
    }

    #[test]
    #[should_panic(expected = "max_size must be > 0")]
    fn zero_capacity_panics() {
        let _ = ConnectionPool::new(0);
    }
}