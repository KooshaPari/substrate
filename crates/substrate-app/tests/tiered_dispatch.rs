use substrate_app::tiered_dispatch::{dispatch_with_reroute, TieredDispatchOutcome};
use substrate_core::{Result, SubstrateError, Tier};

#[test]
fn reroute_escalates_until_dispatch_succeeds() {
    let mut attempts = Vec::new();

    let outcome = dispatch_with_reroute(Tier::Worker, |tier| {
        attempts.push(tier);
        match tier {
            Tier::Worker => Err(SubstrateError::Engine("worker failed".into())),
            Tier::Main => Ok("main output".to_string()),
            Tier::Heavy => Ok("heavy output".to_string()),
        }
    })
    .unwrap();

    assert_eq!(attempts, vec![Tier::Worker, Tier::Main]);
    assert_eq!(
        outcome,
        TieredDispatchOutcome {
            succeeded_tier: Tier::Main,
            output: "main output".to_string(),
        }
    );
}

#[test]
fn reroute_treats_empty_output_as_failure() {
    let mut attempts = Vec::new();

    let outcome = dispatch_with_reroute(Tier::Worker, |tier| {
        attempts.push(tier);
        match tier {
            Tier::Worker => Ok("".to_string()),
            Tier::Main => Ok("   ".to_string()),
            Tier::Heavy => Ok("heavy output".to_string()),
        }
    })
    .unwrap();

    assert_eq!(attempts, vec![Tier::Worker, Tier::Main, Tier::Heavy]);
    assert_eq!(outcome.succeeded_tier, Tier::Heavy);
    assert_eq!(outcome.output, "heavy output");
}

#[test]
fn reroute_returns_last_failure_when_all_tiers_fail() {
    let result: Result<TieredDispatchOutcome> = dispatch_with_reroute(Tier::Main, |tier| {
        Err(SubstrateError::Engine(format!("{tier} failed")))
    });

    let error = result.unwrap_err().to_string();
    assert!(error.contains("heavy failed"));
}
