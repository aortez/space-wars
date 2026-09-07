//! Explicit, fixture-only world profiles. No actor-dependent gravity or transport.

use super::*;

/// Versioned alternatives for the exact same generated case and controllers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedSurfaceProfile {
    #[default]
    Raw,
    SurfaceV1,
}

pub(super) const SURFACE_GRAVITY: f32 = 18.0;
pub(super) const SUN_SURFACE_GRAVITY: f32 = 3.0;
pub(super) const SPIN_GRAVITY_FRACTION: f32 = 0.02;
pub(super) const MAX_SPIN: f32 = 0.04;
/// Enough exterior room for the fixed approach/takeoff probes and normal flight.
pub(super) const FLIGHT_MARGIN: u32 = 200;

impl GeneratedSurfaceProfile {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::SurfaceV1 => "surface-v1",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Raw => "GENERATED / UNTUNED",
            Self::SurfaceV1 => "GENERATED / SURFACE V1",
        }
    }

    pub(super) fn preset(self) -> SurfaceMotionPreset {
        match self {
            Self::Raw => SurfaceMotionPreset::Generated,
            Self::SurfaceV1 => SurfaceMotionPreset::GeneratedSurfaceV1,
        }
    }

    pub(super) fn apply(self, world: &mut SpacewarsState) {
        if self == Self::Raw {
            return;
        }
        // The legacy generator only keeps planet geometry inside the wall;
        // it can leave less than one ship-width outside the outermost planet.
        // Expand the boundary, translating the whole layout to its new center.
        // Orbital radii, relative positions, and all object sizes are preserved.
        world.config.universe_radius += FLIGHT_MARGIN;
        let offset = Vec2::splat(FLIGHT_MARGIN as f32);
        let sun = world.sun.as_mut().expect("generated world has its sun");
        sun.position += offset;
        sun.mass = SUN_SURFACE_GRAVITY * sun.radius.powi(2) / (60.0 * GRAVITY);
        for planet in &mut world.planets {
            planet.position += offset;
            let standing_radius =
                planet.radius * BODY_BOUNDS_RADIUS_SCALE + SurfaceSortieState::spec().half_height();
            planet.mass = SURFACE_GRAVITY * standing_radius.powi(2) / (60.0 * GRAVITY);
            // Retain generated directions and relative spin variety, not the
            // legacy force-incompatible rates. The cap also bounds centrifugal
            // acceleration at a standing capsule to 2% of local planet gravity.
            let spin_limit =
                MAX_SPIN.min((SURFACE_GRAVITY * SPIN_GRAVITY_FRACTION / standing_radius).sqrt());
            planet.wrapper_omega = planet.wrapper_omega / std::f32::consts::FRAC_PI_6 * spin_limit;
            // Match the prescribed path to the central field. Other planets'
            // real fields remain present; this is not a full N-body trajectory.
            let direction = if planet.orbit_omega < 0.0 { -1.0 } else { 1.0 };
            planet.orbit_omega =
                direction * (60.0 * GRAVITY * sun.mass / planet.orbit_radius.powi(3)).sqrt();
        }
    }
}
