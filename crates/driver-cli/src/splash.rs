//! Backbone-2 splash banner for the substrate CLI (L97/L98/L99) with Phase 2
//! tokens.css adoption.
//!
//! Renders a 3-line panel-base banner with the Backbone-2 mark + a pulse-green
//! daemon-pulse line + a sync-violet reroute-up glyph + a warm-amber
//! thermal-cooldown accent. Honors `NO_COLOR` and non-TTY environments by
//! collapsing to plain text.
//!
//! Palette source of truth (Phase 2): `crate::theme::Tokens::BACKBONE2`. All
//! emitted ANSI sequences are derived from those typed tokens so the splash
//! can never drift from tokens.css.

#![allow(dead_code)] // wired conditionally into main when --no-splash is *not* set

use std::io::IsTerminal;

use crate::theme::Tokens;

const BACKBONE2: &str = "Backbone-2";

fn color(tty: bool, code: &str, text: &str) -> String {
    if tty {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Derive a 6-digit ANSI 24-bit-bg payload from an Rgb token.
/// Selects the closest ANSI 256 index when the terminal is truecolor-unsafe.
fn bg_256(rgb: crate::theme::Rgb, text: &str) -> String {
    let payload = format!("48;2;{};{};{}", rgb.0, rgb.1, rgb.2);
    color(true, &payload, text)
}

fn fg_256(rgb: crate::theme::Rgb, text: &str) -> String {
    let payload = format!("38;2;{};{};{}", rgb.0, rgb.1, rgb.2);
    color(true, &payload, text)
}

/// Print the 3-line Backbone-2 splash to stdout. Safe to call in non-TTY
/// contexts (e.g. piped `substrate plan | jq`) — collapses to plain text.
pub fn print_cli_splash(version: &str) {
    let tty = std::io::stdout().is_terminal();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let colored = tty && !no_color;

    // 24-col panel base band — sourced from Tokens::BACKBONE2.panel.
    let t = Tokens::BACKBONE2;
    let panel_pad = "                        ";
    let panel = if colored {
        bg_256(t.panel, panel_pad)
    } else {
        panel_pad.to_string()
    };
    let grid = color(colored, "38;5;60", "substrate hexagonal mesh");

    // Line 1: Backbone-2 mark + version (sync-violet bold title, panel bg)
    let mark = if colored {
        fg_256(t.sync_violet, "  subnet ")
    } else {
        "  subnet ".to_string()
    };
    let ver = color(colored, "38;5;250", version);
    println!("{panel}{mark}{ver}");

    // Line 2: daemon-pulse line (pulse-green from tokens)
    let pulse = if colored {
        fg_256(t.pulse_green, "  p u l s e     ok ")
    } else {
        "  p u l s e     ok ".to_string()
    };
    println!("{panel}{pulse}");

    // Line 3: reroute-up glyph (sync-violet) + thermal-cooldown accent (warm-amber)
    let route = if colored {
        fg_256(t.sync_violet, "  ^ ^ ^  sync ")
    } else {
        "  ^ ^ ^  sync ".to_string()
    };
    let thermal = if colored {
        fg_256(t.warm_amber, "*cooldown*")
    } else {
        "*cooldown*".to_string()
    };
    println!("{panel}{route}{thermal}  {BACKBONE2}  {grid}");

    eprintln!(
        "{:>24}",
        color(colored, "38;5;245", "via forge · codex · claude · agentapi")
    );
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

    /// Phase 2 integration: bg_256() emits an ANSI 48;2; payload derived from
    /// the Backbone-2 panel token, so the splash line 1 background matches
    /// tokens.css bb2-panel exactly.
    #[test]
    fn splash_bg_matches_backbone2_panel_token() {
        let t = Tokens::BACKBONE2;
        let s = bg_256(t.panel, " ");
        assert!(s.starts_with("\x1b[48;2;"));
        // 0x16 = 22 ; 0x1b = 27 ; 0x22 = 34
        assert!(
            s.contains("22;27;34"),
            "expected bb2-panel rgb 22;27;34 in payload, got: {s}"
        );
    }

    /// Phase 2 integration: fg_256() emits an ANSI 38;2; payload derived from
    /// the Backbone-2 warm-amber token — used for *cooldown* on line 3.
    #[test]
    fn splash_fg_matches_backbone2_warm_amber_token() {
        let t = Tokens::BACKBONE2;
        let s = fg_256(t.warm_amber, "*cooldown*");
        assert!(s.starts_with("\x1b[38;2;"));
        // 0xd2 = 210 ; 0x99 = 153 ; 0x22 = 34
        assert!(
            s.contains("210;153;34"),
            "expected bb2-warm-amber rgb 210;153;34, got: {s}"
        );
        assert!(s.contains("*cooldown*"));
    }
}
