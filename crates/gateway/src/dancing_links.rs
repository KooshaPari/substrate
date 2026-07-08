//! Dancing Links (DLX) — Donald Knuth's Algorithm X implementation using
//! doubly-linked circular lists for exact cover problems.
//!
//! Classic use case: solving exact-cover puzzles such as pentomino tiling,
//! Sudoku, or N-queens. The structure supports O(1) uncover / cover
//! operations allowing backtracking search to run efficiently.
//!
//! Pure safe Rust. No `unsafe`, no external crates.

/// A column header carries the per-column count; data nodes just point here.
#[derive(Debug, Clone, Copy)]
struct Node {
    up: usize,
    down: usize,
    left: usize,
    right: usize,
    /// Column header index, or `usize::MAX` for the root sentinel.
    col: usize,
    /// Row id; `usize::MAX` for column headers.
    row: usize,
}

#[derive(Debug, Clone, Copy)]
struct ColHeader {
    size: u32,
}

/// Counts as data nodes (no extra column bookkeeping); size lives in `cols`.
#[derive(Debug)]
pub struct Dlx {
    nodes: Vec<Node>,
    /// `cols[i]` is the header for column i. Index aligned with `Node.col`.
    cols: Vec<ColHeader>,
    /// Sentinel/root node index.
    root: usize,
}

impl Dlx {
    /// Build a DLX solver with `n` columns (0..n).
    pub fn new(n: usize) -> Self {
        let mut dlx = Dlx {
            nodes: Vec::with_capacity(1 + n + 4),
            cols: Vec::with_capacity(n),
            root: 0,
        };
        dlx.nodes.push(Node {
            up: 0, down: 0, left: 0, right: 0,
            col: usize::MAX, row: usize::MAX,
        });
        for c in 0..n {
            dlx.nodes.push(Node {
                up: c + 1, down: c + 1,
                left: c, right: c + 2,
                col: c + 1, row: usize::MAX,
            });
            dlx.cols.push(ColHeader { size: 0 });
        }
        if n > 0 {
            // Close the circular horizontal list through the root.
            let first_col = 1;
            let last_col = n;
            dlx.nodes[dlx.root].right = first_col;
            dlx.nodes[dlx.root].left = last_col;
            dlx.nodes[first_col].left = dlx.root;
            dlx.nodes[last_col].right = dlx.root;
        }
        dlx
    }

    /// Add a row that covers the given column indices.
    pub fn add_row(&mut self, row: usize, cols: &[usize]) {
        let mut prev = usize::MAX;
        for &c in cols {
            let col_idx = c + 1;
            // Find the last data node currently in column `c` (= self.nodes[col_idx].up).
            let bottom = self.nodes[col_idx].up;
            let nidx = self.nodes.len();
            // Insert between bottom and col_idx (vertically).
            self.nodes.push(Node {
                up: bottom,
                down: col_idx,
                left: 0, right: 0,
                col: col_idx, row,
            });
            self.nodes[bottom].down = nidx;
            self.nodes[col_idx].up = nidx;
            self.cols[c].size += 1;

            if prev == usize::MAX {
                // Single-node row becomes its own circular horizontal list.
                self.nodes[nidx].left = nidx;
                self.nodes[nidx].right = nidx;
            } else {
                // Insert into the horizontal list between prev and prev.right.
                let next = self.nodes[prev].right;
                self.nodes[nidx].left = prev;
                self.nodes[nidx].right = next;
                self.nodes[prev].right = nidx;
                self.nodes[next].left = nidx;
            }
            prev = nidx;
        }
    }

    fn cover(&mut self, c: usize) {
        let mut i = self.nodes[c].down;
        while i != c {
            let mut j = self.nodes[i].right;
            while j != i {
                let col_j = self.nodes[j].col;
                let up_j = self.nodes[j].up;
                let down_j = self.nodes[j].down;
                self.nodes[up_j].down = down_j;
                self.nodes[down_j].up = up_j;
                let col_idx = col_j - 1;
                self.cols[col_idx].size = self.cols[col_idx].size.saturating_sub(1);
                j = self.nodes[j].right;
            }
            i = self.nodes[i].down;
        }
        let l = self.nodes[c].left;
        let r = self.nodes[c].right;
        self.nodes[l].right = r;
        self.nodes[r].left = l;
    }

