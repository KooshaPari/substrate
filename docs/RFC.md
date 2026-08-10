# RFC process

This document defines how substrate records decisions that need design review
before implementation. It complements, rather than replaces, GitHub issues,
pull requests, and architecture decision records (ADRs).

The process is deliberately lightweight: an RFC is a reviewable proposal, not
an approval gate for routine maintenance.

## When to write an RFC

Write an RFC before implementation when a change does one or more of the
following:

- adds or removes a public crate, CLI command, HTTP endpoint, protocol, or
  persistent-data format;
- changes a public API, wire contract, compatibility guarantee, or supported
  deployment model;
- crosses crate ownership boundaries or makes a durable architecture choice;
- introduces a significant security, privacy, cost, operational, or migration
  trade-off.

Do not require an RFC for a contained bug fix, documentation correction,
dependency patch, test-only change, or implementation detail that does not
change an agreed contract. Open an issue or pull request directly for those
changes.

## RFC lifecycle

1. **Draft** — Open a GitHub issue labelled `rfc` using the template below.
   State the problem and link prior issues, ADRs, experiments, or incidents.
2. **Review** — Affected maintainers review the proposal. Resolve substantive
   alternatives and record the chosen direction in the RFC issue.
3. **Accepted, declined, or superseded** — A maintainer records one of these
   outcomes in the issue. An accepted RFC is authority to implement its stated
   decision, not a release approval.
4. **Implement** — Link implementation pull requests to the accepted RFC and
   keep rollout and compatibility commitments current.
5. **Close** — Close the RFC when the decision is implemented or deliberately
   abandoned. If later work replaces it, link the successor RFC or ADR.

Urgent security or incident response may proceed first when delay would raise
risk. Record the decision retrospectively in an RFC or ADR once the system is
stable enough for review.

## RFC template

Use these headings in the RFC issue or in a linked Markdown document. Keep
each section concrete enough that an implementer and reviewer can tell what
will change and how success will be measured.

```markdown
# <short, decision-oriented title>

## Summary

One paragraph describing the decision being proposed.

## Motivation and goals

What user, operator, or maintainer problem exists? List measurable goals and
the crates, interfaces, or workflows affected.

## Non-goals

State work that this RFC intentionally excludes.

## Proposal

Describe the design, contract changes, failure handling, ownership boundaries,
and any operational or security consequences. Link prior art and relevant
issues.

## Alternatives considered

Describe credible alternatives, including keeping the current design, and why
the proposal is preferred.

## Compatibility and migration

Identify public API, protocol, configuration, data, and deployment impacts.
Specify a migration, rollback, or explicit statement that none is needed.

## Test and rollout plan

List the unit, integration, conformance, operational, and documentation checks
that demonstrate the change is safe. Include rollout observability and abort
conditions when the change reaches a running system.

## Open questions

List decisions that must be resolved before acceptance. Remove or explicitly
defer each item when the RFC is accepted.
```

## Decision records (ADRs)

An RFC proposes a change; an ADR records a durable architectural decision.
When an accepted RFC makes a long-lived architecture choice, add or update an
ADR under `docs/adr/` using the repository's existing status, context,
decision, consequences, alternatives, and references structure. Link the RFC
from the ADR and the ADR from the RFC issue.

Small or reversible RFCs do not need an ADR. Conversely, a focused ADR may be
written without an RFC when its decision is narrow and already settled through
normal pull-request review.

## Links and ownership

- [Contributing guide](../CONTRIBUTING.md) — issue labels, pull-request
  expectations, and release/security reporting paths.
- [`docs/adr/`](adr/) — canonical architectural decision records.
- [Feature request template](../.github/ISSUE_TEMPLATE/feature_request.yml) —
  required problem, scope, compatibility, rollout, and test-plan prompts.

Maintainers responsible for the affected crate or surface own the review. Use
the repository's CODEOWNERS rules and pull-request template to identify and
include them.
