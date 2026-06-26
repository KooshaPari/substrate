# cloud-dispatch-conformance

Reusable contract tests for [`CloudDispatchPort`](https://docs.rs/substrate-core/latest/substrate_core/cloud_dispatch_port/trait.CloudDispatchPort.html) adapters.

Call [`assert_cloud_dispatch_conformance`] from any adapter crate's test suite, or use the bundled [`FakeCloudDispatch`] for offline xDD tests.

## Scenarios

1. **Happy path** — submit → poll until `Succeeded` → harvest returns PR metadata.
2. **Failed task** — submit → poll reaches `Failed`; harvest errors.
