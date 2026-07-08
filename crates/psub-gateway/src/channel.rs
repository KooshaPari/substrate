use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

pub struct Channel<T> { items: Arc<Mutex<VecDeque<T>>> }
impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self { Self { items: self.items.clone() } }
}
impl<T> Channel<T> {
    pub fn new() -> Self { Self { items: Arc::new(Mutex::new(VecDeque::new())) } }
    pub fn send(&self, item: T) { self.items.lock().unwrap().push_back(item); }
    pub fn recv(&self) -> Option<T> { self.items.lock().unwrap().pop_front() }
    pub fn try_recv(&self) -> Option<T> { self.items.lock().unwrap().pop_front() }
    pub fn len(&self) -> usize { self.items.lock().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.items.lock().unwrap().is_empty() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn send_recv() { let ch: Channel<i32> = Channel::new(); ch.send(42); assert_eq!(ch.recv(), Some(42)); }
    #[test] fn order() { let ch: Channel<i32> = Channel::new(); ch.send(1); ch.send(2); ch.send(3); assert_eq!(ch.recv(), Some(1)); assert_eq!(ch.recv(), Some(2)); }
    #[test] fn empty_recv() { let ch: Channel<i32> = Channel::new(); assert_eq!(ch.recv(), None); }
    #[test] fn clone_shares() { let ch: Channel<i32> = Channel::new(); let ch2 = ch.clone(); ch.send(7); assert_eq!(ch2.recv(), Some(7)); }
    #[test] fn len() { let ch: Channel<i32> = Channel::new(); assert_eq!(ch.len(), 0); ch.send(1); ch.send(2); assert_eq!(ch.len(), 2); }
}
