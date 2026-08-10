//! Quaternions — unit quaternions for 3D rotations.
//!
//! A quaternion `q = w + xi + yj + zk` with `w^2 + x^2 + y^2 + z^2 = 1`
//! encodes a rotation in 3D. They are free of gimbal lock and can be
//! composed by multiplication.
//!
//! Conventions used here (Hamilton convention, right-handed):
//!
//! - `i*j = k`, `j*k = i`, `k*i = j`
//! - Multiplying two unit quaternions composes the rotations: apply
//!   `q2`'s rotation first, then `q1`'s.
//! - Rotation of a 3D vector `v` by `q`: `v' = q * v * q.conjugate()`,
//!   where `v` is treated as the pure quaternion `0 + vi + vj + vk`.
//!
//! Reference: W. R. Hamilton, "On Quaternions", Proceedings of the
//! Royal Irish Academy, 1844. 3D-graphics conventions following
//! Shoemake, "Animating Rotation with Quaternion Curves", SIGGRAPH
//! 1985.

use std::f64::consts::PI;

/// A unit quaternion. `w` is the scalar (real) part; `x`, `y`, `z` are
/// the vector (imaginary) part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// A 3D vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const X: Vec3 = Vec3 {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const Y: Vec3 = Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const Z: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3 {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }

    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalize(self) -> Vec3 {
        let len = self.length();
        if len == 0.0 {
            Vec3::ZERO
        } else {
            Vec3::new(self.x / len, self.y / len, self.z / len)
        }
    }
}

