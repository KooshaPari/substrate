pub fn parse_name(data: &[u8; 100]) -> Option<String> {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    std::str::from_utf8(&data[..end]).ok().map(|s| s.to_string())
}
pub fn make_name(name: &str) -> [u8; 100] {
    let mut buf = [0u8; 100];
    let bytes = name.as_bytes();
    let len = bytes.len().min(99);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf
}
pub fn parse_octal(data: &[u8]) -> Option<u64> {
    let end = data.iter().position(|&b| b == 0 || b == b' ').unwrap_or(data.len());
    if end == 0 { return Some(0); }
    u64::from_str_radix(std::str::from_utf8(&data[..end]).ok()?, 8).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn name_roundtrip() { let b = make_name("test.sh"); assert_eq!(parse_name(&b), Some("test.sh".into())); }
    #[test] fn name_empty() { let b = [0u8; 100]; assert_eq!(parse_name(&b), Some("".into())); }
    #[test] fn octal() { let mut b = [0u8; 12]; b[..5].copy_from_slice(b"00065"); assert_eq!(parse_octal(&b), Some(0o65)); }
    #[test] fn octal_zero() { assert_eq!(parse_octal(&[0u8; 12]), Some(0)); }
    #[test] fn long_name_truncates() { let name = "x".repeat(150); let b = make_name(&name); assert_eq!(parse_name(&b), Some("x".repeat(99))); }
}
