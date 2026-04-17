//! Spacewars engine: 2D entities, physics, fluids (later).
//!
//! See `docs/design/reboot-rust-slint.md` for context.

pub mod bounds;
pub mod color;
pub mod config;
pub mod constants;
pub mod math;
pub mod physics;
pub mod rng;
pub mod transform;

pub use bounds::{Bounds2, Circle, Line};
pub use color::Color;
pub use config::{PlayerConfig, SpacewarsConfig};
pub use math::Vec2;
pub use transform::Transform2;

// pub mod bg_star_field;
// pub mod cannon;
// pub mod debris;
// pub mod escape_pod;
// pub mod laser;
// pub mod particle;
// pub mod physics;
// pub mod planet;
// pub mod ship;
