pub fn encode_pem(label: &str, data: &[u8]) -> String {
    let mut s = String::new();
    s.push_str("-----BEGIN ");
    s.push_str(label);
    s.push_str("-----\n");
    let b64 = base64_encode(data);
    for chunk in b64.as_bytes().chunks(64) {
        s.push_str(std::str::from_utf8(chunk).unwrap());
        s.push('\n');
    }
    s.push_str("-----END ");
    s.push_str(label);
    s.push_str("-----\n");
    s
}
pub fn decode_pem(input: &str) -> Result<(String, Vec<u8>), String> {
    let begin_re = "-----BEGIN ";
    let end_re = "-----END ";
    let begin_idx = input.find(begin_re).ok_or("no BEGIN")?;
    let after_begin = &input[begin_idx + begin_re.len()..];
    let label_end = after_begin.find("-----").ok_or("no label end")?;
    let label = after_begin[..label_end].to_string();
    let after_label = &after_begin[label_end + 5..];
    let end_idx = after_label.find(end_re).ok_or("no END")?;
    let body = &after_label[..end_idx];
    let cleaned: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    Ok((label, base64_decode(&cleaned)?))
}
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let b = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        out.push(T[((b >> 6) & 0x3f) as usize] as char);
        out.push(T[(b & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let b = (data[i] as u32) << 16;
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        out.push(T[((b >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        let v: u32 = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' => continue,
            _ => return Err(format!("bad char {}", b as char)),
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_simple() {
        let data = b"hello world";
        let pem = encode_pem("TEST", data);
        let (label, decoded) = decode_pem(&pem).unwrap();
        assert_eq!(label, "TEST");
        assert_eq!(decoded, data);
    }
    #[test]
    fn round_trip_binary() {
        let data: Vec<u8> = (0..255).collect();
        let pem = encode_pem("BIN", &data);
        let (label, decoded) = decode_pem(&pem).unwrap();
        assert_eq!(label, "BIN");
        assert_eq!(decoded, data);
    }
    #[test]
    fn encoded_format() {
        let pem = encode_pem("LABEL", b"hi");
        assert!(pem.contains("-----BEGIN LABEL-----"));
        assert!(pem.contains("-----END LABEL-----"));
        assert!(pem.ends_with('\n'));
    }
    #[test]
    fn empty_data() {
        let pem = encode_pem("EMPTY", b"");
        let (label, decoded) = decode_pem(&pem).unwrap();
        assert_eq!(label, "EMPTY");
        assert_eq!(decoded, Vec::<u8>::new());
    }
    #[test]
    fn line_wrap_64() {
        // 100 bytes -> base64 is 136 chars -> 3 lines: 64, 64, 8
        let data = vec![0xab; 100];
        let pem = encode_pem("WRAP", &data);
        let body_lines: Vec<&str> = pem
            .lines()
            .skip_while(|l| !l.contains("BEGIN"))
            .skip(1)
            .take_while(|l| !l.contains("END"))
            .collect();
        assert!(body_lines.iter().all(|l| l.len() <= 64));
    }
    #[test]
    fn decode_missing_begin() {
        assert!(decode_pem("not pem").is_err());
    }
    #[test]
    fn whitespace_tolerant_decode() {
        let pem = encode_pem("WS", b"test data here");
        let mangled = pem.replace('\n', "  \n  ");
        let (_, decoded) = decode_pem(&mangled).unwrap();
        assert_eq!(decoded, b"test data here");
    }
}