    fn uncover(&mut self, c: usize) {
        let l = self.nodes[c].left;
        let r = self.nodes[c].right;
        self.nodes[l].right = c;
        self.nodes[r].left = c;
        let mut i = self.nodes[c].up;
        while i != c {
            let mut j = self.nodes[i].left;
            while j != i {
                let col_j = self.nodes[j].col;
                let up_j = self.nodes[j].up;
                let down_j = self.nodes[j].down;
                self.nodes[up_j].down = j;
                self.nodes[down_j].up = j;
                let col_idx = col_j - 1;
                self.cols[col_idx].size = self.cols[col_idx].size.saturating_add(1);
                j = self.nodes[j].left;
            }
            i = self.nodes[i].up;
        }
    }

    /// Search for one exact cover; returns the row ids forming the solution
    /// (empty vector means no cover was found).
    pub fn search(&mut self) -> Vec<usize> {
        let mut solution = Vec::new();
        self.search_inner(&mut solution);
        solution
    }

    fn search_inner(&mut self, solution: &mut Vec<usize>) -> bool {
        if self.nodes[self.root].right == self.root {
            return true;
        }
        // Choose column with the smallest size (heuristic).
        let mut c = self.nodes[self.root].right;
        let mut best = c;
        let mut min_size = self.cols[self.nodes[c].col - 1].size;
        loop {
            let nxt = self.nodes[c].right;
            if nxt == self.root {
                break;
            }
            let sz = self.cols[self.nodes[nxt].col - 1].size;
            if sz < min_size {
                min_size = sz;
                best = nxt;
            }
            c = nxt;
        }
        if min_size == 0 {
            return false;
        }
        self.cover(best);
        let mut r = self.nodes[best].down;
        while r != best {
            solution.push(self.nodes[r].row);
            let mut j = self.nodes[r].right;
            while j != r {
                self.cover(self.nodes[j].col);
                j = self.nodes[j].right;
            }
            if self.search_inner(solution) {
                return true;
            }
            solution.pop();
            let mut j = self.nodes[r].left;
            while j != r {
                self.uncover(self.nodes[j].col);
                j = self.nodes[j].left;
            }
            r = self.nodes[r].down;
        }
        self.uncover(best);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_example() -> Dlx {
        //   A: 0, 1, 4
        //   B: 1, 3
        //   C: 2, 3
        //   D: 1, 2
        //   E: 3, 4
        // Solution: {A,B,C} covers 0,1,3,2,4 -> all 5 columns.
        let mut dlx = Dlx::new(5);
        dlx.add_row(0, &[0, 1, 4]);
        dlx.add_row(1, &[1, 3]);
        dlx.add_row(2, &[2, 3]);
        dlx.add_row(3, &[1, 2]);
        dlx.add_row(4, &[3, 4]);
        dlx
    }

    #[test]
    fn finds_cover_solution() {
        let mut dlx = build_example();
        let sol = dlx.search();
        // Verifies a valid cover was found; multiple solutions may exist
        // (e.g. {0,2} covers cols 0,1,2,3,4 as A+C).
        assert!(!sol.is_empty());
        assert_eq!(sol.len(), 2);
    }

    #[test]
    fn no_solution_when_underdetermined() {
        let mut dlx = Dlx::new(1);
        let sol = dlx.search();
        assert!(sol.is_empty());
    }

    #[test]
    fn singleton_row_solution() {
        let mut dlx = Dlx::new(2);
        dlx.add_row(7, &[0, 1]);
        let sol = dlx.search();
        assert_eq!(sol, vec![7]);
    }

    #[test]
    fn no_solution_when_overconstrained() {
        let mut dlx = Dlx::new(2);
        dlx.add_row(0, &[0]);
        dlx.add_row(1, &[0]);
        dlx.add_row(2, &[0]);
        let sol = dlx.search();
        assert!(sol.is_empty());
    }

    #[test]
    fn distinct_columns_picked() {
        let mut dlx = Dlx::new(3);
        dlx.add_row(10, &[0]);
        dlx.add_row(11, &[1]);
        dlx.add_row(12, &[2]);
        let sol = dlx.search();
        assert_eq!(sol.len(), 3);
    }
}
