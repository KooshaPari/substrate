//! 1D cellular automaton (Wolfram elementary rules).
//!
//! A 1D binary cellular automaton evolves a row of cells under a neighborhood
//! function f({left, self, right}) -> new_self. Wolfram numbering assigns
//! each rule a 0..255 identifier based on the 8 possible 3-cell neighborhoods:
//!
//!   neighborhood 111 -> bit 7 (most significant)
//!   neighborhood 110 -> bit 6
//!   ...
//!   neighborhood 000 -> bit 0 (least significant)
//!
//! Famous rules:
//! * Rule 30 — chaotic; used by Mathematica for random-number generation.
//! * Rule 90 — produces a Sierpinski triangle.
//! * Rule 110 — Turing-complete.
//! * Rule 184 — models traffic flow.
//!
//! Reference: <https://en.wikipedia.org/wiki/Elementary_cellular_automaton>

/// Number of cells in the row. Cells exist on a finite ring (torus), so
/// left/right neighbors wrap around the edge.
pub const DEFAULT_WIDTH: usize = 79;

/// Look up the new cell value for a given neighborhood (3 bits, 0..8).
#[inline]
fn rule_lookup(rule: u8, neighborhood: u8) -> u8 {
    debug_assert!(neighborhood < 8, "neighborhood must be 0..8");
    (rule >> neighborhood) & 1
}

/// Advance a single 1D-CA generation on a wrapped row of cells.
///
/// `cells` is a mutable ring buffer of `0`/`1` values; the function updates
/// each cell in place based on its left, self, and right neighbors (with
/// wrap-around at both ends).
pub fn step(rule: u8, cells: &mut [u8]) {
    assert!(cells.iter().all(|&c| c == 0 || c == 1), "cells must be 0/1");
    let n = cells.len();
    if n == 0 {
        return;
    }
    let src: Vec<u8> = cells.to_vec();
    for i in 0..n {
        let l = src[(i + n - 1) % n];
        let s = src[i];
        let r = src[(i + 1) % n];
        let neighborhood = (l << 2) | (s << 1) | r;
        cells[i] = rule_lookup(rule, neighborhood);
    }
}

