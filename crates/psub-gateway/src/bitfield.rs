#[derive(Default,Debug,Clone,Copy)]
pub struct BitField { bits: u64 }
impl BitField {
    pub fn new() -> Self { Self { bits: 0 } }
    pub fn set(&mut self, idx: u8) { if idx < 64 { self.bits |= 1u64 << idx; } }
    pub fn clear(&mut self, idx: u8) { if idx < 64 { self.bits &= !(1u64 << idx); } }
    pub fn get(&self, idx: u8) -> bool { idx < 64 && self.bits & (1u64 << idx) != 0 }
    pub fn toggle(&mut self, idx: u8) { if idx < 64 { self.bits ^= 1u64 << idx; } }
    pub fn count(&self) -> u32 { self.bits.count_ones() }
    pub fn any(&self) -> bool { self.bits != 0 }
    pub fn empty(&self) -> bool { self.bits == 0 }
    pub fn raw(&self) -> u64 { self.bits }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn set_and_get() { let mut b = BitField::new(); b.set(5); assert!(b.get(5)); assert!(!b.get(4)); }
    #[test] fn clear() { let mut b = BitField::new(); b.set(3); b.clear(3); assert!(!b.get(3)); }
    #[test] fn toggle() { let mut b = BitField::new(); b.toggle(2); assert!(b.get(2)); b.toggle(2); assert!(!b.get(2)); }
    #[test] fn count() { let mut b = BitField::new(); b.set(0); b.set(2); b.set(4); assert_eq!(b.count(), 3); }
    #[test] fn any_empty() { let b = BitField::new(); assert!(b.empty()); assert!(!b.any()); }
    #[test] fn out_of_range_noop() { let mut b = BitField::new(); b.set(100); assert!(!b.get(100)); assert_eq!(b.count(), 0); }
    #[test] fn raw() { let mut b = BitField::new(); b.set(0); b.set(2); assert_eq!(b.raw(), 0b101); }
}
