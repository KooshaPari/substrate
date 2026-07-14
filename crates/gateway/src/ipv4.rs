pub fn parse(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut result = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        let n: u8 = p.parse().ok()?;
        result[i] = n;
    }
    Some(result)
}

pub fn to_string(ip: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

pub fn is_private(ip: [u8; 4]) -> bool {
    if ip[0] == 10 {
        return true;
    }
    if ip[0] == 172 && (16..=31).contains(&ip[1]) {
        return true;
    }
    if ip[0] == 192 && ip[1] == 168 {
        return true;
    }
    if ip[0] == 127 {
        return true;
    }
    false
}

pub fn is_loopback(ip: [u8; 4]) -> bool {
    ip[0] == 127
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_valid() {
        assert_eq!(parse("192.168.1.1"), Some([192, 168, 1, 1]));
    }
    #[test]
    fn test_parse_zero() {
        assert_eq!(parse("0.0.0.0"), Some([0, 0, 0, 0]));
    }
    #[test]
    fn test_parse_invalid() {
        assert_eq!(parse("256.0.0.0"), None);
        assert_eq!(parse("1.1.1"), None);
    }
    #[test]
    fn test_to_string() {
        assert_eq!(to_string([10, 0, 0, 1]), "10.0.0.1");
    }
    #[test]
    fn test_private() {
        assert!(is_private([10, 0, 0, 1]));
        assert!(is_private([192, 168, 1, 1]));
        assert!(!is_private([8, 8, 8, 8]));
    }
    #[test]
    fn test_loopback() {
        assert!(is_loopback([127, 0, 0, 1]));
        assert!(!is_loopback([128, 0, 0, 1]));
    }
}
