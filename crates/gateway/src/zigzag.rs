pub fn encode_32(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}
pub fn decode_32(n: u32) -> i32 {
    let v = (n >> 1) as i32;
    let sign = ((n & 1) as i32) * -1;
    v ^ sign
}
pub fn encode_64(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}
pub fn decode_64(n: u64) -> i64 {
    let v = (n >> 1) as i64;
    let sign = ((n & 1) as i64) * -1;
    v ^ sign
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zz32_pos() {
        assert_eq!(encode_32(5), 10);
        assert_eq!(decode_32(10), 5);
    }
    #[test]
    fn zz32_neg() {
        assert_eq!(encode_32(-5), 9);
        assert_eq!(decode_32(9), -5);
    }
    #[test]
    fn zz32_zero() {
        assert_eq!(encode_32(0), 0);
        assert_eq!(decode_32(0), 0);
    }
    #[test]
    fn zz64_roundtrip() {
        for v in [-1000, -1, 0, 1, 1000] {
            assert_eq!(decode_64(encode_64(v)), v);
        }
    }
}
