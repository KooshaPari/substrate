#[derive(Debug, Clone, PartialEq)]
pub enum ProviderHealth {
    Healthy,
    Degraded,
    Down,
}

#[derive(Debug, Clone)]
pub struct ProviderStat {
    pub name: String,
    pub health: ProviderHealth,
    pub request_count: u64,
    pub error_count: u64,
    pub avg_latency_ms: f64,
}

impl ProviderStat {
    pub fn error_rate(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.error_count as f64 / self.request_count as f64
        }
    }

    pub fn health_str(&self) -> &str {
        match self.health {
            ProviderHealth::Healthy => "healthy",
            ProviderHealth::Degraded => "degraded",
            ProviderHealth::Down => "down",
        }
    }
}

pub struct ProviderPanel {
    pub stats: Vec<ProviderStat>,
}

impl ProviderPanel {
    pub fn new() -> Self {
        Self { stats: vec![] }
    }

    pub fn add(&mut self, s: ProviderStat) {
        self.stats.push(s);
    }

    pub fn healthy_count(&self) -> usize {
        self.stats.iter().filter(|s| s.health == ProviderHealth::Healthy).count()
    }

    pub fn total_requests(&self) -> u64 {
        self.stats.iter().map(|s| s.request_count).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_rate_zero() {
        let s = ProviderStat {
            name: "a".into(),
            health: ProviderHealth::Healthy,
            request_count: 100,
            error_count: 0,
            avg_latency_ms: 10.0,
        };
        assert_eq!(s.error_rate(), 0.0);
    }

    #[test]
    fn error_rate_half() {
        let s = ProviderStat {
            name: "a".into(),
            health: ProviderHealth::Degraded,
            request_count: 10,
            error_count: 5,
            avg_latency_ms: 50.0,
        };
        assert!((s.error_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn healthy_count() {
        let mut p = ProviderPanel::new();
        p.add(ProviderStat {
            name: "a".into(),
            health: ProviderHealth::Healthy,
            request_count: 0,
            error_count: 0,
            avg_latency_ms: 0.0,
        });
        p.add(ProviderStat {
            name: "b".into(),
            health: ProviderHealth::Down,
            request_count: 0,
            error_count: 0,
            avg_latency_ms: 0.0,
        });
        assert_eq!(p.healthy_count(), 1);
    }

    #[test]
    fn total_requests() {
        let mut p = ProviderPanel::new();
        p.add(ProviderStat {
            name: "a".into(),
            health: ProviderHealth::Healthy,
            request_count: 50,
            error_count: 0,
            avg_latency_ms: 0.0,
        });
        p.add(ProviderStat {
            name: "b".into(),
            health: ProviderHealth::Healthy,
            request_count: 30,
            error_count: 0,
            avg_latency_ms: 0.0,
        });
        assert_eq!(p.total_requests(), 80);
    }

    #[test]
    fn health_str() {
        assert_eq!(ProviderHealth::Down, ProviderHealth::Down);
    }
}
