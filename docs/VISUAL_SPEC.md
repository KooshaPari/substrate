# Visual Specification — substrate CLI

The CLI is in scope of `VISUAL_SPEC.md`. This document is the source of truth for what a user sees when they run `substrate` or any of its subcommands. When the rendered output disagrees with this spec, fix the CLI or update the spec — don't let both drift.

The substrate CLI does not currently depend on an ANSI color crate (no `colored`/`owo-colors` in `crates/driver-cli/Cargo.toml`). Colors below are therefore **opt-in**: rendered only when stderr is a TTY (`IsTerminal::is_terminal()`) and `NO_COLOR` is unset per [no-color.org](https://no-color.org/). CI, scripts, and pipes get the same text without escapes.

## 1. Banner on `--help`

Every subcommand (`dispatch`, `plan`, `argv`, `cloud-dispatch`, `serve`) emits the version banner as the first lines of `--help` output:

```
substrate 0.3.0
```

Followed by the standard `clap`-rendered usage line, the long-about paragraph, then per-section option groups (`TASK`, `ENGINE`, `OUTPUT`, etc.). The banner is plain text — no box-drawing, no color. It is emitted on stdout (clap's contract).

The root `substrate --help` adds a one-line tagline:

```
AI dispatch gateway and TUI.
```

## 2. stdout vs stderr split

| Stream | What goes there |
|--------|-----------------|
| **stdout** | The primary result of the command. For `dispatch`/`plan`/`cloud-dispatch`, this is exactly one JSON object (pretty-printed). For `serve`, nothing is printed on stdout — only structured logs. |
| **stderr** | Human-facing diagnostics: progress, tier downgrades, deprecation notices, lifecycle messages (`substrate serve: listening on ...`), and **all errors**. |

Rule: **if `jq` can parse stdout, the output is well-formed.** Do not emit informational text to stdout.

## 3. Error format

Errors are emitted on stderr in one of two shapes:

### 3.1 Single-line structured error

```
substrate: <error-kind>: <message>
```

Where `<error-kind>` is a stable token (lowercase, kebab-case, parseable by shell scripts). Examples:

```
substrate: engine-not-found: forge binary not found on PATH (set FORGE_BIN)
substrate: lock-conflict: another substrate serve is already running on http://127.0.0.1:7777
substrate: dispatch-failed: codex exited with status 64
```

### 3.2 Multi-line crash with context

For errors that need a cause chain (`anyhow`), the first line follows §3.1; subsequent lines are indented two spaces and prefixed with `caused by: `:

```
substrate: dispatch-failed: codex exited with status 64
  caused by: plan stage: planner returned no argv
  caused by: io error: file not found: /nonexistent/Cargo.toml
```

Stack traces (`RUST_BACKTRACE=full`) are emitted only when the env var is set, and only on stderr. They are never mixed into the stdout JSON.

## 4. Exit codes

| Code | Meaning | Notes |
|------|---------|-------|
| 0 | Success | stdout may be empty (`serve`, no-op commands). |
| 1 | Operational error | Engine missing, planner rejection, child-process failure, lock conflict (substrate `serve::run` uses `process::exit(1)` for the abort case — keep that). |
| 2 | Misuse | Bad CLI invocation (clap's default for arg parsing). |
| 64–78 | Reserved (sysexits.h) | `EX_USAGE=64`, `EX_DATAERR=65`, `EX_NOINPUT=66`, `EX_NOUSER=67`, `EX_NOHOST=68`, `EX_UNAVAILABLE=69`, `EX_SOFTWARE=70`, `EX_OSERR=71`, `EX_OSFILE=72`, `EX_CANTCREAT=73`, `EX_IOERR=74`, `EX_TEMPFAIL=75`, `EX_PROTOCOL=76`, `EX_NOPERM=77`, `EX_CONFIG=78`. Map `engine-not-found` → 69, `lock-conflict` → 69, `engine-timeout` → 75. |
| 130 | SIGINT (128 + 2) | Pass-through from `ctrl-c`. |
| 137 | SIGKILL (128 + 9) | OOM or operator kill; do not treat as a crash. |
| 255 | `-1` / implementation-defined | Reserved for `anyhow::Error` paths where the typed exit code is unknown; prefer adding a typed error variant over using 255. |

The error-stderr from §3 is paired with the exit code: scripts can match on either, but never both (the message can change; the code is the contract).

## 5. Coloring rules

- **Red** (`\x1b[31m...\x1b[0m`): the error kind and the trailing message in §3 errors.
- **Yellow** (`\x1b[33m...\x1b[0m`): tier downgrade notices (`downgraded tier from worker to main after dispatch failure`).
- **Cyan** (`\x1b[36m...\x1b[0m`): lifecycle banners (`substrate serve: listening on ...`).
- **Bold white** (`\x1b[1;37m...\x1b[0m`): the version banner in §1 if rendered to a TTY.
- **Never** color stdout. JSON parsers must not see ANSI escapes.

Color is disabled (a) in non-TTY contexts, (b) when `NO_COLOR` is set, and (c) when `TERM=dumb`. The crate chosen to deliver these escapes (e.g. `colored`, `owo-colors`) must support all three with a single feature flag — do not roll a custom escape wrapper.

## 6. Stable surface

The tokens a script may parse from the CLI:

- Version (`substrate --version`): just the version string.
- `substrate dispatch ...` → JSON object, see §2.
- `substrate plan ...` → JSON object, see §2.
- `substrate --json` is **not** a flag: stdout is JSON by default for `dispatch`/`plan`/`cloud-dispatch`. There is no `--no-json` mode.
- `substrate serve` may accept `--log-format text|json` (proposed, not yet shipped).

Anything outside §1–§5 (banner art, spinners, progress bars) requires updating this document first. PRs that change rendered output without touching this file will be requested-changes.
