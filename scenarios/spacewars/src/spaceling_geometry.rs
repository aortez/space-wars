//! Spacewars pilot dimensions, shared by physics, drawing and bot measurements.
//! The standalone engine labs keep their own, larger spaceling specification.

/// Scale of the original suit drawing and capsule.
pub const SCALE: f32 = 0.5;
pub const RADIUS: f32 = 0.3 * SCALE;
pub const HALF_SEGMENT: f32 = 0.6 * SCALE;
/// Distance from the upright actor's center to the soles of its feet.
pub const HALF_HEIGHT: f32 = HALF_SEGMENT + RADIUS;
