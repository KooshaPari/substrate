pub fn encode_chunks(data: &[u8], chunk_size: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let end = (pos + chunk_size).min(data.len());
        out.push(data[pos..end].to_vec());
        pos = end;
    }
    if out.is_empty() {
        out.push(Vec::new());
    }
    out
}
pub fn encode_chunked_streaming<F: FnMut(&[u8])>(data: &[u8], chunk_size: usize, mut emit: F) {
    let mut pos = 0;
    while pos < data.len() {
        let end = (pos + chunk_size).min(data.len());
        emit(&data[pos..end]);
        pos = end;
    }
}
pub fn decode_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    let mut out = Vec::with_capacity(total);
    for c in chunks { out.extend_from_slice(c); }
    out
}
pub fn hex_chunk_header(size: usize) -> String {
    format!("{:x}\r\n", size)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn empty() {
        let chunks = encode_chunks(&[], 10);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].is_empty());
    }
    #[test] fn smaller_than_chunk() {
        let data = b"hello";
        let chunks = encode_chunks(data, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data);
    }
    #[test] fn exact_chunk_size() {
        let data = b"0123456789";
        let chunks = encode_chunks(data, 5);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], b"01234");
        assert_eq!(chunks[1], b"56789");
    }
    #[test] fn partial_last_chunk() {
        let data = b"0123456789ab";
        let chunks = encode_chunks(data, 5);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[2], b"ab");
    }
    #[test] fn decode_round_trip() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let chunks = encode_chunks(data, 7);
        let back = decode_chunks(&chunks);
        assert_eq!(back, data);
    }
    #[test] fn streaming_matches_vector() {
        let data = b"abcdefghijklmnop";
        let mut collected: Vec<Vec<u8>> = Vec::new();
        encode_chunked_streaming(data, 4, |c| collected.push(c.to_vec()));
        assert_eq!(collected, encode_chunks(data, 4));
    }
    #[test] fn hex_header_format() {
        let h = hex_chunk_header(255);
        assert_eq!(h, "ff\r\n");
    }
    #[test] fn hex_header_zero() {
        let h = hex_chunk_header(0);
        assert_eq!(h, "0\r\n");
    }
    #[test] fn hex_header_large() {
        let h = hex_chunk_header(4096);
        assert_eq!(h, "1000\r\n");
    }
}
