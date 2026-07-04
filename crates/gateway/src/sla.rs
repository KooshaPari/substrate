//! SLA latency tier checking: P50/P95/P99 violation detection.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SlaViolationTier {
    P50,
    P95,
    P99,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlaViolation {
    pub tier: SlaViolationTier,
    pub actual_ms: u64,
    pub target_ms: u64,
    pub provider: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlaConfig {
    #[serde(default = "SlaConfig::d_p50")]
    pub p50_ms: u64,
    #[serde(default = "SlaConfig::d_p95")]
    pub p95_ms: u64,
    #[serde(default = "SlaConfig::d_p99")]
    pub p99_ms: u64,
}
impl SlaConfig {
    fn d_p50() -> u64 {
        200
    }
    fn d_p95() -> u64 {
        500
    }
    fn d_p99() -> u64 {
        1000
    }
}
impl Default for SlaConfig {
    fn default() -> Self {
        Self {
            p50_ms: 200,
            p95_ms: 500,
            p99_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SlaChecker {
    config: SlaConfig,
}
impl SlaChecker {
    pub fn new(config: SlaConfig) -> Self {
        Self { config }
    }
    pub fn check(&self, provider: &str, latency_ms: u64) -> Vec<SlaViolation> {
        let mut v = Vec::new();
        if latency_ms > self.config.p99_ms {
            v.push(SlaViolation {
                tier: SlaViolationTier::P99,
                actual_ms: latency_ms,
                target_ms: self.config.p99_ms,
                provider: provider.to_string(),
            });
        }
        if latency_ms > self.config.p95_ms {
            v.push(SlaViolation {
                tier: SlaViolationTier::P95,
                actual_ms: latency_ms,
                target_ms: self.config.p95_ms,
                provider: provider.to_string(),
            });
        }
        if latency_ms > self.config.p50_ms {
            v.push(SlaViolation {
                tier: SlaViolationTier::P50,
                actual_ms: latency_ms,
                target_ms: self.config.p50_ms,
                provider: provider.to_string(),
            });
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c() -> SlaChecker {
        SlaChecker::default()
    }
    #[test]
    fn clean() {
        assert!(c().check("p", 100).is_empty());
    }
    #[test]
    fn p50_only() {
        let v = c().check("p", 250);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].tier, SlaViolationTier::P50);
    }
    #[test]
    fn p95_includes_p50() {
        let v = c().check("p", 600);
        assert_eq!(v.len(), 2);
        assert!(v.iter().any(|x| x.tier == SlaViolationTier::P95));
    }
    #[test]
    fn p99_all_three() {
        assert_eq!(c().check("p", 1500).len(), 3);
    }
    #[test]
    fn boundary_clean() {
        assert!(c().check("p", 200).is_empty());
        assert!(c().check("p", 100).is_empty());
    }
    #[test]
    fn custom_config() {
        let ch = SlaChecker::new(SlaConfig {
            p50_ms: 100,
            p95_ms: 300,
            p99_ms: 800,
        });
        assert_eq!(ch.check("p", 150).len(), 1);
    }
    #[test]
    fn provider_in_violation() {
        assert_eq!(c().check("openai", 300)[0].provider, "openai");
    }
    #[test]
    fn zero_latency_clean() {
        assert!(c().check("p", 0).is_empty());
    }
}
