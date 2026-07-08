pub struct Rgb { pub r: u8, pub g: u8, pub b: u8 }
pub fn hex_to_rgb(s: &str) -> Option<Rgb> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 { return None; }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Rgb { r, g, b })
}
pub fn rgb_to_hex(c: Rgb) -> String { format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b) }
pub fn luminance(c: Rgb) -> f64 {
    let r = c.r as f64 / 255.0; let g = c.g as f64 / 255.0; let b = c.b as f64 / 255.0;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}
pub fn is_dark(c: Rgb) -> bool { luminance(c) < 0.5 }
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn hex_to_rgb_valid() { let c = hex_to_rgb("#ff0080").unwrap(); assert_eq!(c.r, 255); assert_eq!(c.g, 0); assert_eq!(c.b, 128); }
    #[test] fn hex_to_rgb_no_hash() { let c = hex_to_rgb("00ff00").unwrap(); assert_eq!(c.g, 255); }
    #[test] fn hex_to_rgb_invalid() { assert!(hex_to_rgb("xyz").is_none()); assert!(hex_to_rgb("#abc").is_none()); }
    #[test] fn rgb_to_hex_roundtrip() { assert_eq!(rgb_to_hex(Rgb { r: 0x12, g: 0x34, b: 0x56 }), "123456"); }
    #[test] fn dark_vs_light() { assert!(is_dark(Rgb { r: 0, g: 0, b: 0 })); assert!(!is_dark(Rgb { r: 255, g: 255, b: 255 })); }
    #[test] fn luminance_ordering() { assert!(luminance(Rgb { r: 0, g: 0, b: 0 }) < luminance(Rgb { r: 128, g: 128, b: 128 })); }
}
