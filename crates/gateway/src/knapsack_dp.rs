//! 0/1 Knapsack with classic O(n*W) dynamic programming.
//!
//! Given `n` items each with weight `w[i]` and value `v[i]`, choose a subset
//! maximizing total value while total weight <= capacity `W`. Each item is
//! either taken or not (binary choice).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub weight: u32,
    pub value: u32,
}

impl Item {
    pub const fn new(weight: u32, value: u32) -> Self {
        Self { weight, value }
    }
}

/// Returns the maximum total value achievable.
pub fn max_value(items: &[Item], capacity: u32) -> u64 {
    let w = capacity as usize;
    // dp[c] = best value with capacity c using items processed so far.
    let mut dp = vec![0u64; w + 1];
    for item in items {
        // Iterate backward so each item is only considered once.
        for c in (item.weight as usize..=w).rev() {
            let candidate = dp[c - item.weight as usize] + item.value as u64;
            if candidate > dp[c] {
                dp[c] = candidate;
            }
        }
    }
    dp[w]
}

/// Returns `(total_value, total_weight, chosen_indices)` for the optimal
/// solution.
pub fn solve(items: &[Item], capacity: u32) -> Solution {
    let w = capacity as usize;
    let n = items.len();
    // dp[i][c] = best value using first i items with capacity c.
    let mut dp = vec![vec![0u64; w + 1]; n + 1];
    for i in 1..=n {
        let item = items[i - 1];
        let iw = item.weight as usize;
        let iv = item.value as u64;
        for c in 0..=w {
            let without = dp[i - 1][c];
            let with = if c >= iw { dp[i - 1][c - iw] + iv } else { 0 };
            dp[i][c] = without.max(with);
        }
    }
    // Backtrack.
    let mut c = w;
    let mut chosen: Vec<usize> = Vec::new();
    let mut total_weight: u32 = 0;
    for i in (1..=n).rev() {
        if dp[i][c] != dp[i - 1][c] {
            chosen.push(i - 1);
            let iw = items[i - 1].weight as usize;
            total_weight = total_weight.saturating_add(items[i - 1].weight);
            c -= iw;
        }
    }
    chosen.reverse();
    Solution {
        total_value: dp[n][w],
        total_weight,
        chosen,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Solution {
    pub total_value: u64,
    pub total_weight: u32,
    /// Indices into the `items` slice.
    pub chosen: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_example() {
        let items = [
            Item::new(2, 3),
            Item::new(3, 4),
            Item::new(4, 5),
            Item::new(5, 6),
        ];
        // Capacity 5: best = items 0 + 2 (weight 6, doesn't fit) or items 1+0
        // (weight 5, value 7) or item 3 alone (weight 5, value 6). So {0,1}.
        assert_eq!(max_value(&items, 5), 7);
        let s = solve(&items, 5);
        assert_eq!(s.total_value, 7);
        assert_eq!(s.chosen, vec![0, 1]);
        assert_eq!(s.total_weight, 5);
    }

    #[test]
    fn empty_inputs() {
        let items: [Item; 0] = [];
        assert_eq!(max_value(&items, 10), 0);
        let s = solve(&items, 10);
        assert_eq!(s.total_value, 0);
        assert!(s.chosen.is_empty());
        assert_eq!(s.total_weight, 0);
    }

    #[test]
    fn zero_capacity() {
        let items = [Item::new(1, 100), Item::new(2, 200)];
        assert_eq!(max_value(&items, 0), 0);
        let s = solve(&items, 0);
        assert_eq!(s.total_value, 0);
        assert!(s.chosen.is_empty());
    }

    #[test]
    fn take_all_when_fits() {
        let items = [Item::new(1, 1), Item::new(2, 2), Item::new(3, 3)];
        let s = solve(&items, 100);
        assert_eq!(s.total_value, 6);
        assert_eq!(s.chosen, vec![0, 1, 2]);
        assert_eq!(s.total_weight, 6);
    }

    #[test]
    fn greedy_would_fail_classic() {
        // Items (weight, value), capacity 10.
        //   A: (6, 10)  alone value 10
        //   B: (5, 5)   together value 10
        //   C: (5, 5)   together value 10
        // Optimal = 10 (either A alone or B+C), but greedy-by-value also picks A.
        let items = [Item::new(6, 10), Item::new(5, 5), Item::new(5, 5)];
        assert_eq!(max_value(&items, 10), 10);
        let s = solve(&items, 10);
        assert_eq!(s.total_value, 10);
    }

    #[test]
    fn single_high_value_picked() {
        // Capacity 8 holds items with total weight 8. Item 0+1 weights 7+1=8.
        // Item 2 weight 2 cannot fit with the others (would push weight to 10).
        let items = [Item::new(7, 100), Item::new(1, 1), Item::new(2, 2)];
        let s = solve(&items, 8);
        assert_eq!(s.total_value, 101);
        assert_eq!(s.total_weight, 8);
        assert_eq!(s.chosen, vec![0, 1]);
    }
}
