pub fn format_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = n as f64;
    let mut idx = 0;
    while size >= 1024.0 && idx < UNITS.len() - 1 { size /= 1024.0; idx += 1; }
    if idx == 0 { format!("{} {}", n, UNITS[0]) } else { format!("{:.2} {}", size, UNITS[idx]) }
}

pub fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num_str, unit) = if let Some(pos) = s.find(char::is_alphabetic) {
        (s[..pos].trim(), s[pos..].trim().to_uppercase())
    } else { (s, "B".to_string()) };
    let n: f64 = num_str.parse().ok()?;
    let multiplier: u64 = match unit.as_str() {
        "B" => 1, "KB" => 1024, "MB" => 1024 * 1024,
        "GB" => 1024u64.pow(3), "TB" => 1024u64.pow(4),
        _ => return None,
    };
    Some((n * multiplier as f64) as u64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn fmt_b() { assert_eq!(format_bytes(512), "512 B"); }
    #[test] fn fmt_kb() { assert_eq!(format_bytes(2048), "2.00 KB"); }
    #[test] fn fmt_mb() { assert_eq!(format_bytes(1024 * 1024 * 5), "5.00 MB"); }
    #[test] fn fmt_gb() { assert_eq!(format_bytes(1024u64.pow(3)), "1.00 GB"); }
    #[test] fn parse_b() { assert_eq!(parse_bytes("512 B"), Some(512)); }
    #[test] fn parse_kb() { assert_eq!(parse_bytes("2 KB"), Some(2048)); }
    #[test] fn parse_mb() { assert_eq!(parse_bytes("5 MB"), Some(1024 * 1024 * 5)); }
    #[test] fn parse_invalid() { assert_eq!(parse_bytes("xyz"), None); }
}
