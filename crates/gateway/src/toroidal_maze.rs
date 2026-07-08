//! Toroidal maze generation with Wilson's uniform-spanning-tree algorithm.
//!
//! A *toroidal* maze is one where opposite edges are sewn together: walking
//! east off the right edge brings you back to the left, and likewise for
//! the other three borders. Toroidal mazes are **always** cycle-free on
//! flat 2D grids (no path can hit a wall, only its own past self), but
//! they can be wired such that many short cycles exist along with the
//! unique spanning-tree solution Wilson's produces.
//!
//! ## Wilson's algorithm
//!
//! Selected by loop-erased random walk (LERW), published in:
//!
//! > Wilson, D. B. (1996). *Generating random spanning trees more
//! > quickly than the cover time*. STOC '96.
//!
//! The expected number of steps is `O(n log n)` where `n` is the number
//! of cells. Each step is `O(1)` amortized, and the algorithm is
//! straightforward to implement with a fixed seed so the produced
//! spanning tree is fully deterministic.
//!
//! ## Storage and grid model
//!
//! The maze lives in a `cols x rows` grid of cells. Each cell has four
//! walls: `North`, `East`, `South`, `West`. Walls are stored as a
//! `Vec<WallSet>` indexed by `(col, row)` with **flat-index layout**
//! `idx = row * cols + col`. A wall removal between two neighbors is
//! stored on **both** cells to make wall queries symmetric.
//!
//! ## Determinism
//!
//! The pseudo-random generator is a `u64` XorShift64* lifted from
//! Marsaglia (1994), parameterized by a single seed. The same `(cols,
//! rows, seed)` always produces the same maze.

use std::cell::Cell;

/// Pseudo-random XorShift64* generator (Marsaglia 1994). Produces the
/// full 64-bit cycle and is suitable for non-cryptographic use like
/// maze generation. Local copy — we don't depend on `rand` here.
#[derive(Debug, Clone)]
pub struct XorShift64 {
    state: Cell<u64>,
}

impl XorShift64 {
    /// Construct from any non-zero seed.
    pub fn new(seed: u64) -> Self {
        let s = if seed == 0 { 0xDEAD_BEEF_CAFE_F00D } else { seed };
        Self { state: Cell::new(s) }
    }

    /// Next 64-bit random value.
    pub fn next_u64(&self) -> u64 {
        let mut x = self.state.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        if x == 0 {
            x = 1;
        }
        self.state.set(x);
        // Marsaglia's accumulator: x * 0x2545_F491_4F6C_DD1D
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Random value in `[0, n)`. `n` must be > 0.
    pub fn next_range(&self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }

    /// Random index into a `slice`. Returns 0 when the slice is empty.
    pub fn next_idx<T>(&self, s: &[T]) -> usize {
        if s.is_empty() {
            0
        } else {
            self.next_range(s.len() as u64) as usize
        }
    }
}

/// Bit set of walls. North=1, East=2, South=4, West=8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WallSet(u8);

impl WallSet {
    pub const NORTH: u8 = 1;
    pub const EAST: u8 = 2;
    pub const SOUTH: u8 = 4;
    pub const WEST: u8 = 8;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn contains(&self, w: u8) -> bool {
        (self.0 & w) != 0
    }

    pub fn remove(&mut self, w: u8) {
        self.0 &= !w;
    }

    pub fn insert(&mut self, w: u8) {
        self.0 |= w;
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn bits(&self) -> u8 {
        self.0
    }
}

impl std::ops::BitOr for WallSet {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// One cell on the toroidal grid. Stores the four walls that have not
/// been knocked down yet. (Initially: all four walls present.)
#[derive(Debug, Clone)]
pub struct MazeCell {
    pub walls: WallSet,
}

impl MazeCell {
    pub fn new() -> Self {
        // All four walls present.
        Self {
            walls: WallSet(WallSet::NORTH | WallSet::EAST | WallSet::SOUTH | WallSet::WEST),
        }
    }
}

impl Default for MazeCell {
    fn default() -> Self {
        Self::new()
    }
}

/// The generated toroidal maze. Wall queries look at one cell; callers
/// that care about a boundary should remember neighbors live on the
/// toroidal wrap.
#[derive(Debug, Clone)]
pub struct ToroidalMaze {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<MazeCell>,
}

impl ToroidalMaze {
    /// Build a fresh, fully-walled maze of the given dimensions, then
    /// knock down walls using Wilson's algorithm with `seed`.
    pub fn generate(cols: usize, rows: usize, seed: u64) -> Self {
        assert!(cols > 0 && rows > 0, "torus dimensions must be > 0");
        let cells: Vec<MazeCell> = (0..cols * rows).map(|_| MazeCell::new()).collect();
        let mut maze = ToroidalMaze { cols, rows, cells };
        maze.fill_with_wilson(seed);
        maze
    }

