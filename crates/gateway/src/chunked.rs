pub struct Chunked {
    chunks: Vec<Vec<u8>>,
}
impl Chunked {
    pub fn new(data: Vec<u8>, chunk_size: usize) -> Self {
        let mut chunks = Vec::new();
        for slice in data.chunks(chunk_size) {
            chunks.push(slice.to_vec());
        }
        Self { chunks }
    }
    pub fn from_iter<I: IntoIterator<Item = Vec<u8>>>(it: I) -> Self {
        Self {
            chunks: it.into_iter().collect(),
        }
    }
    pub fn chunks(&self) -> &[Vec<u8>] {
        &self.chunks
    }
    pub fn len(&self) -> usize {
        self.chunks.iter().map(|c| c.len()).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
    pub fn assemble(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.len());
        for c in &self.chunks {
            out.extend_from_slice(c);
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_split() {
        let c = Chunked::new(vec![1, 2, 3, 4, 5, 6, 7], 3);
        assert_eq!(c.chunk_count(), 3);
    }
    #[test]
    fn test_assemble() {
        let data = vec![1, 2, 3, 4, 5];
        let c = Chunked::new(data.clone(), 2);
        assert_eq!(c.assemble(), data);
    }
    #[test]
    fn test_empty() {
        let c = Chunked::new(vec![], 10);
        assert!(c.is_empty());
        assert_eq!(c.chunk_count(), 0);
    }
    #[test]
    fn test_exact_size() {
        let c = Chunked::new(vec![1, 2, 3, 4], 2);
        assert_eq!(c.chunk_count(), 2);
    }
    #[test]
    fn test_from_iter() {
        let c = Chunked::from_iter(vec![vec![1, 2], vec![3, 4]]);
        assert_eq!(c.assemble(), vec![1, 2, 3, 4]);
    }
}
