const BUCKET_SIZE: usize = 32;
pub struct Dictionary {
    buckets: Vec<Vec<Vec<u8>>>,
    count: usize,
}
impl Dictionary {
    pub fn new() -> Self {
        Self {
            buckets: vec![Vec::new(); 256],
            count: 0,
        }
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn capacity(&self) -> usize {
        256 * BUCKET_SIZE
    }
    pub fn insert(&mut self, word: &[u8]) -> bool {
        if word.is_empty() || word.len() > 64 {
            return false;
        }
        let bucket = &mut self.buckets[word[0] as usize];
        if bucket.len() >= BUCKET_SIZE {
            return false;
        }
        if bucket.iter().any(|w| w.as_slice() == word) {
            return false;
        }
        bucket.push(word.to_vec());
        self.count += 1;
        true
    }
    pub fn contains(&self, word: &[u8]) -> bool {
        if word.is_empty() || word[0] as usize >= 256 {
            return false;
        }
        self.buckets[word[0] as usize]
            .iter()
            .any(|w| w.as_slice() == word)
    }
    pub fn find_match(&self, data: &[u8], pos: usize) -> Option<(usize, usize)> {
        if pos >= data.len() {
            return None;
        }
        let first = data[pos];
        let bucket = &self.buckets[first as usize];
        let mut best: Option<(usize, usize)> = None;
        for word in bucket {
            if word.len() > data.len() - pos {
                continue;
            }
            let mut i = 0;
            while i < word.len() && data[pos + i] == word[i] {
                i += 1;
            }
            if i == word.len() && best.map_or(true, |(_, bl)| word.len() > bl) {
                best = Some((pos.saturating_sub(0), word.len()));
            }
        }
        best
    }
    pub fn bucket_size(&self, first_byte: u8) -> usize {
        self.buckets[first_byte as usize].len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty() {
        let d = Dictionary::new();
        assert_eq!(d.len(), 0);
        assert!(d.is_empty());
        assert!(!d.contains(b"hello"));
    }
    #[test]
    fn insert_contains() {
        let mut d = Dictionary::new();
        assert!(d.insert(b"hello"));
        assert!(d.contains(b"hello"));
        assert!(!d.contains(b"world"));
        assert_eq!(d.len(), 1);
    }
    #[test]
    fn insert_duplicate_rejected() {
        let mut d = Dictionary::new();
        assert!(d.insert(b"hello"));
        assert!(!d.insert(b"hello"));
        assert_eq!(d.len(), 1);
    }
    #[test]
    fn insert_empty_rejected() {
        let mut d = Dictionary::new();
        assert!(!d.insert(b""));
        assert_eq!(d.len(), 0);
    }
    #[test]
    fn insert_oversized_rejected() {
        let mut d = Dictionary::new();
        let big = vec![b'a'; 100];
        assert!(!d.insert(&big));
    }
    #[test]
    fn find_match_exact() {
        let mut d = Dictionary::new();
        d.insert(b"the");
        let m = d.find_match(b"the", 0);
        assert_eq!(m, Some((0, 3)));
    }
    #[test]
    fn find_match_at_offset() {
        let mut d = Dictionary::new();
        d.insert(b"http");
        let m = d.find_match(b"abc http://x", 4);
        assert_eq!(m, Some((4, 4)));
    }
    #[test]
    fn find_match_no_match() {
        let mut d = Dictionary::new();
        d.insert(b"hello");
        assert_eq!(d.find_match(b"world", 0), None);
    }
    #[test]
    fn bucket_overflow() {
        let mut d = Dictionary::new();
        for i in 0..BUCKET_SIZE + 5 {
            let mut w = vec![
                b'a',
                b'0' + (i % 10) as u8,
                b'0' + ((i / 10) % 10) as u8,
                b'0' + ((i / 100) % 10) as u8,
            ];
            d.insert(&w);
        }
        assert_eq!(d.bucket_size(b'a'), BUCKET_SIZE);
    }
    #[test]
    fn capacity() {
        let d = Dictionary::new();
        assert_eq!(d.capacity(), 256 * BUCKET_SIZE);
    }
}
