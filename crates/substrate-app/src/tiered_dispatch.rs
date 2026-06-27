//! Tiered dispatch with deterministic reroute-up-on-failure.

use std::future::Future;

use substrate_core::error::{Result, SubstrateError};
use substrate_core::Tier;

/// Successful tiered dispatch result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TieredDispatchOutcome {
    /// Tier that produced non-empty output.
    pub succeeded_tier: Tier,
    /// Captured dispatch output.
    pub output: String,
}

/// Dispatch starting at `start_tier`, escalating Worker -> Main -> Heavy on failure.
///
/// Failures include closure errors and successful-but-empty output. The function
/// is pure apart from the supplied closure, making retry order easy to unit test.
pub fn dispatch_with_reroute<F>(start_tier: Tier, mut dispatch: F) -> Result<TieredDispatchOutcome>
where
    F: FnMut(Tier) -> Result<String>,
{
    let mut tier = start_tier;

    loop {
        let error = match dispatch(tier) {
            Ok(output) if !output.trim().is_empty() => {
                return Ok(TieredDispatchOutcome {
                    succeeded_tier: tier,
                    output,
                });
            }
            Ok(_) => SubstrateError::Engine(format!("{tier} dispatch returned empty output")),
            Err(error) => error,
        };

        match tier.escalate() {
            Some(next) => tier = next,
            None => return Err(error),
        }
    }
}

/// Async variant of [`dispatch_with_reroute`] for subprocess or network adapters.
pub async fn dispatch_with_reroute_async<F, Fut>(
    start_tier: Tier,
    mut dispatch: F,
) -> Result<TieredDispatchOutcome>
where
    F: FnMut(Tier) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let mut tier = start_tier;

    loop {
        let error = match dispatch(tier).await {
            Ok(output) if !output.trim().is_empty() => {
                return Ok(TieredDispatchOutcome {
                    succeeded_tier: tier,
                    output,
                });
            }
            Ok(_) => SubstrateError::Engine(format!("{tier} dispatch returned empty output")),
            Err(error) => error,
        };

        match tier.escalate() {
            Some(next) => tier = next,
            None => return Err(error),
        }
    }
}
