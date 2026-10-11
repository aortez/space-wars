use engine_core::Vec2;

/// Row-major matrix; kept private until another engine needs this arithmetic.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Mat(pub [f32; 4]);

impl Mat {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0]);

    pub fn add(self, rhs: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + rhs.0[i]))
    }

    pub fn scale(self, scale: f32) -> Self {
        Self(self.0.map(|v| v * scale))
    }

    pub fn vector(self, v: Vec2) -> Vec2 {
        Vec2::new(
            self.0[0] * v.x + self.0[1] * v.y,
            self.0[2] * v.x + self.0[3] * v.y,
        )
    }

    pub fn mul(self, rhs: Self) -> Self {
        let x = self.vector(Vec2::new(rhs.0[0], rhs.0[2]));
        let y = self.vector(Vec2::new(rhs.0[1], rhs.0[3]));
        Self([x.x, y.x, x.y, y.y])
    }

    pub fn transpose(self) -> Self {
        Self([self.0[0], self.0[2], self.0[1], self.0[3]])
    }

    pub fn outer(a: Vec2, b: Vec2) -> Self {
        Self([a.x * b.x, a.x * b.y, a.y * b.x, a.y * b.y])
    }

    pub fn determinant(self) -> f32 {
        self.0[0] * self.0[3] - self.0[1] * self.0[2]
    }

    pub fn norm(self) -> f32 {
        self.0.iter().map(|v| v * v).sum::<f32>().sqrt()
    }

    pub fn finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }

    /// Orientation-preserving 2D SVD. f64 intermediates avoid cancellation for
    /// almost isotropic strain; the small singular value comes from det(F).
    pub fn svd(self) -> Option<(Self, [f32; 2], Self)> {
        let [a, b, c, d] = self.0.map(f64::from);
        let determinant = a * d - b * c;
        if !determinant.is_finite() || determinant <= 1.0e-12 {
            return None;
        }
        let theta = 0.5 * (2.0 * (a * b + c * d)).atan2(a * a + c * c - b * b - d * d);
        let (sin, cos) = theta.sin_cos();
        let x = a * cos + b * sin;
        let y = c * cos + d * sin;
        let s0 = x.hypot(y);
        let s1 = determinant / s0;
        if s1 <= 1.0e-6 || s0 >= 1.0e6 {
            return None;
        }
        let u = Self([
            (x / s0) as f32,
            (-y / s0) as f32,
            (y / s0) as f32,
            (x / s0) as f32,
        ]);
        let v = Self([cos as f32, -sin as f32, sin as f32, cos as f32]);
        Some((u, [s0 as f32, s1 as f32], v))
    }

    pub fn diagonal(v: [f32; 2]) -> Self {
        Self([v[0], 0.0, 0.0, v[1]])
    }
}
