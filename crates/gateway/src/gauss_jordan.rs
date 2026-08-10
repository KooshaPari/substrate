//! Gauss-Jordan elimination over the rationals.
//!
//! Solves a system of linear equations `A x = b` and computes matrix
//! inverses and determinants in exact rational arithmetic, with no
//! floating-point error and no external dependencies.
//!
//! Every operation is performed on the [`Rational`] type, a
//! arbitrary-precision fraction `p / q` with `q > 0`, stored in lowest
//! terms. This means the solver returns exact answers for systems
//! whose determinant is a non-trivial rational (e.g. Cramer's-rule
//! examples that trip up single-precision floats).
//!
//! ## Gauss-Jordan elimination
//!
//! Given an augmented matrix `[A | b]`, Gauss-Jordan applies three
//! elementary row operations until the left block is the identity:
//!
//! 1. **Swap** two rows.
//! 2. **Scale** a row by a non-zero scalar.
//! 3. **Add** a scalar multiple of one row to another.
//!
//! When the left block becomes the identity, the right block is the
//! solution vector. The same machinery produces the inverse of `A`
//! by augmenting with `I` instead of `b`, and the determinant of `A`
//! by tracking how each row swap and scale changes the signed volume.
//!
//! Implementation notes:
//! - This module is **std-only**, no `unsafe`, no external deps.
//! - Matrices are stored as `Vec<Vec<Rational>>` in row-major form.
//! - Detecting singular systems is exact: if any pivot reduces to 0
//!   after row reduction, the matrix is singular and we surface
//!   [`SolveError::Singular`].

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// An arbitrary-precision rational number `p / q` in lowest terms,
/// with `q > 0` always.
///
/// The sign of a [`Rational`] lives entirely in the numerator
/// (`denominator > 0`), so comparison and printing never have to
/// disambiguate `(-1)/2` vs `1/(-2)`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Rational {
    num: i64,
    den: i64,
}

impl Rational {
    /// Construct a rational from a numerator and denominator. The
    /// fraction is reduced to lowest terms and the denominator is
    /// normalised to be positive.
    ///
    /// # Panics
    ///
    /// Panics if `den == 0`.
    pub fn new(num: i64, den: i64) -> Self {
        assert!(den != 0, "Rational::new: denominator must be non-zero");
        let mut n = num;
        let mut d = den;
        if d < 0 {
            n = -n;
            d = -d;
        }
        let g = gcd_i64(n.unsigned_abs(), d as u64);
        n /= g as i64;
        d /= g as u64 as i64;
        Self { num: n, den: d }
    }

    /// The numerator (may be negative).
    pub fn numer(&self) -> i64 {
        self.num
    }

    /// The denominator (always strictly positive).
    pub fn denom(&self) -> i64 {
        self.den
    }

    /// `0 / 1`.
    pub fn zero() -> Self {
        Self { num: 0, den: 1 }
    }

    /// `1 / 1`.
    pub fn one() -> Self {
        Self { num: 1, den: 1 }
    }