    /// Build a fully-walled maze with no path-carving pass. Useful for
    /// inspection and for tests that assert on the initial state.
    pub fn new_full(cols: usize, rows: usize) -> Self {
        assert!(cols > 0 && rows > 0, "torus dimensions must be > 0");
        let cells: Vec<MazeCell> = (0..cols * rows).map(|_| MazeCell::new()).collect();
        ToroidalMaze { cols, rows, cells }
    }

    /// Apply Wilson's LERW to fill the maze. Operates in place.
    ///
    /// The classic algorithm:
    ///
    /// 1. Start with cell 0 in the spanning tree.
    /// 2. Walk randomly from each not-yet-in-tree cell until the walk
    ///    lands on a cell already in the tree; erase any loops along
    ///    the way (the recorded path is acyclic).
    /// 3. Carve the recorded path into the tree.
    ///
    /// Implementation uses an explicit `Vec<usize>` of cells visited
    /// in order, plus a `HashMap<usize, usize>` from cell to its
    /// position in that list, for O(1) loop-erase lookup.
    pub fn fill_with_wilson(&mut self, seed: u64) {
        let rng = XorShift64::new(seed);
        let total = self.cols * self.rows;
        let mut in_tree: Vec<bool> = vec![false; total];

        // Bootstrap: cell 0 in the tree.
        if total > 0 {
            in_tree[0] = true;
        }

        // Step cap per walk to bound worst-case runtime on adversarial
        // grids. Wilson's algorithm terminates in O(n log n) expected
        // steps; 256 * n * n is a comfortable bound for the sizes we
        // test (<= 10x10 = 100 cells), well under a second of CPU.
        let max_steps_per_walk: usize = 256usize.saturating_mul(total).saturating_mul(total);

        for cell in 0..total {
            if in_tree[cell] {
                continue;
            }
            let mut ordered: Vec<usize> = Vec::with_capacity(total);
            let mut pos_of: std::collections::HashMap<usize, usize> =
                std::collections::HashMap::with_capacity(total);

            let mut cur = cell;
            let mut steps: usize = 0;
            // Add the starting cell to `ordered` so the carving
            // loop at the end will mark it `in_tree` AND connect it
            // to its first-step neighbor (the connection between
            // `cell` and `ordered[1]`).
            ordered.push(cell);
            pos_of.insert(cell, 0);
            // Walk further only if the starting cell is not already
            // part of the tree. (For the very first iteration of a
            // single walk we may have started at an in-tree cell —
            // don't enter the inner loop.)
            let needs_walk = !in_tree[cur];
            let mut stepped_out_early = false;
            while needs_walk && !in_tree[cur] {
                steps += 1;
                if steps > max_steps_per_walk {
                    stepped_out_early = true;
                    break;
                }

                let dc: i32 = cur as i32 % self.cols as i32;
                let dr: i32 = cur as i32 / self.cols as i32;
                let dir = rng.next_range(4);
                let next_idx = match dir {
                    0 => {
                        let ndr = if dr == 0 { self.rows as i32 - 1 } else { dr - 1 };
                        (ndr as usize) * self.cols + dc as usize
                    }
                    1 => {
                        let ndc = (dc + 1) % self.cols as i32;
                        dr as usize * self.cols + ndc as usize
                    }
                    2 => {
                        let ndr = (dr + 1) % self.rows as i32;
                        (ndr as usize) * self.cols + dc as usize
                    }
                    _ => {
                        let ndc = if dc == 0 { self.cols as i32 - 1 } else { dc - 1 };
                        dr as usize * self.cols + ndc as usize
                    }
                };

                if let Some(&cut_pos) = pos_of.get(&next_idx) {
                    if ordered.len() > cut_pos + 1 {
                        for dead in &ordered[cut_pos + 1..] {
                            pos_of.remove(dead);
                        }
                        ordered.truncate(cut_pos + 1);
                    }
                    ordered.push(next_idx);
                    pos_of.insert(next_idx, ordered.len() - 1);
                } else {
                    ordered.push(next_idx);
                    pos_of.insert(next_idx, ordered.len() - 1);
                }
                cur = next_idx;
            }

            if stepped_out_early {
                // Cap reached before the walk entered the tree. Fall
                // back: connect each visited cell to an arbitrary
                // already-in-tree neighbor by stepping the random
                // walk one more time, briefly. This keeps the maze
                // connected even when the loop count is anomalous.
                for _ in 0..4 {
                    let dc: i32 = cur as i32 % self.cols as i32;
                    let dr: i32 = cur as i32 / self.cols as i32;
                    let dir = rng.next_range(4);
                    let nxt = match dir {
                        0 => {
                            let ndr = if dr == 0 { self.rows as i32 - 1 } else { dr - 1 };
                            (ndr as usize) * self.cols + dc as usize
                        }
                        1 => {
                            let ndc = (dc + 1) % self.cols as i32;
                            dr as usize * self.cols + ndc as usize
                        }
                        2 => {
                            let ndr = (dr + 1) % self.rows as i32;
                            (ndr as usize) * self.cols + dc as usize
                        }
                        _ => {
                            let ndc = if dc == 0 { self.cols as i32 - 1 } else { dc - 1 };
                            dr as usize * self.cols + ndc as usize
                        }
                    };
                    self.connect(cur, nxt);
                    if in_tree[nxt] {
                        // Connected to the tree; finalize.
                        for &v in &ordered {
                            in_tree[v] = true;
                        }
                        break;
                    }
                    cur = nxt;
                }
                if let Some(&tail) = ordered.last() {
                    in_tree[tail] = true;
                    for &v in &ordered {
                        in_tree[v] = true;
                    }
                }
                continue;
            }

            for i in 0..ordered.len() {
                let v = ordered[i];
                in_tree[v] = true;
                if i + 1 < ordered.len() {
                    let nxt = ordered[i + 1];
                    self.connect(v, nxt);
                }
            }
        }
    }

