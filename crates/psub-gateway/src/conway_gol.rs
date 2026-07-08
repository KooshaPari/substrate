//! Conway's Game of Life on a 2-D bounded toroidal grid.
//!
//! The Game of Life is a cellular automaton devised by John Conway in 1970
//! on a square grid where each cell is either alive or dead. At every step:
//!
//! 1. Any live cell with 2 or 3 live neighbours survives.
//! 2. Any dead cell with exactly 3 live neighbours becomes alive.
//! 3. All other live cells die and all other dead cells stay dead.
//!
//! "Neighbour" is the standard Moore neighbourhood — the 8 cells sharing an
//! edge or corner.
//!
//! This implementation stores the grid as a row-major `Vec<bool>` of fixed
//! `width * height` cells and applies toroidal wraparound (i.e. the right edge
//! neighbours the left edge, and the bottom edge neighbours the top).
//!
//! Reference: Gardner, "Mathematical Games: The fantastic combinations of
//! John Conway's new solitaire game 'Life'" (Scientific American, October 1970).

/// Bounded toroidal Conway's Game of Life grid.
#[derive(Clone, Debug)]
pub struct Life {
    width: usize,
    height: usize,
    cells: Vec<bool>,
}

impl Life {
    /// Create an empty grid of the given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![false; width * height],
        }
    }

    /// Create a grid with the given initial state. `cells` is interpreted
    /// row-major and must equal `width * height` in length.
    ///
    /// # Panics
    /// Panics if `cells.len() != width * height`.
    pub fn from_state(width: usize, height: usize, cells: Vec<bool>) -> Self {
        assert_eq!(
            cells.len(),
            width * height,
            "cells length {} does not match width * height = {}",
            cells.len(),
            width * height
        );
        Self {
            width,
            height,
            cells,
        }
    }

    /// Grid width in cells.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Grid height in cells.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Return the current state of cell `(x, y)` (toroidal indices).
    ///
    /// Negative or oversized indices wrap around.
    ///
    /// # Panics
    /// Panics if the grid has zero width or height.
    pub fn get(&self, x: isize, y: isize) -> bool {
        let w = self.width as isize;
        let h = self.height as isize;
        assert!(w > 0 && h > 0, "grid must have positive dimensions");
        let xx = x.rem_euclid(w) as usize;
        let yy = y.rem_euclid(h) as usize;
        self.cells[yy * self.width + xx]
    }

    /// Set cell `(x, y)` (toroidal indices).
    ///
    /// # Panics
    /// Panics if the grid has zero width or height.
    pub fn set(&mut self, x: isize, y: isize, alive: bool) {
        let w = self.width as isize;
        let h = self.height as isize;
        assert!(w > 0 && h > 0, "grid must have positive dimensions");
        let xx = x.rem_euclid(w) as usize;
        let yy = y.rem_euclid(h) as usize;
        self.cells[yy * self.width + xx] = alive;
    }

    /// Toggle cell `(x, y)`.
    pub fn toggle(&mut self, x: isize, y: isize) {
        let w = self.width as isize;
        let h = self.height as isize;
        assert!(w > 0 && h > 0, "grid must have positive dimensions");
        let xx = x.rem_euclid(w) as usize;
        let yy = y.rem_euclid(h) as usize;
        let idx = yy * self.width + xx;
        self.cells[idx] = !self.cells[idx];
    }

    /// Clear all cells.
    pub fn clear(&mut self) {
        for c in &mut self.cells {
            *c = false;
        }
    }

    /// Borrow the underlying row-major buffer.
    pub fn as_slice(&self) -> &[bool] {
        &self.cells
    }

    /// Count the live cells in the current generation.
    pub fn population(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }

    /// Return the Moore-neighbour count for cell `(x, y)`.
    fn neighbour_count(&self, x: usize, y: usize) -> u8 {
        let w = self.width;
        let h = self.height;
        let mut n = 0u8;
        for dy in [-1isize, 0, 1] {
            for dx in [-1isize, 0, 1] {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x as isize + dx).rem_euclid(w as isize) as usize;
                let ny = (y as isize + dy).rem_euclid(h as isize) as usize;
                if self.cells[ny * w + nx] {
                    n += 1;
                }
            }
        }
        n
    }

    /// Advance the grid by exactly one generation.
    pub fn step(&mut self) {
        let w = self.width;
        let h = self.height;
        let mut next = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                let n = self.neighbour_count(x, y);
                let alive = self.cells[y * w + x];
                next[y * w + x] = matches!((alive, n), (true, 2) | (true, 3) | (false, 3));
            }
        }
        self.cells = next;
    }

    /// Advance by `n` generations, returning a snapshot of populations.
    pub fn step_n(&mut self, n: usize) -> Vec<usize> {
        let mut pops = Vec::with_capacity(n + 1);
        pops.push(self.population());
        for _ in 0..n {
            self.step();
            pops.push(self.population());
        }
        pops
    }
}

/// A few well-known Conway's Life patterns for tests and demos.
pub mod patterns {
    /// A still life: a 2x2 block.
    pub fn block() -> Vec<(isize, isize)> {
        vec![(0, 0), (1, 0), (0, 1), (1, 1)]
    }

