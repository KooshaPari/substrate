use std::time::{SystemTime, UNIX_EPOCH};

pub fn v7() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let ts_high = (ms >> 16) as u16;
    let ts_mid = (ms >> 8) as u16;
    let ts_low = (ms & 0xff) as u16;
    let rand_a = (nanos & 0xff) as u16;
    let rand_b = ((nanos >> 8) ^ 0xabcd) as u16;
    format!("{:04x}{:04x}-{:04x}-7{:03x}-{:04x}-{:04x}{:04x}",
        ts_high, ts_mid, ts_low, rand_a & 0x0fff, rand_b, rand_a)
}

pub fn v4() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros()).unwrap_or(0);
    let a = ((ms >> 32) as u32) as u16;
    let b = ((ms >> 16) as u32) as u16;
    let c = (ms as u32) as u16;
    let d = ((ms >> 48) as u32) as u16;
    format!("{:04x}{:04x}-4{:03x}-{:04x}-{:04x}-{:04x}{:04x}",
        a, b, b & 0x0fff, c, d, a)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn v7_format() { let id = v7(); assert_eq!(id.len(), 36); assert_eq!(id.chars().nth(14), Some('7')); }
    #[test] fn v7_unique() { std::thread::sleep(std::time::Duration::from_millis(2)); let a = v7(); std::thread::sleep(std::time::Duration::from_millis(2)); let b = v7(); assert_ne!(a, b); }
    #[test] fn v4_format() { let id = v4(); assert_eq!(id.len(), 36); assert_eq!(id.chars().nth(14), Some('4')); }
    #[test] fn v4_unique() { std::thread::sleep(std::time::Duration::from_millis(2)); let a = v4(); std::thread::sleep(std::time::Duration::from_millis(2)); let b = v4(); assert_ne!(a, b); }
}
