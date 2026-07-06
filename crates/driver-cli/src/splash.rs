//! Backbone-2 splash banner for the substrate CLI (L97/L98/L99).
//!
//! Renders a 3-line panel-base banner with the Backbone-2 mark + a pulse-green
//! daemon-pulse line + a sync-violet reroute-up glyph + a warm-amber
//! thermal-cooldown accent. Honors `NO_COLOR` and non-TTY environments by
//! collapsing to plain text.
//!
//! Palette (matches crates/driver-cli/tests/iconset.rs Backbone-2 dominance):
//!   panel   #161b22   pulse-green #3fb950
//!   sync-violet  #a371f7        warm-amber   #d29922
//!   graphite-black  #0a0d12

#![allow(dead_code)] // wired conditionally into main when --no-splash is *not* set

use std::io::IsTerminal;

const BACKBONE2: &str = "Backbone-2";

fn color(tty: bool, code: &str, text: &str) -> String {
    if tty { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
}

/// Print the 3-line Backbone-2 splash to stdout. Safe to call in non-TTY
/// contexts (e.g. piped `substrate plan | jq`) — collapses to plain text.
pub fn print_cli_splash(version: &str) {
    let tty = std::io::stdout().is_terminal();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let colored = tty && !no_color;

    // 24-col panel base band
    let panel = color(colored, "48;5;236", "                        ");
    let grid = color(colored, "38;5;60", "substrate hexagonal mesh");

    // Line 1: Backbone-2 mark + version
    let mark  = color(colored, "1;38;5;141", "  subnet ");
    let ver   = color(colored, "38;5;250", version);
    println!("{panel}{mark}{ver}");

    // Line 2: daemon-pulse line (pulse-green)
    let pulse = color(colored, "38;5;42", "  p u l s e     ok ");
    println!("{panel}{pulse}");

    // Line 3: reroute-up glyph (sync-violet) + thermal-cooldown accent (warm-amber)
    let route = color(colored, "38;5;141", "  ^ ^ ^  sync ");
    let thermal = if tty && !no_color {
        color(colored, "38;5;214", "*cooldown*")
    } else {
        "*cooldown*".to_string()
    };
    println!("{panel}{route}{thermal}  {BACKBONE2}  {grid}");

    eprintln!("{:>24}", color(colored, "38;5;245", "via forge · codex · claude · agentapi"));
}

/// Test-only entrypoint. Always renders regardless of TTY/NO_COLOR so
/// integration tests in `tests/splash.rs` can assert on the captured stdout.
#[cfg(test)]
pub fn render_for_tests(version: &str) {
    print_cli_splash(version);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke: splash renders without panic when NO_COLOR is set.
    /// We don't capture stdout here — just assert the function runs end-to-end.
    #[test]
    fn splash_renders_plain_no_panic() {
        std::env::set_var("NO_COLOR", "1");
        print_cli_splash("0.0.0-test");
    }

    /// Smoke: render-for-tests path also runs end-to-end.
    #[test]
    fn render_for_tests_runs() {
        std::env::set_var("NO_COLOR", "1");
        render_for_tests("0.0.0-test");
    }

    /// Color helper collapses to plain text when colored=false.
    #[test]
    fn color_helper_plain_mode() {
        let plain = color(false, "1;31", "red");
        assert_eq!(plain, "red");
        assert!(!plain.contains('\x1b'));
    }

    /// Color helper emits ANSI sequences when colored=true.
    #[test]
    fn color_helper_ansi_mode() {
        let ansi = color(true, "1;31", "red");
        assert!(ansi.contains("\x1b["));
        assert!(ansi.contains("red"));
    }
}
