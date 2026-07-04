use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug,Clone)]
pub struct Connection { pub id: usize, pub url: String }

pub struct ConnectionPool {
    inner: Arc<Mutex<VecDeque<Connection>>>,
    capacity: usize,
    next_id: Arc<Mutex<usize>>,
}
impl ConnectionPool {
    pub fn new(capacity: usize) -> Self {
        Self { inner: Arc::new(Mutex::new(VecDeque::new())), capacity, next_id: Arc::new(Mutex::new(0)) }
    }
    pub fn add(&self, url: impl Into<String>) -> bool {
        let mut pool = self.inner.lock().unwrap();
        if pool.len() >= self.capacity { return false; }
        let id = { let mut n=self.next_id.lock().unwrap(); let id=*n; *n+=1; id };
        pool.push_back(Connection { id, url: url.into() });
        true
    }
    pub fn acquire(&self) -> Option<Connection> { self.inner.lock().unwrap().pop_front() }
    pub fn release(&self, conn: Connection) {
        let mut pool = self.inner.lock().unwrap();
        if pool.len() < self.capacity { pool.push_back(conn); }
    }
    pub fn available(&self) -> usize { self.inner.lock().unwrap().len() }
    pub fn capacity(&self) -> usize { self.capacity }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn acquire_returns_conn() { let p=ConnectionPool::new(5); p.add("http://a"); let c=p.acquire().unwrap(); assert_eq!(c.url,"http://a"); }
    #[test] fn capacity_enforced() { let p=ConnectionPool::new(2); assert!(p.add("a")); assert!(p.add("b")); assert!(!p.add("c")); }
    #[test] fn release_restores() { let p=ConnectionPool::new(5); p.add("http://a"); let c=p.acquire().unwrap(); p.release(c); assert_eq!(p.available(),1); }
    #[test] fn empty_acquire_none() { assert!(ConnectionPool::new(5).acquire().is_none()); }
    #[test] fn available_count() { let p=ConnectionPool::new(5); p.add("a"); p.add("b"); assert_eq!(p.available(),2); }
}
