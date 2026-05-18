//! 2D math primitives for the Spacewars simulation.
//!
//! The original Java `Vec3` stores a `z` value for rendering, but gameplay math
//! is two-dimensional. Its `angleRadians()` returns `-atan2(y, x)`; this module
//! preserves that sign convention in [`Vec2::angle_radians`].

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);
    pub const X: Self = Self::new(1.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v }
    }

    pub fn from_radians(radians: f32) -> Self {
        Self::X.rotate_radians(radians).normalized()
    }

    pub fn from_degrees(degrees: f32) -> Self {
        Self::from_radians(degrees.to_radians())
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn distance_to(self, other: Self) -> f32 {
        (self - other).length()
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len == 0.0 { self } else { self / len }
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn component_mul(self, other: Self) -> Self {
        Self::new(self.x * other.x, self.y * other.y)
    }

    pub fn component_div(self, other: Self) -> Self {
        Self::new(self.x / other.x, self.y / other.y)
    }

    pub fn midpoint(self, other: Self) -> Self {
        (self + other) * 0.5
    }

    /// Return the original Java angle convention: `-atan2(y, x)`.
    pub fn angle_radians(self) -> f32 {
        -self.y.atan2(self.x)
    }

    pub fn angle_degrees(self) -> f32 {
        self.angle_radians().to_degrees()
    }

    pub fn rotate_radians(self, radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
    }

    pub fn rotate_degrees(self, degrees: f32) -> Self {
        self.rotate_radians(degrees.to_radians())
    }

    pub fn closest_point_on_segment(point: Self, start: Self, end: Self) -> Self {
        let segment = end - start;
        let len_sq = segment.length_squared();
        if len_sq == 0.0 {
            return start;
        }

        let t = ((point - start).dot(segment) / len_sq).clamp(0.0, 1.0);
        start + segment * t
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

impl DivAssign<f32> for Vec2 {
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1.0e-5;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "actual {actual} expected {expected}"
        );
    }

    fn assert_vec_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    #[test]
    fn length_distance_and_normalization_match_java_vec3_2d_behavior() {
        let v = Vec2::new(3.0, 4.0);
        assert_close(v.length(), 5.0);
        assert_close(v.distance_to(Vec2::ZERO), 5.0);
        assert_vec_close(v.normalized(), Vec2::new(0.6, 0.8));
        assert_eq!(Vec2::ZERO.normalized(), Vec2::ZERO);
    }

    #[test]
    fn angle_uses_original_negative_atan2_convention() {
        assert_close(Vec2::X.angle_radians(), 0.0);
        assert_close(Vec2::Y.angle_radians(), -core::f32::consts::FRAC_PI_2);
        assert_close(Vec2::new(0.0, -1.0).angle_degrees(), 90.0);
    }

    #[test]
    fn radians_rotation_matches_original_direction_factory() {
        assert_vec_close(Vec2::from_radians(0.0), Vec2::X);
        assert_vec_close(Vec2::from_radians(core::f32::consts::FRAC_PI_2), Vec2::Y);
        assert_vec_close(Vec2::X.rotate_degrees(180.0), Vec2::new(-1.0, 0.0));
    }

    #[test]
    fn closest_point_on_segment_clamps_to_segment() {
        let start = Vec2::new(0.0, 0.0);
        let end = Vec2::new(10.0, 0.0);
        assert_eq!(
            Vec2::closest_point_on_segment(Vec2::new(5.0, 5.0), start, end),
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(
            Vec2::closest_point_on_segment(Vec2::new(-5.0, 5.0), start, end),
            start
        );
        assert_eq!(
            Vec2::closest_point_on_segment(Vec2::new(15.0, 5.0), start, end),
            end
        );
    }

    #[test]
    fn component_ops_are_pairwise() {
        assert_eq!(
            Vec2::new(2.0, 6.0).component_mul(Vec2::new(4.0, 3.0)),
            Vec2::new(8.0, 18.0)
        );
        assert_eq!(
            Vec2::new(8.0, 18.0).component_div(Vec2::new(4.0, 3.0)),
            Vec2::new(2.0, 6.0)
        );
    }
}
