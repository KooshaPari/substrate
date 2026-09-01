# Changelog

All notable changes to substrate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/) and the project adheres to [Semantic Versioning](https://semver.org/).

Entries are generated from the git history (`git log -200`). Commit subjects are mapped to Keep-a-Changelog sections by their conventional-commit prefix: `feat` -> Added, `fix` -> Fixed, `feat!`/`remove`/`revert` -> Removed, `security` -> Security, `deprecate` -> Deprecated, everything else (`perf`, `refactor`, `style`, `docs`, `test`, `build`, `chore`, `migrate`) -> Changed. Pure `chore(deps): bump` commits, `ci:` housekeeping commits, and merge commits are intentionally omitted.

## [Unreleased]

### Changed

- add AI slop inside + downloads badges ([#350](https://github.com/KooshaPari/substrate/pull/350))
- add genuine files + scorecard CI
- replace placeholder benchmark with real gateway benchmarks
- add gitleaks.toml
- add missing gates
- add CLAUDE.md agent config
- add fuzz, mutation, benchmark scaffolds
- optimize extract_pr_urls to O(n) with HashSet dedup ([#291](https://github.com/KooshaPari/substrate/pull/291))
- recover nexus service registry from archived repo
- exclude unavailable crates from publication

### Fixed

- incremental streaming token budget accounting ([#348](https://github.com/KooshaPari/substrate/pull/348))
- preserve deterministic tier ordering ([#340](https://github.com/KooshaPari/substrate/pull/340))
- preserve memory tier and store integrity ([#282](https://github.com/KooshaPari/substrate/pull/282))
- exclude file watcher squatter crate

## [0.3.10] - 2026-07-21

### Changed

- release v0.3.10
- prepare v0.3.10 release

### Fixed

- complete public crate metadata
- complete public crate metadata

## [0.3.9] - 2026-07-21

### Fixed

- exclude cyclic cloud conformance crates ([#331](https://github.com/KooshaPari/substrate/pull/331))

## [0.3.8] - 2026-07-21

### Fixed

- order cloud conformance before cloud codex ([#330](https://github.com/KooshaPari/substrate/pull/330))

## [0.3.7] - 2026-07-21

### Fixed

- order crate release batches by dependency spine ([#329](https://github.com/KooshaPari/substrate/pull/329))

## [0.3.6] - 2026-07-21

### Changed

- prepare v0.3.6 release ([#328](https://github.com/KooshaPari/substrate/pull/328))

## [0.3.5] - 2026-07-21

### Changed

- prepare v0.3.5 release ([#327](https://github.com/KooshaPari/substrate/pull/327))

## [0.3.4] - 2026-07-21

### Changed

- prepare v0.3.4 release ([#326](https://github.com/KooshaPari/substrate/pull/326))

## [0.3.3] - 2026-07-21

### Changed

- prepare v0.3.3 release ([#325](https://github.com/KooshaPari/substrate/pull/325))
- wait for watcher registration before debounce writes ([#324](https://github.com/KooshaPari/substrate/pull/324))
- reconcile current main evidence

### Fixed

- make crates release publish graph explicit ([#323](https://github.com/KooshaPari/substrate/pull/323))

## [0.3.2] - 2026-07-21

### Added

- close scorecard final gaps

### Changed

- prepare v0.3.2 release
- add dependency policy glossary and FAQ

### Fixed

- publish internal schema crate under owned name

## [0.3.1] - 2026-07-21

### Added

- propagate request IDs
- Phase 0 docs + governance (86%→82.5%) ([#279](https://github.com/KooshaPari/substrate/pull/279))
- Phase 1 tracing + OTel for gateway crate
- Phase 0 docs + governance
- L167 van_eck + gauss_jordan + rail_fence + iso8601 ([#278](https://github.com/KooshaPari/substrate/pull/278))
- L166 soundex + rsync rollsum + toroidal maze + postfix eval ([#277](https://github.com/KooshaPari/substrate/pull/277))
- L165 splay_tree + astar_basic + floyd_warshall + lis_dp ([#276](https://github.com/KooshaPari/substrate/pull/276))
- L164 red-black tree + DLX dancing links + 0/1 knapsack + Mandelbrot ([#275](https://github.com/KooshaPari/substrate/pull/275))
- L163 Kahn topological sort + AVL tree + RLE + Hamilton quaternions ([#274](https://github.com/KooshaPari/substrate/pull/274))
- L162 XXTEA + RC4 + COBS + Salsa20 utility modules ([#273](https://github.com/KooshaPari/substrate/pull/273))
- L161 blowfish + fibonacci_heap + aho_corasick + dijkstra_basic ([#272](https://github.com/KooshaPari/substrate/pull/272))
- L160 RIPEMD-160 + MD4 + BLAKE2 + edit distance ([#271](https://github.com/KooshaPari/substrate/pull/271))
- L159 hamming_code + matrix_ops + xtea + fnv1a ([#270](https://github.com/KooshaPari/substrate/pull/270))
- L158 keccak + crc32c + url_percent + tea ([#269](https://github.com/KooshaPari/substrate/pull/269))
- L157 crc16 + gray_code + delta_encoding + murmur3 ([#268](https://github.com/KooshaPari/substrate/pull/268))
- L156 scrypt + cellular_automaton + integer_log + z85 ([#267](https://github.com/KooshaPari/substrate/pull/267))
- L155 lzw + lz77 + crockford_base32 + simd_checksum ([#266](https://github.com/KooshaPari/substrate/pull/266))
- L154 merkle_tree + quickselect + boyer_moore + bencode ([#265](https://github.com/KooshaPari/substrate/pull/265))
- L153 sha512 + rabin_karp + z_algorithm + conway_gol ([#264](https://github.com/KooshaPari/substrate/pull/264))
- L152 KMP + random distributions + interval tree + segment tree ([#263](https://github.com/KooshaPari/substrate/pull/263))
- wave-41 mapi_props_parity + pres_header_parity
- L151 modular arithmetic + union-find + Fenwick + HTTP status ([#262](https://github.com/KooshaPari/substrate/pull/262))
- L150 statistics + sorts + CRC variants + skip list ([#261](https://github.com/KooshaPari/substrate/pull/261))
- L149 xxHash + string metrics + cuckoo filter + QRNG ([#260](https://github.com/KooshaPari/substrate/pull/260))
- L148 SHA-1 + LRU + YAML + SemVer ([#259](https://github.com/KooshaPari/substrate/pull/259))
- L147 SHA-256 + JWT decode + MIME + cron ([#258](https://github.com/KooshaPari/substrate/pull/258))
- wave-40 rdp_neg_parity + dns_query_parser_parity
- L146 MD5 + CSV + JSON Pointer + Cookie ([#257](https://github.com/KooshaPari/substrate/pull/257))
- L145 Huffman + sorted-index + geo + markdown ([#256](https://github.com/KooshaPari/substrate/pull/256))
- wave-39 rdp_neg + bip39_mnemonic
- wave-38 asn1_ber + dhcpv6_msg
- L144 hash + number theory + finance + text diff ([#255](https://github.com/KooshaPari/substrate/pull/255))
- L143 ciphers (Caesar, Vigenere) + phonetic (Soundex, Metaphone) + base58 + morse_code fixup ([#254](https://github.com/KooshaPari/substrate/pull/254))
- wave-37 mapi_props + pres_header_parse
- L142 v0.3.0 expansion — money_currency + units_si + morse_code ([#253](https://github.com/KooshaPari/substrate/pull/253))
- wave-36 qoi_image + bmp_image + utf8_iter unsafe-fix
- L141 v0.3.0 expansion — noise_xoshiro + polynomials ([#252](https://github.com/KooshaPari/substrate/pull/252))
- L140 v0.3.0 expansion — utf8_iter + decimal_lc + calendar_date ([#251](https://github.com/KooshaPari/substrate/pull/251))
- L138 v0.3.0 expansion — natural_sort + base_n_radix + roman_numeral ([#250](https://github.com/KooshaPari/substrate/pull/250))
- wave-35 snmpv3_msg + cdp_meraki_discovery
- L137 v0.3.0 expansion — base64url + reed_solomon ([#249](https://github.com/KooshaPari/substrate/pull/249))
- wave-34 ipsec_esp_parse + radiotap
- L136 v0.3.0 expansion — pbkdf2 + chacha20 + jwt_es256 + utf8_validator ([#248](https://github.com/KooshaPari/substrate/pull/248))
- wave-33 tacacs_auth + imap_response_parity
- L135 v0.3.0 expansion — hmac_sha256 + hkdf + base32 ([#247](https://github.com/KooshaPari/substrate/pull/247))
- L134 v0.3.0 expansion — qr_code + asn1_der + ipv6_address ([#246](https://github.com/KooshaPari/substrate/pull/246))
- L131 v0.3.0 expansion — cyclic_check + uri_template + unicode_normalization ([#245](https://github.com/KooshaPari/substrate/pull/245))
- wave-32 webvtt_cue_parse + webmanifest
- L129 v0.3.0 expansion — distance + credit_card + bloom_filter + glob_pattern ([#244](https://github.com/KooshaPari/substrate/pull/244))
- wave-31 git_loose_object + ntp_control_message
- L123 v0.3.0 expansion — base85 + backoff utility modules ([#243](https://github.com/KooshaPari/substrate/pull/243))
- L122 wave-29 — DHCP options + MQTT v5 packet codec
- L120 MVP cut-line — TOML wave loader + claude stream parser + watcher ([#241](https://github.com/KooshaPari/substrate/pull/241))
- L119 wave-28 — OAuth1 parity verifier, LDAP filter AST, segwit bech32m ([#240](https://github.com/KooshaPari/substrate/pull/240))
- L114 cockpit upgrade — nav header + module list ([#237](https://github.com/KooshaPari/substrate/pull/237))
- L112 server-rendered HTML cockpit at GET / ([#236](https://github.com/KooshaPari/substrate/pull/236))
- add 'serve' REST subcommand (axum 0.8 + tokio 1.42) ([#234](https://github.com/KooshaPari/substrate/pull/234))
- add 'inspect' subcommand + sync-violet splash ([#233](https://github.com/KooshaPari/substrate/pull/233))
- wave-30 icalendar_parse + vcard_parse

### Changed

- bump workspace to v0.3.1 ([#318](https://github.com/KooshaPari/substrate/pull/318))
- synchronize config watcher readiness
- define RFC process ([#306](https://github.com/KooshaPari/substrate/pull/306))
- add contributor workflow recipes ([#302](https://github.com/KooshaPari/substrate/pull/302))
- document development container contract ([#301](https://github.com/KooshaPari/substrate/pull/301))
- format request ID middleware
- document rust-analyzer Cargo workspace discovery ([#299](https://github.com/KooshaPari/substrate/pull/299))
- add versioned pre-commit quality gates ([#298](https://github.com/KooshaPari/substrate/pull/298))
- correct Codex prompt syntax ([#295](https://github.com/KooshaPari/substrate/pull/295))
- unblock lib test compile by adding missing response_cache to admin fixture
- add RFC 4034 Appendix B key_tag property + 100-case determinism
- add 64-case idempotence property for x509 DER parser
- add BIP-173 spec vectors + 100-case round-trip property
- L117 cockpit footer — cycle stamp ([#239](https://github.com/KooshaPari/substrate/pull/239))
- L116 cockpit CSS polish — pulse pill + fadein animation ([#238](https://github.com/KooshaPari/substrate/pull/238))

### Fixed

- package driver-cli substrate binary ([#316](https://github.com/KooshaPari/substrate/pull/316))
- stabilize cloud conformance and watcher tests ([#314](https://github.com/KooshaPari/substrate/pull/314))
- allowlist explicit public test vectors
- keep registry token out of argv
- preserve orchestration failure handling ([#281](https://github.com/KooshaPari/substrate/pull/281))
- remove dead code + value-never-read warnings
- ratchet 7 pre-existing test failures + admin compile blockers + unused-mut warnings
- morse_code.rs E0252/E0277
- wave-29/30 parity logic bugs
- wave-29/30 parity compile errors
- restore clean oidc_jwt.rs — revert broken #212 squash merge ([#232](https://github.com/KooshaPari/substrate/pull/232))
- bump internal crate versions to 0.2.0 to match workspace.package.version ([#193](https://github.com/KooshaPari/substrate/pull/193))

## [0.3.0] - 2026-07-04

### Changed

- Workspace version bumped from `0.2.x` to `0.3.0`.
- No code-level changes between the `v0.2.0` and `v0.3.0` tags (tags applied ~17 hours apart on the same day); the bump opened the `0.3.x` patch series.

## [0.2.0] - 2026-07-04

### Added

- Prometheus latency histogram (exponential buckets 10ms-5.12s).
- Sliding-window request rate tracker (10s window).
- Extended `/health` endpoint with per-provider SLA and circuit-breaker state.
- SLA violation tier checker (P50/P95/P99).
- TUI boot animation with `BootPhase` state machine.
- Gateway startup banner.
- OCI Containerfile + `process-compose.yaml`.

## [0.1.0] - 2026-06-30

### Added

- Initial repository scaffold (Phase 0).
- Core workspace bootstrap: `substrate` crate skeleton, CI scaffolding, and pinned `phenotype-mcp` path dependency version.

### Notes

- `substrate` description at this point: AI execution and provider-routing substrate across HTTP, CLI, MCP, and A2A interfaces.
- Released as the public v0.1.0 baseline before the v0.2.0 feature work.

<!--
This CHANGELOG was regenerated from `git log -200` on 2026-08-31.
Skipped categories during generation:
  - chore(deps): dependency bumps (handled by Renovate)
  - ci: CI/CD housekeeping (workflow/tooling updates)
  - merge: merge commits
Manual edits to prior-version entries (v0.1.0, v0.2.0) were preserved from the
previous CHANGELOG and augmented with notes where the 200-commit window did
not contain a representative commit.
-->
