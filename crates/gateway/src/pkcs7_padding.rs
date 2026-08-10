// PKCS#7 padding: append N bytes each of value N so total length is a multiple of `block_size`.
// Block size typically 8 (DES) or 16 (AES).
pub fn pad(data: &[u8], block_size: usize) -> Vec<u8> {
    assert!(block_size > 0 && block_size <= 255);
    let pad_len = block_size - (data.len() % block_size);
    let mut out = Vec::with_capacity(data.len() + pad_len);
    out.extend_from_slice(data);
    for _ in 0..pad_len {
        out.push(pad_len as u8);
    }
    out
}
pub fn unpad(data: &[u8], block_size: usize) -> Result<&[u8], String> {
    if data.is_empty() || data.len() % block_size != 0 {
        return Err("data not aligned to block_size".into());
    }
    let pad_byte = *data.last().unwrap();
    if pad_byte == 0 || pad_byte as usize > block_size {
        return Err("invalid pad byte".into());
    }
    let pad_len = pad_byte as usize;
    if pad_len > data.len() {
        return Err("pad exceeds data".into());
    }
    for &b in &data[data.len() - pad_len..] {
        if b != pad_byte {
            return Err("inconsistent pad".into());
        }
    }
    Ok(&data[..data.len() - pad_len])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pad_exact_block() {
        let p = pad(b"01234567", 8);
        assert_eq!(p, b"01234567\x08\x08\x08\x08\x08\x08\x08\x08");
    }
    #[test]
    fn pad_partial() {
        let p = pad(b"hello", 8);
        assert_eq!(p, b"hello\x03\x03\x03");
    }
    #[test]
    fn pad_empty_full_block() {
        let p = pad(&[], 8);
        assert_eq!(p, vec![8u8; 8]);
    }
    #[test]
    fn pad_empty_aes() {
        let p = pad(&[], 16);
        assert_eq!(p, vec![16u8; 16]);
    }
    #[test]
    fn round_trip_various_sizes() {
        for size in 0..50 {
            let data: Vec<u8> = (0..size).map(|i| i as u8).collect();
            let padded = pad(&data, 16);
            assert_eq!(padded.len() % 16, 0);
            let unpadded = unpad(&padded, 16).unwrap();
            assert_eq!(unpadded, &data[..]);
        }
    }
    #[test]
    fn unpad_exact_block() {
        let p = pad(b"01234567", 8);
        assert_eq!(unpad(&p, 8).unwrap(), b"01234567");
    }
    #[test]
    fn unpad_partial() {
        let p = pad(b"hello", 8);
        assert_eq!(unpad(&p, 8).unwrap(), b"hello");
    }
    #[test]
    fn unpad_empty() {
        let p = pad(&[], 16);
        assert_eq!(unpad(&p, 16).unwrap(), b"");
    }
    #[test]
    fn unpad_bad_alignment() {
        assert!(unpad(b"hello", 8).is_err());
    }
    #[test]
    fn unpad_bad_pad_byte() {
        let bad = vec![0u8; 8];
        assert!(unpad(&bad, 8).is_err());
    }
    #[test]
    fn unpad_pad_exceeds_block() {
        let bad = vec![20u8; 8];
        assert!(unpad(&bad, 8).is_err());
    }
    #[test]
    fn unpad_inconsistent() {
        let mut bad = vec![0u8; 7];
        bad.push(2); // pad byte says 2 but only 1 byte matches
        bad[6] = 1;
        assert!(unpad(&bad, 8).is_err());
    }
}
