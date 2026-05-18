//! Normalized RGBA color values.

use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const CLEAR: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const YELLOW: Self = Self::rgb(1.0, 1.0, 0.0);
    pub const DIM_GREY: Self = Self::scale_255(100.0, 100.0, 100.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn scale_255(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r / 255.0, g / 255.0, b / 255.0, 1.0)
    }

    pub const fn scale_255_alpha(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::rgba(r / 255.0, g / 255.0, b / 255.0, a / 255.0)
    }

    pub fn with_intensity(self, intensity: f32) -> Self {
        Self::rgba(
            self.r * intensity,
            self.g * intensity,
            self.b * intensity,
            self.a * intensity,
        )
    }

    pub fn random_variation(self, variance: f32, rng: &mut impl Rng) -> Self {
        let half = variance * 0.5;
        Self::rgb(
            self.r + rng.random_range(-half..half),
            self.g + rng.random_range(-half..half),
            self.b + rng.random_range(-half..half),
        )
    }

    pub fn components_255(self) -> [f32; 4] {
        [
            self.r * 255.0,
            self.g * 255.0,
            self.b * 255.0,
            self.a * 255.0,
        ]
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    #[test]
    fn scale_255_matches_original_color_helper() {
        let c = Color::scale_255_alpha(255.0, 128.0, 0.0, 64.0);
        assert_eq!(c, Color::rgba(1.0, 128.0 / 255.0, 0.0, 64.0 / 255.0));
        assert_eq!(c.components_255(), [255.0, 128.0, 0.0, 64.0]);
    }

    #[test]
    fn intensity_scales_alpha_like_original_helper() {
        assert_eq!(
            Color::rgba(0.5, 0.25, 0.125, 0.75).with_intensity(2.0),
            Color::rgba(1.0, 0.5, 0.25, 1.5)
        );
    }

    #[test]
    fn random_variation_uses_supplied_rng() {
        let mut a = rand::rngs::StdRng::seed_from_u64(42);
        let mut b = rand::rngs::StdRng::seed_from_u64(42);

        assert_eq!(
            Color::RED.random_variation(0.1, &mut a),
            Color::RED.random_variation(0.1, &mut b)
        );
    }
}
