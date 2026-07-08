//! Mandelbrot set: point-in-set membership with configurable iteration cap.
//!
//! A complex point `c` is "in" the Mandelbrot set if iterating
//! `z_{n+1} = z_n^2 + c` starting at `z_0 = 0` stays bounded. In practice we
//! bound iterations and check whether `|z|` exceeded 2 (the escape radius).

/// Result of a Mandelbrot test: `iterations` is the escape count.
/// `iterations == max_iter` is interpreted as "appears to stay bounded".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EscapeCount {
    pub iterations: u32,
}

/// Test a single point. Returns how many iterations escaped.
///
/// `c_re` and `c_im` are the real and imaginary parts of `c`.
pub fn escape(c_re: f64, c_im: f64, max_iter: u32) -> EscapeCount {
    let mut zr = 0.0_f64;
    let mut zi = 0.0_f64;
    for i in 0..max_iter {
        let r2 = zr * zr + zi * zi;
        if r2 > 4.0 {
            return EscapeCount { iterations: i };
        }
        let new_zr = zr * zr - zi * zi + c_re;
        let new_zi = 2.0 * zr * zi + c_im;
        zr = new_zr;
        zi = new_zi;
    }
    EscapeCount { iterations: max_iter }
}

/// `true` if the point is "in" the set (within the iteration cap).
pub fn in_set(c_re: f64, c_im: f64, max_iter: u32) -> bool {
    escape(c_re, c_im, max_iter).iterations == max_iter
}

/// Render a `(width, height)` ASCII grid into a `String` of '.' (escaped)
/// and '#' (in-set) characters.
pub fn ascii_grid(x_min: f64, x_max: f64, y_min: f64, y_max: f64,
                  width: usize, height: usize, max_iter: u32) -> String {
    let mut out = String::with_capacity((width + 1) * height);
    for row in 0..height {
        for col in 0..width {
            let cx = x_min + (x_max - x_min) * (col as f64 / width as f64);
            let cy = y_min + (y_max - y_min) * (row as f64 / height as f64);
            let c = escape(cx, cy, max_iter);
            let ch = if c.iterations == max_iter { '#' } else { '.' };
            out.push(ch);
        }
        if row + 1 < height {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_in_set() {
        // c = 0 + 0i: orbit is z_0 = 0, z_1 = 0, ...
        assert!(in_set(0.0, 0.0, 1000));
    }

    #[test]
    fn point_at_1_escapes() {
        // c = 1: z = 0,1,2,5,26 -> explodes immediately.
        let e = escape(1.0, 0.0, 100);
        assert!(e.iterations < 5);
        assert!(!in_set(1.0, 0.0, 100));
    }

    #[test]
    fn point_at_minus_1_in_set() {
        // c = -1: orbit 0,-1,0,-1... bounded.
        assert!(in_set(-1.0, 0.0, 1000));
    }

    #[test]
    fn point_at_1_i_escapes() {
        // c = 1+i: outside the Mandelbrot set, escapes quickly.
        let e = escape(1.0, 1.0, 1000);
        assert!(e.iterations < 10);
        assert!(!in_set(1.0, 1.0, 1000));
    }

    #[test]
    fn point_at_minus_0p75_in_set() {
        // c = -0.75 is in the main cardioid.
        assert!(in_set(-0.75, 0.0, 1000));
    }

    #[test]
    fn ascii_grid_dimensions() {
        let grid = ascii_grid(-2.0, 0.5, -1.25, 1.25, 10, 5, 100);
        let lines: Vec<&str> = grid.split('\n').collect();
        assert_eq!(lines.len(), 5);
        for line in &lines {
            assert_eq!(line.chars().count(), 10);
        }
    }

    #[test]
    fn ascii_grid_contains_hashes_and_dots() {
        let grid = ascii_grid(-2.0, 0.5, -1.25, 1.25, 40, 20, 200);
        assert!(grid.contains('#'));
        assert!(grid.contains('.'));
    }
}
