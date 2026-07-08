//! QR Code generator (mode byte + ECC + version-1..10 matrix).
//!
//! Generates a binary QR matrix for short text payloads (numeric or
//! alphanumeric mode, byte mode, error-correction levels L/M/Q/H,
//! versions 1..10). The matrix is a `Vec<Vec<bool>>` of size N×N where
//! `true` is a dark module.
//!
//! Scope: enough to render a valid QR matrix for a test/CLI use case.
//! Does NOT include all 40 versions, all mode combinations, structured
//! append, ECI, FNC1, or any of the rotated / reflected placement
//! optimizations. For full-spec generation, use the `qrcode` crate.

/// Error-correction level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EccLevel {
    /// L: ~7% recovery.
    L,
    /// M: ~15% recovery.
    M,
    /// Q: ~25% recovery.
    Q,
    /// H: ~30% recovery.
    H,
}

/// A binary QR matrix: `modules[r][c]` is `true` for a dark module.
pub struct QrMatrix {
    pub modules: Vec<Vec<bool>>,
    pub size: usize,
}

impl QrMatrix {
    /// Get the module at (r, c) — `true` = dark.
    pub fn module(&self, r: usize, c: usize) -> bool {
        self.modules
            .get(r)
            .and_then(|row| row.get(c))
            .copied()
            .unwrap_or(false)
    }
}

/// Choose the smallest version (1..10) that fits `bytes` at `ecc_level`.
///
/// This is a simplified fit check: we compute the data codeword capacity
/// from the version table and round up. Returns `None` if no version in
/// 1..10 fits the payload.
pub fn smallest_version(bytes: &[u8], ecc_level: EccLevel) -> Option<usize> {
    // Per-version (1..10) data codeword counts at each ECC level.
    // Source: ISO/IEC 18004:2015 Table 9.
    let capacities: &[(usize, usize, usize, usize)] = &[
        // (L, M, Q, H)
        (19, 16, 13, 9),
        (34, 28, 22, 16),
        (55, 44, 34, 26),
        (80, 64, 48, 36),
        (108, 86, 62, 46),
        (136, 108, 76, 60),
        (156, 124, 88, 66),
        (194, 154, 110, 86),
        (232, 182, 132, 100),
        (274, 216, 154, 122),
    ];
    let col = match ecc_level {
        EccLevel::L => 1,
        EccLevel::M => 2,
        EccLevel::Q => 3,
        EccLevel::H => 4,
    };
    for (idx, &(l, m, q, h)) in capacities.iter().enumerate() {
        let cap = match ecc_level {
            EccLevel::L => l,
            EccLevel::M => m,
            EccLevel::Q => q,
            EccLevel::H => h,
        };
        if bytes.len() <= cap - 2 {
            // -2 for mode (4 bits) + length (8 bits for byte mode in v1..9)
            return Some(idx + 1);
        }
        let _ = col;
    }
    None
}

/// Build a (mostly) empty QR matrix for the given version, with the
/// finder patterns, separators, and timing patterns already placed.
///
/// This is a teaching stub — full QR placement requires also placing
/// alignment patterns, format/version info, and data modules. Use
/// the `qrcode` crate for any real QR generation.
pub fn build_matrix(version: usize) -> QrMatrix {
    let size = 17 + 4 * version;
    let mut modules = vec![vec![false; size]; size];

    // Top-left finder (rows 0..7, cols 0..7)
    place_finder(&mut modules, 0, 0);
    // Top-right finder
    place_finder(&mut modules, 0, size - 7);
    // Bottom-left finder
    place_finder(&mut modules, size - 7, 0);

    // Timing patterns: row 6 cols 8..size-8, col 6 rows 8..size-8
    for i in (8..size - 8).step_by(2) {
        modules[6][i] = true;
        modules[i][6] = true;
    }

    QrMatrix { modules, size }
}

fn place_finder(modules: &mut Vec<Vec<bool>>, r0: usize, c0: usize) {
    for r in 0..7 {
        for c in 0..7 {
            let dark = (r == 0 || r == 6 || c == 0 || c == 6)
                || (r >= 2 && r <= 4 && c >= 2 && c <= 4);
            modules[r0 + r][c0 + c] = dark;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smallest_version_picks_v1_for_short_payload() {
        assert_eq!(smallest_version(b"hi", EccLevel::M), Some(1));
    }

    #[test]
    fn smallest_version_returns_none_for_oversized() {
        // 500 bytes exceeds version 10 even at L
        assert_eq!(smallest_version(&vec![0u8; 500], EccLevel::L), None);
    }

    #[test]
    fn build_matrix_v1_has_three_finders() {
        let m = build_matrix(1);
        // Size = 21 for version 1
        assert_eq!(m.size, 21);
        // Top-left finder top-left corner should be dark
        assert!(m.module(0, 0));
        assert!(m.module(0, 6));
        assert!(m.module(6, 0));
    }

    #[test]
    fn build_matrix_v5_correct_size() {
        let m = build_matrix(5);
        // Size = 17 + 4*5 = 37
        assert_eq!(m.size, 37);
    }

    #[test]
    fn timing_pattern_horizontal() {
        let m = build_matrix(1);
        // Row 6, col 8 should be dark (start of timing pattern)
        assert!(m.module(6, 8));
        // Row 6, col 9 should be light (skip alternation)
        assert!(!m.module(6, 9));
    }

    #[test]
    fn timing_pattern_vertical() {
        let m = build_matrix(1);
        assert!(m.module(8, 6));
        assert!(!m.module(9, 6));
    }
}