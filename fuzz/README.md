# substrate-fuzz

> **Phase 1 (P1.5).** Fuzz targets for substrate's most parser-heavy
> surfaces. This directory is intentionally **not** in the workspace
> (cargo-fuzz convention) so a regular `cargo build --workspace` doesn't
> try to compile it.

---

## Targets

| File | Surfaces under test | Mutation strategy |
|---|---|---|
| `openai_chat_request.rs` | The body parser behind `POST /v1/chat/completions` | Random bytes; never panic, never UB |
| `psub_a2a_message.rs` | `psub_a2a::Message` round-trip (encode → decode) | Random `Message`; verify reconstructed equals original |
| `asn1_der.rs` | `psub_gateway::asn1_der` round-trip | Random OID components and integers |
| `cron_parser.rs` | `psub_gateway::cron_parser::parse` | Random ASCII, including unicode that should reject |
| `ini_parser.rs` | `psub_gateway::ini_parser` | Random bytes; reject infinite-loop keys |

---

## Running

```bash
# 1. Install cargo-fuzz and a nightly toolchain (one-off).
cargo +nightly install cargo-fuzz

# 2. Build a target (sanity check).
cd fuzz
cargo +nightly fuzz build

# 3. Run for ~10 minutes per target.
cargo +nightly fuzz run openai_chat_request -- -max_total_time=600
cargo +nightly fuzz run psub_a2a_message    -- -max_total_time=600
cargo +nightly fuzz run asn1_der            -- -max_total_time=600
cargo +nightly fuzz run cron_parser         -- -max_total_time=600
cargo +nightly fuzz run ini_parser          -- -max_total_time=600
```

Corpus lives under `fuzz/corpus/<target>/`. Regression inputs land in
`fuzz/artifacts/<target>/` on the first crash.

---

## Triage

When a target finds a crash:

1. The artifact is a short sequence of bytes. **Do not** paste it into the
   repo as a literal — store it under `fuzz/corpus/<target>/crash-<sha>`.
2. Write a regression unit test in the corresponding crate
   (`crates/<x>/tests/regressions/<sha>.rs`).
3. Add the regression test input to a `corpus_regressions/` subdirectory
   so the next fuzz run replays it immediately.
4. File a ticket tagged `fuzz-finding`.

---

## Integration with CI

A nightly GitHub Actions workflow should run each target for ~10 minutes and
file the artifact directories as build artifacts on completion. See
`docs/operations/fuzz-ci.md` (P2.2 follow-up) for the workflow definition.

---

## Exit criteria (P1.5)

- [x] Cargo.toml scaffolding in place.
- [x] At least 5 fuzz targets covering hot parsers.
- [ ] Each target runs 1,000,000 iterations with no panic.
- [ ] Regression corpus committed for any prior crash.
- [ ] Nightly fuzz job in GitHub Actions.
