pub struct TarHeader { pub name: String, pub size: u64 }

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
    std::str::from_utf8(&data[..end]).ok()?.parse::<u64>().ok()
}

pub fn make_octal(value: u64, width: usize) -> Vec<u8> {
    let s = format!("{:0>width$o}", value, width = width.saturating_sub(1));
    let mut buf = vec![0u8; width];
    for (i, c) in s.bytes().rev().take(width).enumerate() {
        if i < buf.len() { buf[buf.len() - 1 - i] = c; }
    }
    buf
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parse_name() { let mut b = [0u8; 100]; b[..7].copy_from_slice(b"test.sh"); assert_eq!(parse_name(&b), Some("test.sh".into())); }
    #[test] fn make_name() { let b = make_name("hi.txt"); assert_eq!(parse_name(&b), Some("hi.txt".into())); }
    #[test] fn parse_octal() { let mut b = [0u8; 12]; b[..5].copy_from_slice(b"00065"); assert_eq!(parse_octal(&b), Some(0o65)); }
    #[test] fn make_octal() { let b = make_octal(8, 12); assert_eq!(parse_octal(&try_arr(b)).unwrap_or(0), 8); }
    #[test] fn empty_name() { let b = [0u8; 100]; assert_eq!(parse_name(&b), Some("".into())); }
}
fn try_arr<const N: usize>(v: Vec<u8>) -> Result<[u8; N], String> { if v.len() == N { let mut a = [0u8; N]; a.copy_from_slice(&v); Ok(a) } else { Err("wrong size".into()) } }
