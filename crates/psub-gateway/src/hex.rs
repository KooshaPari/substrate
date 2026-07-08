pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 { return Err("hex string must have even length".into()); }
    let mut out = Vec::with_capacity(s.len() / 2);
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let high = hex_digit(chars[i])?;
        let low = hex_digit(chars[i + 1])?;
        out.push((high << 4) | low);
        i += 2;
    }
    Ok(out)
}

fn hex_digit(c: char) -> Result<u8, String> {
    match c {
        '0'..='9' => Ok(c as u8 - b'0'),
        'a'..='f' => Ok(c as u8 - b'a' + 10),
        'A'..='F' => Ok(c as u8 - b'A' + 10),
        _ => Err(format!("invalid hex char: {}", c)),
    }
}

pub fn is_valid(s: &str) -> bool { decode(s).is_ok() }
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_encode() { assert_eq!(encode(&[0x12, 0xab, 0xff]), "12abff"); }
    #[test] fn test_encode_empty() { assert_eq!(encode(&[]), ""); }
    #[test] fn test_decode() { assert_eq!(decode("12abff").unwrap(), vec![0x12, 0xab, 0xff]); }
    #[test] fn test_decode_upper() { assert_eq!(decode("DEADBE").unwrap(), vec![0xde, 0xad, 0xbe]); }
    #[test] fn test_decode_odd_len() { assert!(decode("abc").is_err()); }
    #[test] fn test_decode_invalid_char() { assert!(decode("zz").is_err()); }
    #[test] fn test_is_valid() { assert!(is_valid("aabbcc")); assert!(!is_valid("xyz")); }
}
