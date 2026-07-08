//! A* search on weighted graphs.
//!
//! Given a graph (adjacency list with non-negative edge weights), a start
//! node, a goal-test predicate, and an admissible heuristic function, A*
//! finds a minimum-cost path from `start` to any node satisfying `is_goal`.
//!
//! Returns the reconstructed path (start..goal) and its total cost, or
//! `None` if no path exists.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::hash::Hash;

/// Default graph type: a map from node-id to a list of (neighbor, cost) pairs.
pub type Adjacency<N, C> = HashMap<N, Vec<(N, C)>>;

/// Run A* over `graph`. `start` is the source. `is_goal(&n)` tests each
/// expanded node. `heuristic(&n)` returns an admissible lower bound on the
/// cost from `n` to the nearest goal.
///
/// `C` must implement `Ord + Copy + Default + Add<Output = C>`. We use
/// `Default` as a sentinel "infinity" — if `node_cost + edge_cost` would
/// overflow, the entry is rejected. Callers should use a sufficiently wide
/// numeric type (e.g. `u32`, `u64`, or a wrapping big-int).
pub fn astar<N, C, FG, FH>(
    graph: &Adjacency<N, C>,
    start: N,
    is_goal: FG,
    heuristic: FH,
) -> Option<(Vec<N>, C)>
where
    N: Hash + Eq + Clone + Ord,
    C: Ord + Copy + Default + std::ops::Add<Output = C>,
    FG: Fn(&N) -> bool,
    FH: Fn(&N) -> C,
{
    let zero: C = C::default();
    let mut came_from: HashMap<N, N> = HashMap::new();
    let mut g_score: HashMap<N, C> = HashMap::new();
    g_score.insert(start.clone(), zero);

    let mut open: BinaryHeap<(Reverse<C>, N)> = BinaryHeap::new();
    let h0 = heuristic(&start);
    open.push((Reverse(h0), start.clone()));

    while let Some((Reverse(_f), current)) = open.pop() {
        if is_goal(&current) {
            let total = *g_score.get(&current).unwrap_or(&zero);
            let path = reconstruct(&came_from, &current);
            return Some((path, total));
        }
        let cur_g = *g_score.get(&current).unwrap_or(&zero);
        if let Some(neighbors) = graph.get(&current) {
            for (next, edge_cost) in neighbors {
                let tentative = cur_g + *edge_cost;
                let prev = g_score.get(next).copied().unwrap_or(zero);
                if !g_score.contains_key(next) || tentative < prev {
                    came_from.insert(next.clone(), current.clone());
                    g_score.insert(next.clone(), tentative);
                    let h = heuristic(next);
                    let f = tentative + h;
                    open.push((Reverse(f), next.clone()));
                }
            }
        }
    }
    None
}

fn reconstruct<N: Clone + Hash + Eq>(came_from: &HashMap<N, N>, node: &N) -> Vec<N> {
    let mut path = vec![node.clone()];
    let mut cur = node;
    while let Some(prev) = came_from.get(cur) {
        path.push(prev.clone());
        cur = prev;
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_graph(walls: &[(i32, i32)]) -> Adjacency<(i32, i32), u32> {
        let mut g: Adjacency<(i32, i32), u32> = HashMap::new();
        for x in 0..5 {
            for y in 0..5 {
                if walls.contains(&(x, y)) {
                    continue;
                }
                let mut adj = Vec::new();
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= 5 || ny >= 5 {
                        continue;
                    }
                    if walls.contains(&(nx, ny)) {
                        continue;
                    }
                    adj.push(((nx, ny), 1u32));
                }
                g.insert((x, y), adj);
            }
        }
        g
    }

    fn manhattan(p: &(i32, i32)) -> u32 {
        (p.0.abs() + p.1.abs()) as u32
    }

    #[test]
    fn straight_line_path() {
        let g = grid_graph(&[]);
        let result = astar(&g, (0, 0), |n| *n == (4, 0), |n| manhattan(n));
        let (path, cost) = result.expect("path should exist");
        assert_eq!(path.first(), Some(&(0, 0)));
        assert_eq!(path.last(), Some(&(4, 0)));
        assert_eq!(cost, 4);
    }

    #[test]
    fn detour_around_wall() {
        let walls = vec![(1, 0), (1, 1), (1, 2), (1, 3)];
        let g = grid_graph(&walls);
        let result = astar(&g, (0, 0), |n| *n == (4, 0), |n| manhattan(n));
        let (path, cost) = result.expect("path should exist");
        assert!(cost >= 12);
        assert_eq!(path.first().copied(), Some((0, 0)));
        assert_eq!(path.last().copied(), Some((4, 0)));
    }

    #[test]
    fn no_path_when_start_isolated() {
        let g = grid_graph(&[]);
        let result = astar(&g, (0, 0), |n| *n == (10, 10), |n| manhattan(n));
        assert!(result.is_none());
    }

    #[test]
    fn start_is_goal() {
        let g = grid_graph(&[]);
        let result = astar(&g, (2, 2), |n| *n == (2, 2), |_| 0u32);
        let (path, cost) = result.expect("start is its own goal");
        assert_eq!(path, vec![(2, 2)]);
        assert_eq!(cost, 0);
    }

    #[test]
    fn weighted_edges_chooses_cheaper_path() {
        let mut g: Adjacency<&str, u32> = HashMap::new();
        g.insert("A", vec![("B", 10), ("C", 1)]);
        g.insert("B", vec![("D", 1)]);
        g.insert("C", vec![("D", 100)]);
        g.insert("D", vec![]);
        let result = astar(&g, "A", |n| *n == "D", |_| 0u32);
        let (path, cost) = result.expect("path exists");
        // A→B→D = 11, A→C→D = 101
        assert_eq!(cost, 11);
        assert_eq!(path.first().copied(), Some("A"));
        assert_eq!(path.last().copied(), Some("D"));
    }

    #[test]
    fn heuristic_admissible_returns_optimal() {
        let g = grid_graph(&[]);
        let result = astar(&g, (0, 0), |n| *n == (4, 4), |n| manhattan(n));
        let (_path, cost) = result.expect("path exists");
        // Optimal Manhattan distance is 8.
        assert_eq!(cost, 8);
    }

    #[test]
    fn single_node_graph() {
        let mut g: Adjacency<i32, u32> = HashMap::new();
        g.insert(42, vec![]);
        let result = astar(&g, 42, |n| *n == 42, |_| 0u32);
        let (path, cost) = result.expect("goal is the start");
        assert_eq!(path, vec![42]);
        assert_eq!(cost, 0);
    }

    #[test]
    fn empty_graph_returns_none() {
        let g: Adjacency<i32, u32> = HashMap::new();
        // Start is not in the empty graph; only an unreachable goal returns None.
        let result = astar(&g, 42, |n| *n == 99, |_| 0u32);
        assert!(result.is_none());
    }
}