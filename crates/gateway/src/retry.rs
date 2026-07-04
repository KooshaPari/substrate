use std::time::Duration;
#[derive(Clone,Debug)]
pub struct RetryConfig { pub max_attempts:u32, pub base_ms:u64, pub max_ms:u64, pub retryable:Vec<u16> }
impl Default for RetryConfig { fn default()->Self { Self{max_attempts:3,base_ms:100,max_ms:5000,retryable:vec![429,500,502,503,504]} } }
impl RetryConfig {
  pub fn should_retry(&self,attempt:u32,status:u16)->bool { attempt<self.max_attempts && self.retryable.contains(&status) }
  pub fn delay(&self,attempt:u32)->Duration { Duration::from_millis((self.base_ms*2u64.pow(attempt)).min(self.max_ms)) }
}
#[cfg(test)]
mod tests {
  use super::*;
  #[test] fn no_retry_past_max(){ assert!(!RetryConfig::default().should_retry(3,500)); }
  #[test] fn retry_429(){ assert!(RetryConfig::default().should_retry(0,429)); }
  #[test] fn no_retry_404(){ assert!(!RetryConfig::default().should_retry(0,404)); }
  #[test] fn delay_doubles(){ let c=RetryConfig::default(); assert_eq!(c.delay(0).as_millis(),100); assert_eq!(c.delay(1).as_millis(),200); }
  #[test] fn delay_capped(){ let c=RetryConfig{base_ms:1000,max_ms:2000,..Default::default()}; assert_eq!(c.delay(5).as_millis(),2000); }
}