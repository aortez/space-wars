//! Simple 2D transform for simulation entities and render primitive emission.

use serde::{Deserialize, Serialize};

use crate::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2 {
    pub translation: Vec2,
    pub scale: Vec2,
    pub rotation_radians: f32,
    pub pivot: Vec2,
}

impl Transform2 {
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        scale: Vec2::new(1.0, 1.0),
        rotation_radians: 0.0,
        pivot: Vec2::ZERO,
    };

    pub const fn from_translation(translation: Vec2) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub fn transform_point(self, point: Vec2) -> Vec2 {
        let local = (point - self.pivot).component_mul(self.scale);
        local.rotate_radians(self.rotation_radians) + self.pivot + self.translation
    }

    pub fn translate(&mut self, delta: Vec2) {
        self.translation += delta;
    }

    pub fn rotate(&mut self, delta_radians: f32) {
        self.rotation_radians += delta_radians;
    }
}

impl Default for Transform2 {
    fn default() -> Self {
        Self::IDENTITY
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
    fn identity_leaves_points_unchanged() {
        let point = Vec2::new(3.0, 4.0);
        assert_eq!(Transform2::default().transform_point(point), point);
    }

    #[test]
    fn transform_applies_scale_rotation_pivot_and_translation() {
        let transform = Transform2 {
            translation: Vec2::new(10.0, 0.0),
            scale: Vec2::new(2.0, 1.0),
            rotation_radians: core::f32::consts::FRAC_PI_2,
            pivot: Vec2::new(1.0, 1.0),
        };

        assert_vec_close(
            transform.transform_point(Vec2::new(2.0, 1.0)),
            Vec2::new(11.0, 3.0),
        );
    }

    #[test]
    fn translate_and_rotate_mutate_incrementally() {
        let mut transform = Transform2::from_translation(Vec2::new(1.0, 2.0));
        transform.translate(Vec2::new(3.0, 4.0));
        transform.rotate(0.25);

        assert_eq!(transform.translation, Vec2::new(4.0, 6.0));
        assert_close(transform.rotation_radians, 0.25);
    }
}
