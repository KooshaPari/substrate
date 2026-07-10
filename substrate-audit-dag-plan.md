# Substrate Audit — DAG / WBS / Rubric Plan

> **Audit baseline:** kooshapari/substrate @ `cb9a3e7` (psub- prefix rename)
> **Audit date:** 2026-07-08
> **Current score:** **82.5 / 100 (Grade B+)** across 140 pillars
> **Remediation:** Main track complete (Phase0 + Phase1 + Phase2).
> **Remaining gap:** 17.5 points (107 satisfied, 17 partial, 16 missing)
> **Companion artifacts:** [`audit_scorecard.json`](./audit_scorecard.json) (per-pillar verdicts)
>
> **Outstanding PRs:** None — all changes uncommitted in working tree.

---

## 1. Executive Summary

Substrate is a 44-crate Rust workspace that implements an agent-orchestration
gateway (OpenAI-compatible HTTP + A2A + adapter drivers). The codebase is
**architecturally mature** (hexagonal ports/adapters, MSRV pinned, clippy
strict) but the **engineering hygiene layer is thin** — no `deny.toml`,
no `CODEOWNERS`, no `CONTRIBUTING.md`/`ARCHITECTURE.md`, no fuzz targets,
no `/metrics` endpoint, no OpenTelemetry.

The path from **57.8 → 100** is mechanical but volume-heavy. Estimated effort:
~3–4 calendar weeks of focused work by one engineer, or ~1.5 weeks with two.

| Pillar count | Baseline (cb9a3e7) | After remediation | Target |
|---|---|---|---|
| Satisfied | 81 | **107** | **140** |
| Partial | 32 | **17** | 0 |
| Missing | 27 | **16** | 0 |
| **Score %** | **57.8** | **82.5** | **100.0** |

---

## 2. Rubric

**Grading scale (per pillar):**
- **Satisfied** — implemented, verifiable, no caveats
- **Partial** — implemented but with known caveats (env-gated, partial coverage,
  warn-only, partial enforcement)
- **Missing** — no implementation found

**Domain weights (sum to 100):**

| Domain | Pillars | Weight | Current score | Weighted contribution |
|---|---:|---:|---:|---:|
| code_quality | 18 | 12 | 86.1 | 10.3 |
| architecture | 17 | 12 | 82.4 | 9.9 |
| testing | 13 | 11 | 65.4 | 7.2 |
| observability | 9 | 9 | 50.0 | 4.5 |
| security | 16 | 14 | 65.6 | 9.2 |
| documentation | 17 | 12 | 47.1 | 5.7 |
| ci_cd | 14 | 10 | 78.6 | 7.9 |
| supply_chain | 10 | 9 | 70.0 | 6.3 |
| release_engineering | 13 | 6 | 61.5 | 3.7 |
| dx | 13 | 5 | 69.2 | 3.5 |
| **TOTAL** | **140** | **100** |   | **58.2** |

(Rounded report score 57.8 reflects per-pillar floor rounding; weighted ≈ 58.2.)

---

## 3. Phased DAG — work breakdown structure

### Phase 0 — Hygiene Backbone (1 day, → 73.6 / B grade)

DAG edges: each Phase 0 item blocks several Phase 1 / Phase 2 items.

```
[P0.1 deny.toml] ──┐
                   ├──▶ [P0.2 cargo-deny in CI] ──┐
[P0.3 CODEOWNERS] ─┤                              │
                   │                              ▼
[P0.4 CONTRIBUTING.md] ─┐                  [Phase 1 starts]
                        ├──▶ [P0.6 ADR-0002..0006] ──┘
[P0.5 ARCHITECTURE.md] ─┘
```