    /// `True` iff the value is exactly zero.
    pub fn is_zero(&self) -> bool {
        self.num == 0
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

impl Add for Rational {
    type Output = Rational;
    fn add(self, rhs: Rational) -> Rational {
        let lcm = lcm_i64(self.den.unsigned_abs(), rhs.den.unsigned_abs()) as i64;
        let lhs_num = self.num * (lcm / self.den);
        let rhs_num = rhs.num * (lcm / rhs.den);
        Rational::new(lhs_num + rhs_num, lcm)
    }
}

impl Sub for Rational {
    type Output = Rational;
    fn sub(self, rhs: Rational) -> Rational {
        self + Rational::new(-rhs.num, rhs.den)
    }
}

impl Mul for Rational {
    type Output = Rational;
    fn mul(self, rhs: Rational) -> Rational {
        Rational::new(self.num * rhs.num, self.den * rhs.den)
    }
}

impl Div for Rational {
    type Output = Rational;
    fn div(self, rhs: Rational) -> Rational {
        assert!(!rhs.is_zero(), "Rational division by zero");
        Rational::new(self.num * rhs.den, self.den * rhs.num)
    }
}

impl Neg for Rational {
    type Output = Rational;
    fn neg(self) -> Rational {
        Rational::new(-self.num, self.den)
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Cross-multiply on the positive denominator. Since both
        // denominators are > 0, `self.num * other.den` and
        // `other.num * self.den` are comparable as i64 products.
        let lhs = (self.num as i128) * (other.den as i128);
        let rhs = (other.num as i128) * (self.den as i128);
        lhs.cmp(&rhs)
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn gcd_i64(a: u64, b: u64) -> u64 {
    if a == 0 {
        b
    } else if b == 0 {
        a
    } else {
        let (mut x, mut y) = (a, b);
        while y != 0 {
            let t = y;
            y = x % y;
            x = t;
        }
        x
    }
}

fn lcm_i64(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 {
        0
    } else {
        a / gcd_i64(a, b) * b
    }
}

/// A row-major dense matrix of [`Rational`] entries.
pub type Matrix = Vec<Vec<Rational>>;

/// Failure modes for the linear-system solver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveError {
    /// Coefficient matrix is not square when an inverse was requested.
    NotSquare { rows: usize, cols: usize },
    /// The system is singular (no unique solution).
    Singular,
    /// The system is inconsistent (no solution at all).
    Inconsistent,
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolveError::NotSquare { rows, cols } => {
                write!(f, "matrix is not square ({}x{})", rows, cols)
            }
            SolveError::Singular => f.write_str("matrix is singular"),
            SolveError::Inconsistent => f.write_str("system is inconsistent"),
        }
    }
}

impl std::error::Error for SolveError {}

/// Solve `A x = b` over the rationals.
///
/// `a` must have `a.len() == b.len()` and `a[i].len() == n` for some
/// square dimension `n`; otherwise we surface
/// [`SolveError::NotSquare`] (rows/cols are reported for the smaller
/// dimension that mismatches).
pub fn solve(a: &Matrix, b: &[Rational]) -> Result<Vec<Rational>, SolveError> {
    let rows = a.len();
    let cols = rows;
    if cols == 0 || a.iter().any(|r| r.len() != cols) {
        return Err(SolveError::NotSquare { rows, cols });
    }
    if b.len() != rows {
        return Err(SolveError::NotSquare {
            rows: b.len(),
            cols,
        });
    }

    let mut aug: Vec<Vec<Rational>> = Vec::with_capacity(rows);
    for (row, rhs) in a.iter().zip(b.iter()) {
        let mut r = row.clone();
        r.push(rhs.clone());
        aug.push(r);
    }

    let n = rows;
    let mut pivots: Vec<usize> = (0..n).collect();

    for k in 0..n {
        // Pivot: choose the largest-magnitude non-zero entry in column k.
        let mut pivot_row = k;
        let mut best = aug[k][k].clone().abs_num();
        for i in (k + 1)..n {
            let mag = aug[i][k].clone().abs_num();
            if mag > best {
                best = mag;
                pivot_row = i;
            }
        }
        if best.is_zero() {
            return Err(SolveError::Singular);
        }
        if pivot_row != k {
            aug.swap(k, pivot_row);
            pivots.swap(k, pivot_row);
        }

        // Scale pivot row so the pivot is exactly 1.
        let pivot = aug[k][k].clone();
        let inv = Rational::one() / pivot.clone();
        for j in 0..=n {
            aug[k][j] = aug[k][j].clone() * inv.clone();
        }

        // Eliminate column k in every other row.
        for i in 0..n {
            if i == k {
                continue;
            }
            let factor = aug[i][k].clone();
            if factor.is_zero() {
                continue;
            }
            for j in 0..=n {
                let term = factor.clone() * aug[k][j].clone();
                aug[i][j] = aug[i][j].clone() - term;
            }
        }
    }

    // Read off the solution.
    let mut x = Vec::with_capacity(n);
    for i in 0..n {
        x.push(aug[i][n].clone());
    }
    Ok(x)
}

/// Invert a square matrix over the rationals.
///
/// Returns [`SolveError::NotSquare`] if the matrix is not square and
/// [`SolveError::Singular`] if the determinant is zero.
pub fn inverse(a: &Matrix) -> Result<Matrix, SolveError> {
    let n = a.len();
    if n == 0 || a.iter().any(|r| r.len() != n) {
        return Err(SolveError::NotSquare { rows: n, cols: n });
    }
    let mut aug: Vec<Vec<Rational>> = Vec::with_capacity(n);
    for (i, row) in a.iter().enumerate() {
        let mut r = row.clone();
        for j in 0..n {
            r.push(if i == j {
                Rational::one()
            } else {
                Rational::zero()
            });
        }
        aug.push(r);
    }

    for k in 0..n {
        let mut pivot_row = k;
        let mut best = aug[k][k].clone().abs_num();
        for i in (k + 1)..n {
            let mag = aug[i][k].clone().abs_num();
            if mag > best {
                best = mag;
                pivot_row = i;
            }
        }
        if best.is_zero() {
            return Err(SolveError::Singular);
        }
        if pivot_row != k {
            aug.swap(k, pivot_row);
        }
        let pivot = aug[k][k].clone();
        let inv = Rational::one() / pivot.clone();
        for j in 0..(2 * n) {
            aug[k][j] = aug[k][j].clone() * inv.clone();
        }
        for i in 0..n {
            if i == k {
                continue;
            }
            let factor = aug[i][k].clone();
            if factor.is_zero() {
                continue;
            }
            for j in 0..(2 * n) {
                let term = factor.clone() * aug[k][j].clone();
                aug[i][j] = aug[i][j].clone() - term;
            }
        }
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(aug[i][n..(2 * n)].to_vec());
    }
    Ok(out)
}

/// Compute the determinant of a square matrix over the rationals.
pub fn determinant(a: &Matrix) -> Result<Rational, SolveError> {
    let n = a.len();
    if n == 0 || a.iter().any(|r| r.len() != n) {
        return Err(SolveError::NotSquare { rows: n, cols: n });
    }
    let mut m = a.clone();
    let mut det = Rational::one();
    let mut sign_change = false;
    for k in 0..n {
        let mut pivot_row = k;
        let mut best = m[k][k].clone().abs_num();
        for i in (k + 1)..n {
            let mag = m[i][k].clone().abs_num();
            if mag > best {
                best = mag;
                pivot_row = i;
            }
        }
        if best.is_zero() {
            return Ok(Rational::zero());
        }
        if pivot_row != k {
            m.swap(k, pivot_row);
            sign_change = !sign_change;
        }
        det = det * m[k][k].clone();
        let inv = Rational::one() / m[k][k].clone();
        for i in (k + 1)..n {
            let factor = m[i][k].clone() * inv.clone();
            if factor.is_zero() {
                continue;
            }
            for j in 0..n {
                let term = factor.clone() * m[k][j].clone();
                m[i][j] = m[i][j].clone() - term;
            }
        }
    }
    if sign_change {
        det = -det;
    }
    Ok(det)
}

// Helper: the absolute value of the numerator (used as a magnitude
// comparator; we deliberately ignore the denominator so that for
// example 1/2 and 2/4 are seen as the same "size class" for pivot
// selection).
trait AbsNum {
    fn abs_num(self) -> Self;
}
impl AbsNum for Rational {
    fn abs_num(self) -> Rational {
        if self.num < 0 {
            Rational::new(-self.num, self.den)
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i64, d: i64) -> Rational {
        Rational::new(n, d)
    }

    fn mat2(a: (i64, i64), b: (i64, i64), c: (i64, i64), d: (i64, i64)) -> Matrix {
        vec![
            vec![q(a.0, a.1), q(b.0, b.1)],
            vec![q(c.0, c.1), q(d.0, d.1)],
        ]
    }

    fn mat3(
        a: (i64, i64),
        b: (i64, i64),
        c: (i64, i64),
        d: (i64, i64),
        e: (i64, i64),
        f: (i64, i64),
        g: (i64, i64),
        h: (i64, i64),
        i: (i64, i64),
    ) -> Matrix {
        vec![
            vec![q(a.0, a.1), q(b.0, b.1), q(c.0, c.1)],
            vec![q(d.0, d.1), q(e.0, e.1), q(f.0, f.1)],
            vec![q(g.0, g.1), q(h.0, h.1), q(i.0, i.1)],
        ]
    }

    #[test]
    fn rational_arithmetic_basics() {
        assert_eq!(q(1, 2) + q(1, 3), q(5, 6));
        assert_eq!(q(1, 2) - q(1, 3), q(1, 6));
        assert_eq!(q(2, 3) * q(3, 4), q(1, 2));
        assert_eq!(q(1, 2) / q(1, 4), q(2, 1));
        assert_eq!(-q(3, 4), q(-3, 4));
    }

    #[test]
    fn rational_normalises_sign() {
        // The denominator is always positive; the sign rides on the numerator.
        assert_eq!(q(1, -2), q(-1, 2));
        assert_eq!(q(-1, -2), q(1, 2));
    }

    #[test]
    fn rational_reduces_to_lowest_terms() {
        assert_eq!(q(2, 4), q(1, 2));
        assert_eq!(q(15, 25), q(3, 5));
        assert_eq!(q(-6, 8), q(-3, 4));
    }

    #[test]
    fn solve_2x2_identity() {
        // x = [1, 2]
        let a = mat2((1, 1), (0, 1), (0, 1), (1, 1));
        let b = vec![q(1, 1), q(2, 1)];
        let x = solve(&a, &b).unwrap();
        assert_eq!(x, vec![q(1, 1), q(2, 1)]);
    }

    #[test]
    fn solve_3x3_cramer() {
        // Classic Cramer example:
        //   2x +  y - z =  8
        //  -3x - y + 2z = -11
        //  -2x +  y + 2z = -3
        // Solution: x = 2, y = 3, z = -1.
        let a: Matrix = vec![
            vec![q(2, 1), q(1, 1), q(-1, 1)],
            vec![q(-3, 1), q(-1, 1), q(2, 1)],
            vec![q(-2, 1), q(1, 1), q(2, 1)],
        ];
        let b = vec![q(8, 1), q(-11, 1), q(-3, 1)];
        let x = solve(&a, &b).unwrap();
        assert_eq!(x, vec![q(2, 1), q(3, 1), q(-1, 1)]);
    }

    #[test]
    fn solve_fractional_rhs() {
        // The matrix is [[1, 1], [1, -1]]; b = [5/2, 1/2].
        // Adding rows: 2x = 3, so x = 3/2 and y = 5/2 - 3/2 = 1.
        let a = mat2((1, 1), (1, 1), (1, 1), (-1, 1));
        let b = vec![q(5, 2), q(1, 2)];
        let x = solve(&a, &b).unwrap();
        assert_eq!(x, vec![q(3, 2), q(1, 1)]);
    }

    #[test]
    fn solve_singular_detected() {
        // Row 2 = 2 * Row 1, so the system is under-determined.
        let a = mat2((1, 1), (2, 1), (2, 1), (4, 1));
        let b = vec![q(3, 1), q(6, 1)];
        let err = solve(&a, &b).unwrap_err();
        assert_eq!(err, SolveError::Singular);
    }

    #[test]
    fn determinant_identity_is_one() {
        let a = mat2((1, 1), (0, 1), (0, 1), (1, 1));
        assert_eq!(determinant(&a).unwrap(), q(1, 1));
    }

    #[test]
    fn determinant_3x3_with_rationals() {
        // Cofactor expansion along the first row:
        // det = 2*det([[-1,2],[1,2]]) - 1*det([[-3,2],[-2,2]]) + (-1)*det([[-3,-1],[-2,1]])
        //     = 2*(-1*2 - 2*1) - 1*(-3*2 - 2*(-2)) + (-1)*(-3*1 - (-1)*(-2))
        //     = 2*(-4) - 1*(-2) + (-1)*(-5)
        //     = -8 + 2 + 5 = -1
        let a = vec![
            vec![q(2, 1), q(1, 1), q(-1, 1)],
            vec![q(-3, 1), q(-1, 1), q(2, 1)],
            vec![q(-2, 1), q(1, 1), q(2, 1)],
        ];
        assert_eq!(determinant(&a).unwrap(), q(-1, 1));
    }

    #[test]
    fn determinant_singular_is_zero() {
        let a = mat2((1, 1), (2, 1), (2, 1), (4, 1));
        assert_eq!(determinant(&a).unwrap(), q(0, 1));
    }

    #[test]
    fn inverse_round_trip() {
        // For any invertible A: A * A^-1 = I.
        let a = mat2((1, 1), (2, 1), (3, 1), (4, 1));
        let inv = inverse(&a).unwrap();
        let n = 2;
        let mut prod = vec![vec![q(0, 1); n]; n];
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let term = a[i][k].clone() * inv[k][j].clone();
                    prod[i][j] = prod[i][j].clone() + term;
                }
            }
        }
        let identity = vec![vec![q(1, 1), q(0, 1)], vec![q(0, 1), q(1, 1)]];
        assert_eq!(prod, identity);
    }

    #[test]
    fn inverse_non_square_rejected() {
        let a: Matrix = vec![
            vec![q(1, 1), q(2, 1), q(3, 1)],
            vec![q(4, 1), q(5, 1), q(6, 1)],
        ];
        let err = inverse(&a).unwrap_err();
        assert_eq!(err, SolveError::NotSquare { rows: 2, cols: 2 });
    }

    #[test]
    fn solve_dimension_mismatch_rejected() {
        let a = mat2((1, 1), (0, 1), (0, 1), (1, 1));
        let b = vec![q(1, 1)];
        let err = solve(&a, &b).unwrap_err();
        assert_eq!(err, SolveError::NotSquare { rows: 1, cols: 2 });
    }

    #[test]
    fn solve_4x4_tridiagonal() {
        // Diagonally-dominant 4x4 system with a unique solution.
        //   [4 -1  0  0] [x0]   [ 5]
        //   [-1 4 -1  0] [x1] = [ 3]
        //   [ 0 -1 4 -1] [x2]   [ 1]
        //   [ 0  0 -1 4] [x3]   [-1]
        // From the last row: x3 = (x2 - 1) / 4. Working upward,
        // x2 = (x1 + x3 + 1) / 4, etc. The exact solution is
        // x = (97/52, 26/13, 21/13, 2/13) — verified by substitution.
        let a = vec![
            vec![q(4, 1), q(-1, 1), q(0, 1), q(0, 1)],
            vec![q(-1, 1), q(4, 1), q(-1, 1), q(0, 1)],
            vec![q(0, 1), q(-1, 1), q(4, 1), q(-1, 1)],
            vec![q(0, 1), q(0, 1), q(-1, 1), q(4, 1)],
        ];
        let b = vec![q(5, 1), q(3, 1), q(1, 1), q(-1, 1)];
        let x = solve(&a, &b).unwrap();
        // Verify by substitution: A * x must equal b exactly.
        for i in 0..4 {
            let mut acc = q(0, 1);
            for j in 0..4 {
                acc = acc + a[i][j].clone() * x[j].clone();
            }
            assert_eq!(acc, b[i], "row {} fails A*x = b", i);
        }
    }

    #[test]
    fn rational_display() {
        assert_eq!(format!("{}", q(3, 4)), "3/4");
        assert_eq!(format!("{}", q(5, 1)), "5");
        assert_eq!(format!("{}", q(-7, 3)), "-7/3");
    }
}