    /// A blinker (period-2 oscillator) placed at the origin.
    pub fn blinker() -> Vec<(isize, isize)> {
        vec![(0, 0), (1, 0), (2, 0)]
    }

    /// A glider oriented to drift toward the upper-right (SE on screen).
    pub fn glider() -> Vec<(isize, isize)> {
        vec![(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)]
    }

    /// Place `cells` into a fresh `width x height` grid.
    pub fn place(cells: &[(isize, isize)], width: usize, height: usize) -> super::Life {
        let mut g = super::Life::new(width, height);
        for &(x, y) in cells {
            g.set(x, y, true);
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::patterns::{blinker, block, glider};
    use super::*;

    #[test]
    fn new_is_empty() {
        let g = Life::new(5, 5);
        assert_eq!(g.population(), 0);
        assert_eq!(g.width(), 5);
        assert_eq!(g.height(), 5);
    }

    #[test]
    fn set_and_get() {
        let mut g = Life::new(4, 4);
        g.set(2, 3, true);
        assert!(g.get(2, 3));
        assert!(!g.get(0, 0));
    }

    #[test]
    fn toroidal_wrap() {
        let mut g = Life::new(4, 4);
        g.set(-1, 0, true);
        assert!(g.get(3, 0));
        g.set(0, -1, true);
        assert!(g.get(0, 3));
        g.set(4, 4, true);
        assert!(g.get(0, 0));
    }

    #[test]
    fn block_is_still_life() {
        let mut g = patterns::place(&block(), 4, 4);
        let pop = g.population();
        for _ in 0..10 {
            g.step();
            assert_eq!(g.population(), pop);
        }
    }

    #[test]
    fn blinker_oscillates_period_2() {
        let mut g = patterns::place(&blinker(), 5, 5);
        let pop = g.population();
        assert_eq!(pop, 3);
        // After 1 step, horizontal blinker becomes vertical.
        g.step();
        assert_eq!(g.population(), 3);
        // Find a vertical column of 3 live cells.
        let mut cols = [0usize; 5];
        for y in 0..5 {
            for x in 0..5 {
                if g.get(x as isize, y as isize) {
                    cols[x] += 1;
                }
            }
        }
        assert!(cols.iter().any(|&c| c == 3));
        // After 2 steps it should be horizontal again.
        g.step();
        let mut rows = [0usize; 5];
        for y in 0..5 {
            for x in 0..5 {
                if g.get(x as isize, y as isize) {
                    rows[y] += 1;
                }
            }
        }
        assert!(rows.iter().any(|&c| c == 3));
    }

    #[test]
    fn glider_drifts_and_growth() {
        let mut g = patterns::place(&glider(), 10, 10);
        let start_pop = g.population();
        assert_eq!(start_pop, 5);
        g.step_n(4);
        // A glider moves 1 cell per 4 generations; after 4 steps it should
        // have shifted diagonally. Population stays at 5 indefinitely.
        assert_eq!(g.population(), 5);
    }

    #[test]
    fn toggle_flips_cell() {
        let mut g = Life::new(3, 3);
        assert!(!g.get(1, 1));
        g.toggle(1, 1);
        assert!(g.get(1, 1));
        g.toggle(1, 1);
        assert!(!g.get(1, 1));
    }

    #[test]
    fn clear_resets_state() {
        let mut g = Life::new(4, 4);
        g.set(0, 0, true);
        g.set(2, 3, true);
        assert_eq!(g.population(), 2);
        g.clear();
        assert_eq!(g.population(), 0);
    }

    #[test]
    fn as_slice_is_row_major() {
        let mut g = Life::new(3, 2);
        g.set(0, 0, true);
        g.set(2, 1, true);
        let s = g.as_slice();
        assert_eq!(s.len(), 6);
        assert!(s[0]);
        assert!(s[5]);
        assert!(!s[3]);
    }

    #[test]
    fn from_state_panics_on_size_mismatch() {
        let result = std::panic::catch_unwind(|| {
            Life::from_state(3, 3, vec![false; 5]);
        });
        assert!(result.is_err());
    }

    #[test]
    fn step_n_returns_population_curve() {
        let mut g = patterns::place(&glider(), 8, 8);
        let pops = g.step_n(8);
        assert_eq!(pops.len(), 9);
        // Glider is a stable population-5 pattern forever.
        for &p in &pops {
            assert_eq!(p, 5);
        }
    }

    #[test]
    fn blinker_toroidal_returns_after_period() {
        // On a small toroidal board a blinker may run into itself; here we
        // use a large enough grid that the period stays 2.
        let mut g = patterns::place(&blinker(), 7, 7);
        let initial_state: Vec<bool> = g.as_slice().to_vec();
        g.step();
        g.step();
        assert_eq!(g.as_slice(), initial_state.as_slice());
    }

    #[test]
    fn empty_grid_stays_empty() {
        let mut g = Life::new(8, 8);
        for _ in 0..5 {
            g.step();
            assert_eq!(g.population(), 0);
        }
    }
}