impl Quat {
    /// Identity rotation.
    pub const IDENTITY: Quat = Quat {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(w: f64, x: f64, y: f64, z: f64) -> Self {
        Quat { w, x, y, z }
    }

    /// Quaternion conjugate. For a unit quaternion this is the inverse
    /// rotation.
    pub fn conjugate(self) -> Quat {
        Quat {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// Squared norm.
    pub fn norm_sq(self) -> f64 {
        self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Euclidean norm.
    pub fn norm(self) -> f64 {
        self.norm_sq().sqrt()
    }

    /// Normalize to a unit quaternion. If the input is the zero
    /// quaternion, returns `Quat::IDENTITY`.
    pub fn normalize(self) -> Quat {
        let n = self.norm();
        if n == 0.0 {
            Quat::IDENTITY
        } else {
            Quat::new(self.w / n, self.x / n, self.y / n, self.z / n)
        }
    }

    /// Quaternion multiplication. Composes `self` then `other` (i.e.
    /// when applied to a vector, `other`'s rotation happens first then
    /// `self`'s).
    pub fn mul(self, other: Quat) -> Quat {
        Quat {
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
        }
    }

    /// Rotate `v` by this quaternion. Equivalent to
    /// `q * v_quat * q.conjugate()` projected back to 3D.
    pub fn rotate(self, v: Vec3) -> Vec3 {
        // Optimized form (shoemake): v + 2 * cross(q.xyz, cross(q.xyz, v) + q.w * v)
        let qv = Vec3::new(self.x, self.y, self.z);
        let t = qv.cross(v).add_scaled(v, self.w).scale(2.0);
        v.add_scaled(qv.cross(t), 1.0)
    }
}

impl Vec3 {
    fn add_scaled(self, o: Vec3, s: f64) -> Vec3 {
        Vec3::new(self.x + o.x * s, self.y + o.y * s, self.z + o.z * s)
    }
    fn scale(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

/// Build a unit quaternion that rotates by `angle` radians about the
/// given `axis`. `axis` need not be pre-normalized; it is normalized
/// here. A zero-length axis produces `Quat::IDENTITY`.
pub fn from_axis_angle(axis: Vec3, angle: f64) -> Quat {
    let n = axis.length();
    if n == 0.0 {
        return Quat::IDENTITY;
    }
    let (ax, ay, az) = (axis.x / n, axis.y / n, axis.z / n);
    let half = angle * 0.5;
    let s = half.sin();
    Quat::new(half.cos(), ax * s, ay * s, az * s)
}

/// Spherical linear interpolation between two unit quaternions. `t` is
/// clamped to `[0, 1]`. Uses the shortest arc (negates `b` if the dot
/// product is negative).
pub fn slerp(a: Quat, b: Quat, t: f64) -> Quat {
    let t = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    let mut dot = a.w * b.w + a.x * b.x + a.y * b.y + a.z * b.z;
    let b = if dot < 0.0 {
        dot = -dot;
        Quat::new(-b.w, -b.x, -b.y, -b.z)
    } else {
        b
    };
    // If the inputs are nearly identical, fall back to linear interp +
    // normalize to avoid 0/0 from sin(theta).
    if dot > 0.9995 {
        let q = Quat::new(
            a.w + t * (b.w - a.w),
            a.x + t * (b.x - a.x),
            a.y + t * (b.y - a.y),
            a.z + t * (b.z - a.z),
        );
        return q.normalize();
    }
    let theta_0 = dot.clamp(-1.0, 1.0).acos();
    let theta = theta_0 * t;
    let sin_theta_0 = theta_0.sin();
    let s1 = theta.sin() / sin_theta_0;
    let s0 = (theta_0 - theta).sin() / sin_theta_0;
    Quat::new(
        s0 * a.w + s1 * b.w,
        s0 * a.x + s1 * b.x,
        s0 * a.y + s1 * b.y,
        s0 * a.z + s1 * b.z,
    )
}

/// Convert a unit quaternion to a 3x3 rotation matrix in row-major
/// order. Returns 9 elements.
pub fn to_matrix(q: Quat) -> [f64; 9] {
    let (w, x, y, z) = (q.w, q.x, q.y, q.z);
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    let xy = x * y;
    let xz = x * z;
    let yz = y * z;
    let wx = w * x;
    let wy = w * y;
    let wz = w * z;
    [
        1.0 - 2.0 * (yy + zz),
        2.0 * (xy - wz),
        2.0 * (xz + wy),
        2.0 * (xy + wz),
        1.0 - 2.0 * (xx + zz),
        2.0 * (yz - wx),
        2.0 * (xz - wy),
        2.0 * (yz + wx),
        1.0 - 2.0 * (xx + yy),
    ]
}

const EPS: f64 = 1e-9;

fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rotate_unchanged() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        let r = Quat::IDENTITY.rotate(v);
        assert!(approx_eq(r.x, v.x, 1e-12));
        assert!(approx_eq(r.y, v.y, 1e-12));
        assert!(approx_eq(r.z, v.z, 1e-12));
    }

    #[test]
    fn rotate_about_z_axis_90deg() {
        // +X should map to +Y for a +90deg rotation about +Z.
        let q = from_axis_angle(Vec3::Z, PI / 2.0);
        let r = q.rotate(Vec3::X);
        assert!(approx_eq(r.x, 0.0, 1e-9), "x={}", r.x);
        assert!(approx_eq(r.y, 1.0, 1e-9), "y={}", r.y);
        assert!(approx_eq(r.z, 0.0, 1e-9), "z={}", r.z);
    }

    #[test]
    fn rotate_about_y_axis_180deg() {
        // +X should map to -X for a +180deg rotation about +Y.
        let q = from_axis_angle(Vec3::Y, PI);
        let r = q.rotate(Vec3::X);
        assert!(approx_eq(r.x, -1.0, 1e-9));
        assert!(approx_eq(r.y, 0.0, 1e-9));
        assert!(approx_eq(r.z, 0.0, 1e-9));
    }

    #[test]
    fn rotation_axis_is_invariant() {
        // Rotating about an axis should leave points on that axis fixed.
        let axis = Vec3::new(1.0, 2.0, 3.0).normalize();
        let q = from_axis_angle(axis, 1.2345);
        let r = q.rotate(axis);
        assert!(approx_eq(r.x, axis.x, 1e-9));
        assert!(approx_eq(r.y, axis.y, 1e-9));
        assert!(approx_eq(r.z, axis.z, 1e-9));
    }

    #[test]
    fn compose_two_rotations_about_x_and_y() {
        // Rotate 90deg about X then 90deg about Y. Then rotate Z unit
        // through both. We can also compose by q_y * q_x.
        let qx = from_axis_angle(Vec3::X, PI / 2.0);
        let qy = from_axis_angle(Vec3::Y, PI / 2.0);
        let v = Vec3::Z;
        let r1 = qy.rotate(qx.rotate(v));
        let r2 = (qy.mul(qx)).rotate(v);
        assert!(approx_eq(r1.x, r2.x, 1e-9));
        assert!(approx_eq(r1.y, r2.y, 1e-9));
        assert!(approx_eq(r1.z, r2.z, 1e-9));
    }

    #[test]
    fn conjugate_is_inverse_for_unit() {
        let q = from_axis_angle(Vec3::new(0.3, 0.6, 0.7), 1.0);
        let inv = q.conjugate();
        // q * inv == identity
        let prod = q.mul(inv);
        assert!(approx_eq(prod.w, 1.0, 1e-12));
        assert!(approx_eq(prod.x, 0.0, 1e-12));
        assert!(approx_eq(prod.y, 0.0, 1e-12));
        assert!(approx_eq(prod.z, 0.0, 1e-12));
    }

    #[test]
    fn slerp_endpoints() {
        let a = from_axis_angle(Vec3::Z, 0.0);
        let b = from_axis_angle(Vec3::Z, PI / 2.0);
        let q0 = slerp(a, b, 0.0);
        let q1 = slerp(a, b, 1.0);
        let v0 = q0.rotate(Vec3::X);
        let v1 = q1.rotate(Vec3::X);
        // At t=0 should equal X; at t=1 should equal Y.
        assert!(approx_eq(v0.x, 1.0, 1e-9));
        assert!(approx_eq(v0.y, 0.0, 1e-9));
        assert!(approx_eq(v1.x, 0.0, 1e-9));
        assert!(approx_eq(v1.y, 1.0, 1e-9));
    }

    #[test]
    fn slerp_midpoint_is_quarter_turn() {
        let a = from_axis_angle(Vec3::Z, 0.0);
        let b = from_axis_angle(Vec3::Z, PI / 2.0);
        let q = slerp(a, b, 0.5);
        // Should rotate X by 45deg -> (cos45, sin45, 0) ~= (0.7071, 0.7071, 0).
        let v = q.rotate(Vec3::X);
        let s = (PI / 4.0).sin();
        let c = (PI / 4.0).cos();
        assert!(approx_eq(v.x, c, 1e-9));
        assert!(approx_eq(v.y, s, 1e-9));
        assert!(approx_eq(v.z, 0.0, 1e-9));
    }

    #[test]
    fn matrix_orthonormal_and_rotate_matches() {
        let q = from_axis_angle(Vec3::new(0.3, 0.6, 0.7).normalize(), 1.2345);
        let m = to_matrix(q);
        let v = Vec3::new(1.5, -2.0, 0.75);
        let via_q = q.rotate(v);
        // Matrix is row-major: result = M * v.
        let via_m = Vec3::new(
            m[0] * v.x + m[1] * v.y + m[2] * v.z,
            m[3] * v.x + m[4] * v.y + m[5] * v.z,
            m[6] * v.x + m[7] * v.y + m[8] * v.z,
        );
        assert!(approx_eq(via_q.x, via_m.x, 1e-9));
        assert!(approx_eq(via_q.y, via_m.y, 1e-9));
        assert!(approx_eq(via_q.z, via_m.z, 1e-9));

        // Orthonormal: columns are unit length and mutually orthogonal.
        let c0 = Vec3::new(m[0], m[3], m[6]);
        let c1 = Vec3::new(m[1], m[4], m[7]);
        let c2 = Vec3::new(m[2], m[5], m[8]);
        assert!(approx_eq(c0.length(), 1.0, 1e-9));
        assert!(approx_eq(c1.length(), 1.0, 1e-9));
        assert!(approx_eq(c2.length(), 1.0, 1e-9));
        assert!(approx_eq(c0.dot(c1), 0.0, 1e-9));
        assert!(approx_eq(c1.dot(c2), 0.0, 1e-9));
        assert!(approx_eq(c2.dot(c0), 0.0, 1e-9));
    }

    #[test]
    fn normalize_unit_quat_is_idempotent() {
        let q = from_axis_angle(Vec3::X, 0.7);
        let n = q.normalize();
        assert!(approx_eq(n.norm(), 1.0, 1e-12));
    }

    #[test]
    fn zero_axis_returns_identity() {
        let q = from_axis_angle(Vec3::ZERO, PI);
        assert_eq!(q, Quat::IDENTITY);
    }

    #[test]
    fn vec_ops_basics() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a.dot(b), 1.0 * 4.0 + 2.0 * 5.0 + 3.0 * 6.0);
        let c = a.cross(b);
        assert!(approx_eq(c.x, -3.0, 1e-12));
        assert!(approx_eq(c.y, 6.0, 1e-12));
        assert!(approx_eq(c.z, -3.0, 1e-12));
        assert!(approx_eq(a.normalize().length(), 1.0, 1e-12));
    }
}
