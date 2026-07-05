pub fn hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data { h = h.wrapping_mul(0x100000001b3) ^ *b as u64; }
    h
}
pub fn meets(data: &[u8], target: u64) -> bool {
    hash(data) < target
}
pub fn difficulty_for(target: u64) -> u32 {
    let mut bits = 0;
    let mut t = target;
    while t > 0 && t < u64::MAX { bits += 1; t = t << 1; }
    bits
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn hash_deterministic() { assert_eq!(hash(b"hello"), hash(b"hello")); }
    #[test] fn hash_differs() { assert_ne!(hash(b"hello"), hash(b"world")); }
    #[test] fn meets_target() { assert!(meets(b"x", u64::MAX)); }
    #[test] fn not_meets() { assert!(!meets(b"hello", u64::MAX)); }
    #[test] fn difficulty() { assert!(difficulty_for(u64::MAX) > 0); }
}
