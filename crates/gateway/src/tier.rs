#[derive(Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub enum Tier { Economy, Standard, Premium }

impl Tier {
    pub fn as_str(&self) -> &str {
        match self { Self::Economy=>"economy", Self::Standard=>"standard", Self::Premium=>"premium" }
    }
    pub fn rank(&self) -> u8 { match self { Self::Economy=>0, Self::Standard=>1, Self::Premium=>2 } }
}

#[derive(Debug,Clone)]
pub struct TierGate { pub default: Tier }

impl TierGate {
    pub fn new(default: Tier) -> Self { Self { default } }
    pub fn resolve(&self, requested: Option<Tier>) -> Tier { requested.unwrap_or_else(|| self.default.clone()) }
    pub fn allows(&self, requested: Tier, max: Tier) -> bool { requested <= max }
    pub fn escalation(&self, current: Tier, sla_violations: u32) -> Tier {
        if sla_violations > 5 { Tier::Premium }
        else if sla_violations > 2 && current == Tier::Economy { Tier::Standard }
        else { current }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn default_when_none() { assert_eq!(TierGate::new(Tier::Standard).resolve(None), Tier::Standard); }
    #[test] fn requested_wins() { assert_eq!(TierGate::new(Tier::Economy).resolve(Some(Tier::Premium)), Tier::Premium); }
    #[test] fn allows() { let g = TierGate::new(Tier::Standard); assert!(g.allows(Tier::Economy, Tier::Premium)); assert!(!g.allows(Tier::Premium, Tier::Economy)); }
    #[test] fn rank_ordering() { assert!(Tier::Premium.rank() > Tier::Economy.rank()); }
    #[test] fn escalation_severe() { assert_eq!(TierGate::new(Tier::Economy).escalation(Tier::Economy, 10), Tier::Premium); }
    #[test] fn escalation_mild() { assert_eq!(TierGate::new(Tier::Economy).escalation(Tier::Economy, 3), Tier::Standard); }
    #[test] fn tier_str() { assert_eq!(Tier::Economy.as_str(), "economy"); }
}