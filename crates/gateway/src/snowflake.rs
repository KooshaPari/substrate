use std::time::{SystemTime, UNIX_EPOCH};

pub struct Snowflake {
    epoch_ms: u64,
    last_ts: u64,
    sequence: u16,
    node: u16,
}
impl Snowflake {
    pub fn new(epoch_ms: u64, node: u16) -> Self {
        Self {
            epoch_ms,
            last_ts: 0,
            sequence: 0,
            node,
        }
    }
    pub fn next_id(&mut self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0) as u64;
        if now < self.last_ts {
            return 0;
        }
        if now == self.last_ts {
            self.sequence = (self.sequence + 1) & 0xfff;
            if self.sequence == 0 {
                while SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0) as u64
                    <= self.last_ts
                {}
            }
        } else {
            self.sequence = 0;
        }
        self.last_ts = now;
        ((now - self.epoch_ms) << 22) | ((self.node as u64) << 12) | self.sequence as u64
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unique() {
        let mut sf = Snowflake::new(0, 1);
        let a = sf.next_id();
        let b = sf.next_id();
        assert_ne!(a, b);
    }
    #[test]
    fn has_node() {
        let mut sf = Snowflake::new(0, 42);
        let id = sf.next_id();
        assert_eq!((id >> 12) & 0x3ff, 42);
    }
    #[test]
    fn sequence_increments() {
        let mut sf = Snowflake::new(0, 0);
        let a = sf.next_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = sf.next_id();
        assert!(a < b);
    }
}