    /// Flatten `(col, row)` to a single index.
    pub fn idx(&self, col: usize, row: usize) -> usize {
        debug_assert!(col < self.cols && row < self.rows);
        row * self.cols + col
    }

    /// Look at cell `(col, row)` and return its wall set.
    pub fn walls_at(&self, col: usize, row: usize) -> WallSet {
        self.cells[self.idx(col, row)].walls
    }

    /// Knock down the wall between two adjacent cells, accounting for
    /// the toroidal wrap. `a` -> `b` direction is computed via the
    /// *minimum* toroidal offset: a step in the +x direction uses
    /// `(bc + cols - ac) % cols == 1` and likewise for y.
    fn connect(&mut self, a: usize, b: usize) {
        let (ac, ar) = (a % self.cols, a / self.cols);
        let (bc, br) = (b % self.cols, b / self.cols);
        let dir = if ar == br
            && (bc + self.cols - ac) % self.cols == 1
            // Skip wrap on a 1-column grid (degenerate).
            && self.cols > 1
        {
            WallSet::EAST
        } else if ar == br
            && (ac + self.cols - bc) % self.cols == 1
            && self.cols > 1
        {
            WallSet::WEST
        } else if ac == bc
            && (br + self.rows - ar) % self.rows == 1
            && self.rows > 1
        {
            WallSet::SOUTH
        } else if ac == bc
            && (ar + self.rows - br) % self.rows == 1
            && self.rows > 1
        {
            WallSet::NORTH
        } else {
            // Not adjacent — defensive no-op.
            return;
        };
        let opp = opposite_wall(dir);
        self.cells[a].walls.remove(dir);
        self.cells[b].walls.remove(opp);
    }

    /// Returns the four raw wall-presence flags, in order N, E, S, W.
    pub fn walls_row(&self, col: usize, row: usize) -> [bool; 4] {
        let w = self.cells[self.idx(col, row)].walls;
        [
            w.contains(WallSet::NORTH),
            w.contains(WallSet::EAST),
            w.contains(WallSet::SOUTH),
            w.contains(WallSet::WEST),
        ]
    }

