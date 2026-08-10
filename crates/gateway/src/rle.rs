pub fn encode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let mut count: u32 = 1;
        while (count as usize) < 255
            && i + (count as usize) < input.len()
            && input[i + count as usize] == input[i]
        {
            count += 1;
        }
        out.push(count as u8);
        out.push(input[i]);
        i += count as usize;
    }
    out
}
pub fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    if input.len() % 2 != 0 {
        return Err("RLE input must have even length".into());
    }
    let mut out = Vec::new();
    for chunk in input.chunks(2) {
        let count = chunk[0] as usize;
        if count == 0 {
            return Err("zero-length run".into());
        }
        for _ in 0..count {
            out.push(chunk[1]);
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let data = vec![1, 1, 1, 2, 2, 3, 3, 3, 3];
        let enc = encode(&data);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }
    #[test]
    fn unique() {
        let data = vec![1, 2, 3, 4];
        let enc = encode(&data);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }
    #[test]
    fn empty() {
        assert_eq!(encode(&[]), Vec::<u8>::new());
        assert_eq!(decode(&[]).unwrap(), Vec::<u8>::new());
    }
    #[test]
    fn all_same() {
        let data = vec![5; 200];
        let enc = encode(&data);
        assert!(enc.len() < 50);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }
    #[test]
    fn invalid() {
        assert!(decode(&[1, 2, 3]).is_err());
    }
}