| ID | Title | Files | Est | Verifies |
|---|---|---|---:|---|
| **P0.1** | Add `deny.toml` | `deny.toml` (new) | 30m | SC-02, SC-03, SC-04, SC-05, SEC-05 |
| **P0.2** | Run `cargo deny` in CI | `.github/workflows/ci.yml` | 20m | CI-06, SEC-06 |
| **P0.3** | Add `.github/CODEOWNERS` | `.github/CODEOWNERS` (new) | 15m | CI-13 |
| **P0.4** | Add `CONTRIBUTING.md` | `CONTRIBUTING.md` (new) | 1h | DOC-05 |
| **P0.5** | Add `ARCHITECTURE.md` | `ARCHITECTURE.md` (new, psub- paths) | 1.5h | ARCH-05, DOC-04 |
| **P0.6** | Foundational ADRs (5) | `docs/adr/0002..0006` | 2h | ARCH-06, DOC-06 |

**Phase 0 exit criteria:**
- `cargo deny check` passes locally with the new config
- CI fails when a deny violation is introduced
- `docs/adr/` contains 6 accepted ADRs
- A new contributor can answer "where does the gateway end and the core begin?" by reading `ARCHITECTURE.md`

### Phase 1 — Observability, docs, fuzz, coverage (1 week, → 86.4 / B+ grade)

```
[P1.1 tracing init] ─┐
                     ├──▶ [P1.3 OTel exporter] (optional)
[P1.2 /metrics]   ───┤
                     │
[P1.4 OpenAPI spec] ──┤──▶ [P1.5 fuzz targets]
                     │
[P1.6 coverage CI] ───┤
                     │
[P1.7 dependabot]  ───┘
```

| ID | Title | Pillars fixed | Est |
|---|---|---|---:|
| **P1.1** | `tracing-subscriber` init + `#[tracing::instrument]` on gateway hot path (`crates/psub-gateway/src/lib.rs`) | OBS-02, OBS-03 | 0.5d |
| **P1.2** | Prometheus `/metrics` endpoint in `crates/psub-gateway/src/metrics.rs` + `axum_prometheus` | OBS-05 | 0.5d |
| **P1.3** | OpenTelemetry exporter (`opentelemetry-otlp`) | OBS-04, OBS-07 | 1d (optional) |
| **P1.4** | OpenAPI spec for `crates/driver-http` + `crates/psub-gateway` | DOC-10 | 1d |
| **P1.5** | `fuzz/` directory with cargo-fuzz targets (HTTP body parser, JSON envelope, MCP/SSE parser) | TEST-04 | 1d |
| **P1.6** | `cargo-llvm-cov` integration + coverage gate (e.g. `--fail-under-lines 70`) | TEST-05 | 0.5d |
| **P1.7** | `.github/dependabot.yml` (weekly cadence, cargo ecosystem) | CI-07 | 0.5h |
| **P1.8** | `.editorconfig` + `.pre-commit-config.yaml` (or `lefthook.yml`) | DX-04, DX-06 | 0.5d |

**Phase 1 exit criteria:**
- `cargo fuzz run http_body -- -runs=1000000` finds no crash for 10 min
- `cargo llvm-cov report` shows ≥ 70% line coverage
- `/metrics` returns Prometheus text, request-id appears in every log line

### Phase 2 — Supply chain + release (1 week, → 96.4 / A grade)

| ID | Title | Pillars fixed | Est |
|---|---|---|---:|
| **P2.1** | CycloneDX SBOM in `.github/workflows/sbom.yml` | SEC-08, SC-10 | 0.5d |
| **P2.2** | SLSA provenance generation in release-binary | RE-13 | 1d |
| **P2.3** | Container image push to GHCR | RE-06 | 0.5d |
| **P2.4** | `.github/ISSUE_TEMPLATE/` + `pull_request_template.md` | CI-14, DX-11, DX-12 | 0.5d |
| **P2.5** | `mutation_testing` with `cargo-mutants` (central policy module only) | TEST-08 | 1d |
| **P2.6** | `docs/migration/v0.2-to-v0.3.md` (psub- rename upgrade) | RE-08 | 0.5d |
| **P2.7** | `docs/ops/DEPLOY.md` + `docs/ops/ROLLBACK.md` | DOC-11, RE-09 | 1d |
| **P2.8** | `docs/TROUBLESHOOTING.md` + `docs/GLOSSARY.md` + `docs/FAQ.md` | DOC-13, DOC-15, DOC-16 | 0.5d |