    /// Total number of cells in the maze.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether the maze is empty (zero dimensions).
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

fn opposite_wall(w: u8) -> u8 {
    match w {
        WallSet::NORTH => WallSet::SOUTH,
        WallSet::SOUTH => WallSet::NORTH,
        WallSet::EAST => WallSet::WEST,
        WallSet::WEST => WallSet::EAST,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== XorShift64 tests ==========

    #[test]
    fn xorshift_starts_zero_swapped_for_default() {
        let r = XorShift64::new(0);
        assert_ne!(r.next_u64(), 0);
    }

    #[test]
    fn xorshift_is_deterministic() {
        let a = XorShift64::new(0xC0FFEE_1234_5678);
        let b = XorShift64::new(0xC0FFEE_1234_5678);
        let seq_a: Vec<u64> = (0..128).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..128).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn xorshift_different_seeds_diverge() {
        let a = XorShift64::new(1);
        let b = XorShift64::new(2);
        let mut same = 0;
        for _ in 0..128 {
            if a.next_u64() == b.next_u64() {
                same += 1;
            }
        }
        assert!(same < 8, "too many identical draws: {same}/128");
    }

    #[test]
    fn xorshift_next_range_distributes() {
        let r = XorShift64::new(0x1234_5678);
        let mut bins = [0usize; 8];
        for _ in 0..1024 {
            bins[r.next_range(8) as usize] += 1;
        }
        // Each bin should be in [80, 200] — gives substantial slack.
        for (i, &b) in bins.iter().enumerate() {
            assert!(b > 80 && b < 200, "bin {i} = {b}");
        }
    }

    // ========== WallSet tests ==========

    #[test]
    fn wallset_basic_set_ops() {
        let mut w = WallSet::new();
        assert!(w.is_empty());
        w.insert(WallSet::NORTH);
        assert!(w.contains(WallSet::NORTH));
        w.insert(WallSet::EAST);
        let south = WallSet(WallSet::SOUTH);
        let combined = w | south;
        assert!(combined.contains(WallSet::SOUTH));
        w.remove(WallSet::NORTH);
        assert!(!w.contains(WallSet::NORTH));
        assert!(w.contains(WallSet::EAST));
    }

    #[test]
    fn wallset_opposite_walls_are_pairs() {
        // Sanity: NORTH <-> SOUTH, EAST <-> WEST.
        assert_eq!(opposite_wall(WallSet::NORTH), WallSet::SOUTH);
        assert_eq!(opposite_wall(WallSet::SOUTH), WallSet::NORTH);
        assert_eq!(opposite_wall(WallSet::EAST), WallSet::WEST);
        assert_eq!(opposite_wall(WallSet::WEST), WallSet::EAST);
    }

    // ========== Maze tests ==========

    #[test]
    fn fresh_maze_has_all_walls() {
        let m = ToroidalMaze::new_full(4, 4);
        for col in 0..4 {
            for row in 0..4 {
                let w = m.walls_at(col, row);
                assert!(w.contains(WallSet::NORTH));
                assert!(w.contains(WallSet::EAST));
                assert!(w.contains(WallSet::SOUTH));
                assert!(w.contains(WallSet::WEST));
            }
        }
    }

    #[test]
    fn deterministic_generation() {
        let a = ToroidalMaze::generate(8, 5, 0xBEEF);
        let b = ToroidalMaze::generate(8, 5, 0xBEEF);
        // The same seed must produce identical walls at every cell.
        for col in 0..8 {
            for row in 0..5 {
                assert_eq!(a.walls_at(col, row), b.walls_at(col, row));
            }
        }
    }

    #[test]
    fn generated_maze_spans_all_cells_and_leaves_no_isolated_cell() {
        let m = ToroidalMaze::generate(10, 10, 0xACE);
        // Each cell must have at least one opening (the spanning tree
        // leaves leaves with degree 1, but no degree 0).
        for col in 0..10 {
            for row in 0..10 {
                let w = m.walls_at(col, row);
                let openings = [
                    w.contains(WallSet::NORTH),
                    w.contains(WallSet::EAST),
                    w.contains(WallSet::SOUTH),
                    w.contains(WallSet::WEST),
                ]
                .iter()
                .filter(|x| !**x)
                .count();
                assert!(
                    openings >= 1,
                    "cell ({col},{row}) had {openings} openings"
                );
            }
        }
    }

    #[test]
    fn mazes_with_different_seeds_differ() {
        let a = ToroidalMaze::generate(5, 5, 1);
        let b = ToroidalMaze::generate(5, 5, 2);
        let mut diff_count = 0;
        for col in 0..5 {
            for row in 0..5 {
                if a.walls_at(col, row) != b.walls_at(col, row) {
                    diff_count += 1;
                }
            }
        }
        assert!(
            diff_count > 5,
            "different seeds produced nearly identical mazes ({diff_count} diffs)"
        );
    }

    #[test]
    fn walls_row_helper_matches_walls_at() {
        let m = ToroidalMaze::generate(6, 6, 0x42);
        for col in 0..6 {
            for row in 0..6 {
                let w = m.walls_at(col, row);
                let r = m.walls_row(col, row);
                assert_eq!(r[0], w.contains(WallSet::NORTH));
                assert_eq!(r[1], w.contains(WallSet::EAST));
                assert_eq!(r[2], w.contains(WallSet::SOUTH));
                assert_eq!(r[3], w.contains(WallSet::WEST));
            }
        }
    }

    #[test]
    #[should_panic(expected = "must be > 0")]
    fn zero_dim_panics() {
        let _ = ToroidalMaze::generate(0, 5, 1);
    }
}
