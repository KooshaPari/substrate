pub fn compute(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xffffffff;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb88320 } else { crc >> 1 };
        }
    }
    crc ^ 0xffffffff
}

pub fn fcs16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xffff;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xa001 } else { crc >> 1 };
        }
    }
    crc
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_crc32_known() { assert_eq!(compute(b"123456789"), 0xcbf43926); }
    #[test] fn test_crc32_empty() { assert_eq!(compute(b""), 0); }
    #[test] fn test_crc32_diff() { assert_ne!(compute(b"hello"), compute(b"world")); }
    #[test] fn test_fcs16_diff() { assert_ne!(fcs16(b"hello"), fcs16(b"world")); }
    #[test] fn test_fcs16_empty() { assert_eq!(fcs16(b""), 0); }
}
