pub struct ByteBuffer { buf: Vec<u8>, read_pos: usize, write_pos: usize }
impl ByteBuffer {
    pub fn with_capacity(cap: usize) -> Self { Self { buf: vec![0; cap], read_pos: 0, write_pos: 0 } }
    pub fn from_vec(v: Vec<u8>) -> Self { let n = v.len(); Self { buf: v, read_pos: 0, write_pos: n } }
    pub fn available(&self) -> usize { self.buf.len() - self.write_pos }
    pub fn len(&self) -> usize { self.buf.len() }
    pub fn remaining_read(&self) -> usize { self.write_pos - self.read_pos }
    pub fn write_byte(&mut self, b: u8) -> bool {
        if self.write_pos >= self.buf.len() { return false; }
        self.buf[self.write_pos] = b;
        self.write_pos += 1;
        true
    }
    pub fn read_byte(&mut self) -> Option<u8> {
        if self.read_pos >= self.write_pos { return None; }
        let b = self.buf[self.read_pos];
        self.read_pos += 1;
        Some(b)
    }
    pub fn reset(&mut self) { self.read_pos = 0; self.write_pos = 0; }
    pub fn as_slice(&self) -> &[u8] { &self.buf[self.read_pos..self.write_pos] }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn write_read() { let mut b = ByteBuffer::with_capacity(10); assert!(b.write_byte(42)); assert_eq!(b.read_byte(), Some(42)); assert_eq!(b.read_byte(), None); }
    #[test] fn capacity_limit() { let mut b = ByteBuffer::with_capacity(2); assert!(b.write_byte(1)); assert!(b.write_byte(2)); assert!(!b.write_byte(3)); }
    #[test] fn empty() { let b = ByteBuffer::with_capacity(10); assert_eq!(b.remaining_read(), 0); }
    #[test] fn slice() { let mut b = ByteBuffer::with_capacity(10); b.write_byte(1); b.write_byte(2); assert_eq!(b.as_slice(), &[1, 2]); }
    #[test] fn reset() { let mut b = ByteBuffer::with_capacity(10); b.write_byte(1); b.read_byte(); b.reset(); assert_eq!(b.remaining_read(), 0); }
    #[test] fn from_vec() { let mut b = ByteBuffer::from_vec(vec![1,2,3]); assert_eq!(b.read_byte(), Some(1)); assert_eq!(b.remaining_read(), 2); }
}