**Phase 2 exit criteria:**
- `gh release download v0.3.0` produces `attestation.intoto.jsonl` (SLSA provenance)
- `cyclonedx-bom.json` regenerates fresh on every release tag

### Phase 3 — Long-tail polish (1 week, → 100%)

Remaining partial pillars (5): CQ-04, CQ-10, CQ-06, TEST-03 (property tests
workspace-wide), OBS-06 (request-id propagation).

Plus 1 missing pillar (ARCH-14: shared schema crate). Estimated effort: 3 days.

---

## 4. Critical path

```
P0.1 ── P0.2 ── P1.6 ── P2.1 ── P2.2 ── 100%
P0.5 ── P1.4 ── P2.7 ── P3 long-tail
```

**Slip-prone items:**
- P1.5 (fuzz) — finds infinite-parser bugs which take longer than estimated
- P2.2 (SLSA) — Rust SLSA generator config is fragile
- P1.3 (OTel) — context propagation across `axum` → `tokio::spawn` is non-trivial

---

## 5. Verification playbook

For every Phase exit:

```bash
# 1. CI green
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check

# 2. Domain-specific
cargo llvm-cov report --fail-under-lines 70     # Phase 1+
cargo fuzz run http_body -- -runs=1000000        # Phase 1+
cat docs/sbom/cyclonedx.json | jq .components | wc -l   # Phase 2+
```

---

## 6. Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| `psub-` prefix rename generated cross-crate compile breakage that masks the deny step | medium | Run `cargo build --workspace` before claiming Phase 0 exit |
| Fuzz findings surface real parser bugs that explode into a 2-week detour | high | Time-box fuzz to 3 working days; capture first crash and create tickets, defer to Phase 3 |
| OTel exporter adds 5 MB to binary and slows dispatch p99 | medium | Make OTel opt-in via `OTEL_EXPORTER_OTLP_ENDPOINT` env, gate behind `tracing-otel` feature |
| SLSA build provenance requires GitHub OIDC + reusable workflow | medium | Follow [slsa-framework/slsa-github-generator](https://github.com/slsa-framework/slsa-github-generator) examples |

---

## 7. Reporting cadence

- After Phase 0: rerun scorecard, confirm ≥ 73.6 score, publish scorecard diff in PR
- After Phase 1: rerun, expect ≥ 86.4, attach coverage report to release notes
- After Phase 2: rerun, expect ≥ 96.4, attach SBOM diff
- After Phase 3: rerun, target 100, append final score to `CHANGELOG.md`

---

## 8. Out of scope (intentional)

- **Test-09 (load/soak harness)** — k6 plan exists in
  `DISPATCH_MIGRATION_PLAN_2026_06_30.md`; will not run in this engagement
- **DOC-17 (RFC process)** — ad-hoc ADR workflow is sufficient for v0.3
- **SEC-07 (CodeQL)** — GitHub Advanced Security not enabled on this repo
- **RE-07 (homebrew/apt)** — public distribution deferred to Q4 2026

---

## 9. Filing

This plan plus `audit_scorecard.json` plus the in-flight git commits land in
two pull requests:

1. **PR-A — Phase 0 hygiene** (deny.toml, ci.yml, CODEOWNERS, CONTRIBUTING.md,
   ARCHITECTURE.md, docs/adr/0002..0006) — standalone, merges first.
2. **PR-B — Phase 1 observability + fuzz** (tracing, /metrics, fuzz, cov) —
   depends on PR-A's updated docs.

PR-B, PR-C (Phase 2), and the leftover Phase 3 items become separate follow-up
PRs once the first two are merged.