/// Render a CA evolution as a multi-line ASCII diagram.
///
/// `generations` rows are produced. The first row is `seed`. Returns a String
/// suitable for printing, with each row on its own line.
pub fn render(rule: u8, seed: &[u8], generations: usize) -> String {
    let w = if seed.is_empty() {
        DEFAULT_WIDTH
    } else {
        seed.len()
    };
    let mut row: Vec<u8> = if seed.is_empty() {
        let mut r = vec![0u8; w];
        r[w / 2] = 1;
        r
    } else {
        let mut r = Vec::with_capacity(w);
        for &b in seed {
            r.push(if b != 0 { 1 } else { 0 });
        }
        // If seed was shorter than DEFAULT_WIDTH, pad with zeros around it.
        if r.len() < w {
            let pad = (w - r.len()) / 2;
            let mut padded = vec![0u8; pad];
            padded.append(&mut r);
            padded.resize(w, 0);
            r = padded;
        }
        r
    };

    let mut out = String::new();
    for g in 0..generations {
        for &c in &row {
            out.push(if c == 1 { '#' } else { ' ' });
        }
        if g + 1 < generations {
            out.push('\n');
        }
        step(rule, &mut row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_to_string(cells: &[u8]) -> String {
        cells
            .iter()
            .map(|&c| if c == 1 { '#' } else { ' ' })
            .collect()
    }

    #[test]
    fn step_rule_0_all_zeros() {
        // Rule 0 -> all cells become 0 (regardless of neighborhood).
        let mut row = [1u8, 1, 1, 1, 1];
        step(0, &mut row);
        assert_eq!(row, [0, 0, 0, 0, 0]);
    }

    #[test]
    fn step_rule_255_all_ones() {
        // Rule 255 -> all cells become 1.
        let mut row = [0u8, 0, 0, 0, 0];
        step(255, &mut row);
        assert_eq!(row, [1, 1, 1, 1, 1]);
    }

    #[test]
    fn step_rule_30_step_from_single_seed() {
        // Rule 30 (binary 00011110): 000->0, 001->1, 010->1, 011->1, 100->1,
        // 101->0, 110->0, 111->0. Starting from a single 1 in a 9-cell ring:
        //   gen 0: 000010000
        //   gen 1: 000111000  (cells 3,4,5 become 1)
        //   gen 2: 001100100  (cells 2,3,6)
        let mut row = vec![0u8, 0, 0, 0, 1, 0, 0, 0, 0];
        let gen0 = row_to_string(&row);
        assert_eq!(gen0, "    #    ");

        step(30, &mut row);
        let gen1 = row_to_string(&row);
        assert_eq!(gen1, "   ###   ");

        step(30, &mut row);
        let gen2 = row_to_string(&row);
        assert_eq!(gen2, "  ##  #  ");
    }

    #[test]
    fn step_rule_90_produces_sierpinski() {
        // Rule 90: 000->0, 001->1, 010->0, 011->1, 100->1, 101->0, 110->1, 111->0
        // Starting from a single 1 in a 9-cell ring of zeros:
        //   gen 0: 000010000
        //   gen 1: 000101000  (cells 3, 5)
        //   gen 2: 001000100  (cells 2, 6)
        let mut row = vec![0u8, 0, 0, 0, 1, 0, 0, 0, 0];
        let gen0 = row_to_string(&row);
        assert_eq!(gen0, "    #    ");

        step(90, &mut row);
        let gen1 = row_to_string(&row);
        assert_eq!(gen1, "   # #   ");

        step(90, &mut row);
        let gen2 = row_to_string(&row);
        assert_eq!(gen2, "  #   #  ");
    }

    #[test]
    fn step_rule_184_traffic_flow() {
        // Rule 184 (binary 10111000): 000->0, 001->0, 010->0, 011->1,
        // 100->1, 101->0, 110->0, 111->1. Models traffic flow.
        // Input [0,0,0,1,1,1,0,0] -> cells 3,4,5,6 become [1,1,1,0]
        // computed by neighborhood; see traces in test below.
        let mut row = [0u8, 0, 0, 1, 1, 1, 0, 0];
        step(184, &mut row);
        // i=0..7 neighborhoods from row (with wrap):
        // i=0: l=0 s=0 r=0 -> 000 -> 0
        // i=1: l=0 s=0 r=0 -> 0
        // i=2: l=0 s=0 r=1 -> 001 -> 0
        // i=3: l=0 s=1 r=1 -> 011 -> 1
        // i=4: l=1 s=1 r=1 -> 111 -> 1
        // i=5: l=1 s=1 r=0 -> 110 -> 0
        // i=6: l=1 s=0 r=0 -> 100 -> 1
        // i=7: l=0 s=0 r=0 -> 0
        assert_eq!(row, [0, 0, 0, 1, 1, 0, 1, 0]);
    }

    #[test]
    fn step_empty_row_is_noop() {
        let mut row: [u8; 0] = [];
        step(30, &mut row); // should not panic
        assert_eq!(row.len(), 0);
    }

    #[test]
    fn render_produces_expected_lines_count() {
        let s = render(30, &[], 5);
        assert_eq!(s.lines().count(), 5);
    }

    #[test]
    fn render_rule_30_known_first_line() {
        // Empty seed -> default width 79 with center cell as 1.
        let s = render(30, &[], 3);
        let first = s.lines().next().unwrap();
        assert_eq!(first.len(), DEFAULT_WIDTH);
        assert_eq!(first.chars().nth(DEFAULT_WIDTH / 2).unwrap(), '#');
        // First line is the seed: exactly one '#'.
        assert_eq!(first.chars().filter(|c| *c == '#').count(), 1);
    }

    #[test]
    fn render_produces_class_3_chaotic_rule_30() {
        // Rule 30 is Wolfram class III (chaotic). Use the default 79-wide row
        // so density has room to vary. Skip the seed row; for the remaining
        // generations, the density must vary (not uniform).
        let s = render(30, &[], 50);
        let lines: Vec<&str> = s.lines().collect();
        let densities: Vec<usize> = lines
            .iter()
            .skip(1)
            .map(|l| l.chars().filter(|c| *c == '#').count())
            .collect();
        let max_ones = *densities.iter().max().unwrap_or(&0);
        let min_ones = *densities.iter().min().unwrap_or(&0);
        assert!(max_ones > min_ones, "rule 30 density should vary");
        assert!(max_ones >= 5, "rule 30 should reach nontrivial density");
    }

    #[test]
    fn rule_0_yields_uniform_zero() {
        // After one step, rule 0 produces all zeros (the seed row is preserved).
        let s = render(0, &[1, 1, 1, 1, 1], 4);
        let lines: Vec<&str> = s.lines().collect();
        for line in &lines[1..] {
            assert!(
                !line.contains('#'),
                "rule 0 step should be all zeros: {:?}",
                line
            );
        }
    }

    #[test]
    fn rule_255_yields_uniform_one() {
        // After one step, rule 255 produces all ones (the seed row is preserved).
        let s = render(255, &[0, 0, 0, 0, 0], 3);
        let lines: Vec<&str> = s.lines().collect();
        for line in &lines[1..] {
            assert!(
                !line.contains(' '),
                "rule 255 step should be all ones: {:?}",
                line
            );
        }
    }
}
