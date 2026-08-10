#[derive(Debug, Clone, PartialEq)]
pub enum SlaStatus {
    Green,
    Yellow,
    Red,
}
#[derive(Debug, Clone)]
pub struct SlaEntry {
    pub label: String,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}
impl SlaEntry {
    pub fn status(&self, target: f64) -> SlaStatus {
        if self.p99_ms <= target {
            SlaStatus::Green
        } else if self.p99_ms <= target * 1.5 {
            SlaStatus::Yellow
        } else {
            SlaStatus::Red
        }
    }
    pub fn summary(&self) -> String {
        format!(
            "p50={:.0}ms p95={:.0}ms p99={:.0}ms",
            self.p50_ms, self.p95_ms, self.p99_ms
        )
    }
}
pub struct SlaPanel {
    pub entries: Vec<SlaEntry>,
    pub target_p99_ms: f64,
}
impl SlaPanel {
    pub fn new(t: f64) -> Self {
        Self {
            entries: vec![],
            target_p99_ms: t,
        }
    }
    pub fn add(&mut self, e: SlaEntry) {
        self.entries.push(e);
    }
    pub fn violations(&self) -> Vec<&SlaEntry> {
        self.entries
            .iter()
            .filter(|e| e.status(self.target_p99_ms) == SlaStatus::Red)
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn green() {
        assert_eq!(
            SlaEntry {
                label: "".into(),
                p50_ms: 5.0,
                p95_ms: 50.0,
                p99_ms: 90.0
            }
            .status(100.0),
            SlaStatus::Green
        );
    }
    #[test]
    fn yellow() {
        assert_eq!(
            SlaEntry {
                label: "".into(),
                p50_ms: 50.0,
                p95_ms: 100.0,
                p99_ms: 130.0
            }
            .status(100.0),
            SlaStatus::Yellow
        );
    }
    #[test]
    fn red() {
        assert_eq!(
            SlaEntry {
                label: "".into(),
                p50_ms: 100.0,
                p95_ms: 200.0,
                p99_ms: 200.0
            }
            .status(100.0),
            SlaStatus::Red
        );
    }
    #[test]
    fn violations_count() {
        let mut p = SlaPanel::new(100.0);
        p.add(SlaEntry {
            label: "ok".into(),
            p50_ms: 5.0,
            p95_ms: 50.0,
            p99_ms: 80.0,
        });
        p.add(SlaEntry {
            label: "bad".into(),
            p50_ms: 100.0,
            p95_ms: 200.0,
            p99_ms: 300.0,
        });
        assert_eq!(p.violations().len(), 1);
    }
    #[test]
    fn summary() {
        assert!(SlaEntry {
            label: "".into(),
            p50_ms: 5.0,
            p95_ms: 20.0,
            p99_ms: 50.0
        }
        .summary()
        .contains("p50="));
    }
}
