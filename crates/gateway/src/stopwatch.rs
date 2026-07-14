use std::time::{Duration, Instant};
pub struct Stopwatch {
    start: Instant,
    laps: Vec<Duration>,
}
impl Stopwatch {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            laps: Vec::new(),
        }
    }
    pub fn lap(&mut self) -> Duration {
        let now = Instant::now();
        let d = now.duration_since(self.start);
        self.laps.push(d);
        self.start = now;
        d
    }
    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.start) + self.laps.iter().sum::<Duration>()
    }
    pub fn lap_count(&self) -> usize {
        self.laps.len()
    }
    pub fn reset(&mut self) {
        self.start = Instant::now();
        self.laps.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starts() {
        let s = Stopwatch::new();
        assert_eq!(s.lap_count(), 0);
    }
    #[test]
    fn lap_increments() {
        let mut s = Stopwatch::new();
        s.lap();
        s.lap();
        assert_eq!(s.lap_count(), 2);
    }
    #[test]
    fn reset() {
        let mut s = Stopwatch::new();
        s.lap();
        s.reset();
        assert_eq!(s.lap_count(), 0);
    }
    #[test]
    fn elapsed_nonzero() {
        std::thread::sleep(Duration::from_millis(2));
        let s = Stopwatch::new();
        assert!(s.elapsed() > Duration::ZERO);
    }
}
