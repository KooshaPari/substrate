#[derive(Debug,Clone,PartialEq)]
pub enum ProviderHealth { Healthy, Degraded, Down }

#[derive(Debug,Clone)]
pub struct ProviderStat {
    pub name: String,
    pub health: ProviderHealth,
    pub requests: u64,
    pub errors: u64,
    pub avg_latency_ms: f64,
}

impl ProviderStat {
    pub fn error_rate(&self) -> f64 {
        if self.requests == 0 { 0.0 } else { self.errors as f64 / self.requests as f64 }
    }
    pub fn is_healthy(&self) -> bool { self.health == ProviderHealth::Healthy }
    pub fn health_label(&self) -> &str {
        match self.health { ProviderHealth::Healthy=>"healthy", ProviderHealth::Degraded=>"degraded", ProviderHealth::Down=>"down" }
    }
}

pub struct ProviderPanel { pub providers: Vec<ProviderStat> }
impl ProviderPanel {
    pub fn new() -> Self { Self { providers: vec![] } }
    pub fn push(&mut self, s: ProviderStat) { self.providers.push(s); }
    pub fn healthy_count(&self) -> usize { self.providers.iter().filter(|p| p.is_healthy()).count() }
    pub fn total_requests(&self) -> u64 { self.providers.iter().map(|p| p.requests).sum() }
}
impl Default for ProviderPanel { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn error_rate_zero() { let s=ProviderStat{name:"a".into(),health:ProviderHealth::Healthy,requests:0,errors:0,avg_latency_ms:0.0}; assert_eq!(s.error_rate(),0.0); }
    #[test] fn error_rate_10pct() { let s=ProviderStat{name:"b".into(),health:ProviderHealth::Degraded,requests:100,errors:10,avg_latency_ms:50.0}; assert!((s.error_rate()-0.1).abs()<1e-10); }
    #[test] fn healthy_count() { let mut p=ProviderPanel::new(); p.push(ProviderStat{name:"a".into(),health:ProviderHealth::Healthy,requests:0,errors:0,avg_latency_ms:0.0}); p.push(ProviderStat{name:"b".into(),health:ProviderHealth::Down,requests:0,errors:0,avg_latency_ms:0.0}); assert_eq!(p.healthy_count(),1); }
    #[test] fn total_reqs() { let mut p=ProviderPanel::new(); p.push(ProviderStat{name:"x".into(),health:ProviderHealth::Healthy,requests:40,errors:0,avg_latency_ms:0.0}); p.push(ProviderStat{name:"y".into(),health:ProviderHealth::Healthy,requests:60,errors:0,avg_latency_ms:0.0}); assert_eq!(p.total_requests(),100); }
    #[test] fn health_label() { assert_eq!(ProviderHealth::Down,ProviderHealth::Down); let s=ProviderStat{name:"d".into(),health:ProviderHealth::Down,requests:0,errors:0,avg_latency_ms:0.0}; assert_eq!(s.health_label(),"down"); }
}
