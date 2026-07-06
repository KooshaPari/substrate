//! Backbone-2 theme tokens for substrate (Phase 2 tokens.css adoption).
//!
//! Source of truth: tokens.css (`bb2-graphite`, `bb2-panel`, `bb2-pulse-green`,
//! `bb2-sync-violet`, `bb2-warm-amber`). This module is the Rust mirror so
//! splash, tui, and any future ratatui-driven TUI chrome all paint with the
//! same family without re-deriving hex anywhere.
//!
//! Mirrors sharecli/src/theme.rs so both repos agree on the canonical hex
//! values. If tokens.css changes, BOTH repos must update this file.

#![allow(dead_code)]

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Build an `Rgb` from a `#rrggbb` or `rrggbb` hex literal at compile time.
    pub const fn from_hex(hex: &str) -> Self {
        let bytes = hex.as_bytes();
        let start = if bytes[0] == b'#' { 1 } else { 0 };
        let mut n: u32 = 0;
        let mut i = 0;
        while i < 6 {
            let b = bytes[start + i];
            let v = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => panic!("non-hex digit in from_hex"),
            };
            n = (n << 4) | v as u32;
            i += 1;
        }
        Rgb(((n >> 16) & 0xff) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8)
    }

    /// ANSI 24-bit truecolor foreground escape.
    pub fn ansi_fg(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.0, self.1, self.2)
    }

    /// ANSI 24-bit truecolor background escape.
    pub fn ansi_bg(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.0, self.1, self.2)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ThemeVariant {
    Backbone2,
}

impl Default for ThemeVariant {
    fn default() -> Self { ThemeVariant::Backbone2 }
}

/// Backbone-2 token set — exact mirror of tokens.css.
#[derive(Copy, Clone, Debug)]
pub struct Tokens {
    pub variant: ThemeVariant,
    pub graphite: Rgb,
    pub panel: Rgb,
    pub pulse_green: Rgb,
    pub sync_violet: Rgb,
    pub warm_amber: Rgb,
}

impl Tokens {
    pub const BACKBONE2: Tokens = Tokens {
        variant: ThemeVariant::Backbone2,
        graphite:    Rgb::from_hex("#0a0d12"),
        panel:       Rgb::from_hex("#161b22"),
        pulse_green: Rgb::from_hex("#3fb950"),
        sync_violet: Rgb::from_hex("#a371f7"),
        warm_amber:  Rgb::from_hex("#d29922"),
    };

    pub const fn default() -> Self { Self::BACKBONE2 }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "backbone-2" | "backbone2" | "bb2" => Some(Self::BACKBONE2),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_hex_drops_prefix_and_packs_rgb() {
        assert_eq!(Rgb::from_hex("#3fb950"), Rgb(0x3f, 0xb9, 0x50));
    }

    #[test]
    fn from_hex_accepts_no_prefix() {
        assert_eq!(Rgb::from_hex("a371f7"), Rgb(0xa3, 0x71, 0xf7));
    }

    #[test]
    fn backbone2_constants_match_tokens_css() {
        let t = Tokens::BACKBONE2;
        assert_eq!(t.graphite,    Rgb(0x0a, 0x0d, 0x12));
        assert_eq!(t.panel,       Rgb(0x16, 0x1b, 0x22));
        assert_eq!(t.pulse_green, Rgb(0x3f, 0xb9, 0x50));
        assert_eq!(t.sync_violet, Rgb(0xa3, 0x71, 0xf7));
        assert_eq!(t.warm_amber,  Rgb(0xd2, 0x99, 0x22));
    }

    #[test]
    fn ansi_helpers_emit_truecolor_payload() {
        let fg = Tokens::BACKBONE2.pulse_green.ansi_fg();
        let bg = Tokens::BACKBONE2.panel.ansi_bg();
        assert!(fg.starts_with("\x1b[38;2;"));
        assert!(bg.starts_with("\x1b[48;2;"));
        assert!(fg.contains("63"));  // 0x3f
        assert!(bg.contains("27"));  // 0x1b
    }
}
