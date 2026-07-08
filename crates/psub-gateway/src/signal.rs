pub struct DetachedSig { pub on: bool }
impl DetachedSig {
    pub fn new() -> Self { Self { on: false } }
    pub fn fire(&mut self) -> bool {
        if self.on { false } else { self.on = true; true }
    }
    pub fn is_set(&self) -> bool { self.on }
    pub fn reset(&mut self) { self.on = false; }
}
pub struct CounterSig { count: u32, threshold: u32, fired: bool }
impl CounterSig {
    pub fn new(threshold: u32) -> Self { Self { count: 0, threshold, fired: false } }
    pub fn tick(&mut self) -> bool {
        if self.fired { return false; }
        self.count += 1;
        if self.count >= self.threshold { self.fired = true; }
        true
    }
    pub fn is_fired(&self) -> bool { self.fired }
    pub fn reset(&mut self) { self.count = 0; self.fired = false; }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn detached_once() { let mut s = DetachedSig::new(); assert!(s.fire()); assert!(!s.fire()); }
    #[test] fn detached_reset() { let mut s = DetachedSig::new(); s.fire(); s.reset(); assert!(s.fire()); }
    #[test] fn counter_threshold() { let mut s = CounterSig::new(3); assert!(!s.is_fired()); s.tick(); s.tick(); assert!(!s.is_fired()); s.tick(); assert!(s.is_fired()); }
    #[test] fn counter_reset() { let mut s = CounterSig::new(2); s.tick(); s.tick(); s.reset(); assert!(!s.is_fired()); }
    #[test] fn counter_stops() { let mut s = CounterSig::new(1); s.tick(); assert!(s.is_fired()); s.tick(); assert!(s.is_fired()); }
}
