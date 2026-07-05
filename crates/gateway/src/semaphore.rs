use std::sync::{Arc, Mutex};

struct Inner { available: usize, max: usize }

pub struct Semaphore { inner: Arc<Mutex<Inner>> }
impl Semaphore {
    pub fn new(max: usize) -> Self { Self { inner: Arc::new(Mutex::new(Inner { available: max, max })) } }
    pub fn try_acquire(&self) -> Option<SemaphoreGuard> {
        let mut g = self.inner.lock().unwrap();
        if g.available > 0 {
            g.available -= 1;
            Some(SemaphoreGuard { sem: self.inner.clone() })
        } else { None }
    }
    pub fn available(&self) -> usize { self.inner.lock().unwrap().available }
    pub fn max(&self) -> usize { self.inner.lock().unwrap().max }
    #[allow(dead_code)]
    fn release(&self) {
        let mut g = self.inner.lock().unwrap();
        g.available += 1;
    }
}

pub struct SemaphoreGuard { sem: Arc<Mutex<Inner>> }
impl Drop for SemaphoreGuard {
    fn drop(&mut self) { self.sem.lock().unwrap().available += 1; }
}

pub struct WeightedSemaphore { max_weight: usize, current: Mutex<usize> }
impl WeightedSemaphore {
    pub fn new(max_weight: usize) -> Self { Self { max_weight, current: Mutex::new(0) } }
    pub fn try_acquire(&self, weight: usize) -> bool {
        let mut g = self.current.lock().unwrap();
        if *g + weight <= self.max_weight { *g += weight; true } else { false }
    }
    pub fn release(&self, weight: usize) {
        let mut g = self.current.lock().unwrap();
        *g = g.saturating_sub(weight);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn try_acquire_decrements() { let s = Semaphore::new(2); let _g = s.try_acquire().unwrap(); assert_eq!(s.available(), 1); }
    #[test] fn blocks_when_full() { let s = Semaphore::new(1); let _g = s.try_acquire().unwrap(); assert!(s.try_acquire().is_none()); }
    #[test] fn guard_release() { let s = Semaphore::new(1); { let _g = s.try_acquire().unwrap(); drop(_g); } assert_eq!(s.available(), 1); }
    #[test] fn weighted_acquire_release() { let s = WeightedSemaphore::new(10); assert!(s.try_acquire(7)); assert!(s.try_acquire(2)); assert!(!s.try_acquire(2)); s.release(7); assert!(s.try_acquire(7)); }
    #[test] fn weighted_release_clamps() { let s = WeightedSemaphore::new(10); s.release(100); assert!(s.try_acquire(10)); }
}
