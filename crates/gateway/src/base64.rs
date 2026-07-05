const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(input: &[u8]) -> String {
    let mut out = String::new();
    for c in input.chunks(3) {
        let b0 = c[0];
        let b1 = c.get(1).copied().unwrap_or(0);
        let b2 = c.get(2).copied().unwrap_or(0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        let idx = |shift: u32, mask: u32| ((n >> shift) & mask) as usize;
        out.push(ALPHA[idx(18, 0x3f)] as char);
        out.push(ALPHA[idx(12, 0x3f)] as char);
        if c.len() > 1 { out.push(ALPHA[idx(6, 0x3f)] as char); } else { out.push('='); }
        if c.len() > 2 { out.push(ALPHA[idx(0, 0x3f)] as char); } else { out.push('='); }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn empty() { assert_eq!(encode(b""), ""); }
    #[test] fn hi() { assert_eq!(encode(b"hi"), "aGk="); }
    #[test] fn abc() { assert_eq!(encode(b"abc"), "YWJj"); }
    #[test] fn a() { assert_eq!(encode(b"a"), "YQ=="); }
    #[test] fn ab() { assert_eq!(encode(b"ab"), "YWI="); }
}
