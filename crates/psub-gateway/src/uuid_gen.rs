use std::time::{SystemTime, UNIX_EPOCH};

pub fn v7() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let nanos_total = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let part_a = (ms as u64 & 0xffffffff) as u32;
    let part_b = ((nanos_total & 0xffff) as u16) | 0x7000;
    let part_c = ((nanos_total >> 16) & 0xffff) as u16;
    let part_d = ((nanos_total >> 32) & 0xffff) as u16;
    let part_e = ((ms as u64 >> 32) & 0xffffffff) as u32;
    format!("{:08x}-{:04x}-{:04x}-{:04x}-{:08x}{:04x}", part_a, part_b, part_c, part_d, part_e, part_d)
}

pub fn v4() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros()).unwrap_or(0);
    let part_a = (ms as u64 & 0xffffffff) as u32;
    let part_b = (((ms >> 32) & 0xffff) as u16) | 0x4000;
    let part_c = ((ms >> 48) & 0xffff) as u16 | 0x8000;
    let part_d = ((ms >> 16) & 0xffff) as u16;
    let part_e = ((ms as u64 >> 32) & 0xffffffff) as u32;
    format!("{:08x}-{:04x}-{:04x}-{:04x}-{:08x}{:04x}", part_a, part_b, part_c, part_d, part_e, part_d)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn v7_format() { let id = v7(); assert_eq!(id.len(), 36); }
    #[test] fn v7_unique() { std::thread::sleep(std::time::Duration::from_millis(2)); let a = v7(); std::thread::sleep(std::time::Duration::from_millis(2)); let b = v7(); assert_ne!(a, b); }
    #[test] fn v4_format() { let id = v4(); assert_eq!(id.len(), 36); }
    #[test] fn v4_unique() { std::thread::sleep(std::time::Duration::from_millis(2)); let a = v4(); std::thread::sleep(std::time::Duration::from_millis(2)); let b = v4(); assert_ne!(a, b); }
}
