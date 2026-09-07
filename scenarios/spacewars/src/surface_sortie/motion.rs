//! Motion fixtures and completed-step surface frames. No actor transport.

use super::*;

/// Named, deterministic configurations of the same scenario and controllers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceMotionPreset {
    #[default]
    Stationary,
    /// A short headless diagnostic; a straight path eventually leaves the map.
    Translating,
    Orbit,
    /// Untuned ordinary generated world; intentionally a compatibility diagnostic.
    Generated,
    GeneratedSurfaceV1,
}

impl SurfaceMotionPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stationary => "STATIONARY CENTER",
            Self::Translating => "TRANSLATING",
            Self::Orbit => "ORBIT + SPIN",
            Self::Generated => "GENERATED / UNTUNED",
            Self::GeneratedSurfaceV1 => "GENERATED / SURFACE V1",
        }
    }

    pub(super) fn configure(self, planet: &mut PlanetState) -> Option<SunState> {
        if self != Self::Orbit {
            return None;
        }
        let center = planet.position;
        planet.orbit_radius = 220.0;
        planet.orbit_omega = 0.065;
        planet.position = center + Vec2::X * planet.orbit_radius;
        // For this evidence fixture, the scripted circular path's acceleration
        // agrees with the common sun gravity at the planet center. The planet
        // is still kinematic, not an N-body integration. Nearby actors receive
        // the unmodified field (including its spatial variation) exactly once.
        let mass = planet.orbit_omega.powi(2) * planet.orbit_radius.powi(3) / (60.0 * GRAVITY);
        Some(SunState {
            position: center,
            radius: 20.0,
            mass,
            color: Color::YELLOW,
        })
    }

    pub(super) fn initial_velocity(self, planet: &PlanetState) -> Vec2 {
        match self {
            Self::Translating => Vec2::new(3.0, 0.0),
            Self::Stationary | Self::Orbit | Self::Generated | Self::GeneratedSurfaceV1 => {
                planet_surface_velocity(planet, planet.position)
            }
        }
    }

    pub(super) fn advance(self, planet: &mut PlanetState, sun: Option<SunState>, dt: f32) {
        if matches!(
            self,
            Self::Orbit | Self::Generated | Self::GeneratedSurfaceV1
        ) {
            planet.update_orbit(
                sun.expect("orbital fixture has its central source")
                    .position,
                dt,
            );
        } else {
            planet.wrapper_angle += planet.wrapper_omega * dt;
            if self == Self::Translating {
                planet.position += self.initial_velocity(planet) * dt;
            }
        }
        planet.wrapper_angle = planet.wrapper_angle.rem_euclid(std::f32::consts::TAU);
    }
}

/// Use actual completed Rapier motion, not the analytic endpoint velocity or
/// next target pose. Contacts, boarding, and landing must share one time sample.
pub(super) fn point_velocity(surface: SurfaceFrame, point: Vec2) -> Vec2 {
    let offset = point - surface.position;
    surface.linear_velocity + Vec2::new(-offset.y, offset.x) * surface.angular_velocity
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SurfaceFrame {
    pub position: Vec2,
    pub angle: f32,
    /// Velocity of the frame origin, not of the rigid body's center of mass.
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
}

impl SurfaceFrame {
    pub(super) fn read(physics: &physics::SpacewarsPhysics, planet: usize) -> Self {
        let body = physics.planet_body(planet);
        let motion = physics
            .world
            .motion(body)
            .expect("sortie planet is present");
        Self {
            position: motion.position,
            angle: motion.angle,
            linear_velocity: physics
                .world
                .velocity_at_point(body, motion.position)
                .unwrap(),
            angular_velocity: motion.angular_velocity,
        }
    }
}

impl SurfaceSortieState {
    pub(super) fn planet_motion(&self) -> SurfaceFrame {
        SurfaceFrame::read(&self.world.physics, self.motion_planet_index())
    }

    pub fn motion_preset(&self) -> SurfaceMotionPreset {
        self.motion_preset
    }
}

/// Completed-step values. Relative velocity is measured at the active body's
/// origin, using the same surface frame as landing/boarding (not screen motion).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SurfaceMotionObservation {
    pub planet: usize,
    pub preset: SurfaceMotionPreset,
    pub planet_position: Vec2,
    pub planet_velocity: Vec2,
    pub planet_angle: f32,
    pub planet_spin: f32,
    pub scripted_acceleration: Vec2,
    pub external_gravity_at_center: Vec2,
    pub surface_velocity: Vec2,
    pub relative_velocity: Vec2,
    pub support_velocity: Option<Vec2>,
    pub support_relative_speed: Option<f32>,
}

/// Deterministic counters, updated only by simulation steps. No wall-clock
/// assertions or per-frame trace storage. Jumps/boarding are not support losses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SurfaceMotionMetrics {
    pub on_foot_ticks: u64,
    pub supported_ticks: u64,
    pub ship_landed_ticks: u64,
    pub pilot_support_losses: u64,
    pub ship_support_losses: u64,
    pub jumps: u64,
    pub knockdowns: u64,
    pub landings: u64,
    pub departures: u64,
    pub ship_damage: f32,
    /// Largest displacement in planet-local coordinates during a continuous
    /// balanced/supported idle interval. Movement, transfer, or flight resets
    /// that interval, but not this lifetime maximum.
    pub max_idle_drift: f32,
    pub idle_drift: f32,
}

pub(super) struct StepSample {
    planet: usize,
    snapshot: Option<SpacelingSnapshot>,
    landing: LandingTelemetry,
    ship_health: f32,
    ship_available: bool,
}

impl StepSample {
    pub(super) fn read(state: &SurfaceSortieState) -> Self {
        Self {
            planet: state.motion_planet_index(),
            snapshot: state.spaceling_snapshot(),
            landing: state.landing,
            ship_health: state.world.ships[state.pilot.vehicle.0].life,
            ship_available: state.vehicle_available(),
        }
    }
}

impl SurfaceSortieState {
    pub(super) fn motion_observation(&self) -> SurfaceMotionObservation {
        let frame = self.planet_motion();
        let snapshot = self.spaceling_snapshot();
        let body = self.pilot.body.as_ref().map_or_else(
            || self.world.physics.ship_body(self.pilot.vehicle.0),
            |pilot| pilot.body(),
        );
        let actor = self
            .world
            .physics
            .world
            .motion(body)
            .expect("active sortie body");
        let surface_velocity = point_velocity(frame, actor.position);
        let planet_index = self.motion_planet_index();
        let planet = &self.world.planets[planet_index];
        let (scripted_acceleration, mut external_gravity_at_center) =
            self.world.sun.map_or((Vec2::ZERO, Vec2::ZERO), |sun| {
                let inward = sun.position - frame.position;
                let gravity = if inward.length_squared() > 1.0e-6 {
                    inward.normalized() * (60.0 * GRAVITY * sun.mass / inward.length_squared())
                } else {
                    Vec2::ZERO
                };
                let scripted = if matches!(
                    self.motion_preset,
                    SurfaceMotionPreset::Orbit
                        | SurfaceMotionPreset::Generated
                        | SurfaceMotionPreset::GeneratedSurfaceV1
                ) {
                    inward * planet.orbit_omega.powi(2)
                } else {
                    Vec2::ZERO
                };
                (scripted, gravity)
            });
        for (index, planet) in self.world.planets.iter().enumerate() {
            if index != planet_index {
                let origin = SurfaceFrame::read(&self.world.physics, index).position;
                external_gravity_at_center +=
                    compatibility::source_acceleration(origin, planet.mass, frame.position);
            }
        }
        let support = snapshot.and_then(|snapshot| snapshot.support);
        SurfaceMotionObservation {
            planet: planet_index,
            preset: self.motion_preset,
            planet_position: frame.position,
            planet_velocity: frame.linear_velocity,
            planet_angle: frame.angle,
            planet_spin: frame.angular_velocity,
            scripted_acceleration,
            external_gravity_at_center,
            surface_velocity,
            relative_velocity: self
                .world
                .physics
                .world
                .velocity_at_point(body, actor.position)
                .unwrap()
                - surface_velocity,
            support_velocity: support.map(|contact| contact.velocity),
            support_relative_speed: support.map(|contact| {
                (self
                    .world
                    .physics
                    .world
                    .velocity_at_point(body, contact.position)
                    .unwrap()
                    - contact.velocity)
                    .length()
            }),
        }
    }

    pub(super) fn record_motion_step(&mut self, before: StepSample, input: SurfaceSortieAction) {
        if before.planet != self.motion_planet_index() {
            self.idle_anchor = None;
        }
        let after = self.spaceling_snapshot();
        let available = self.vehicle_available();
        let ship = &self.world.ships[self.pilot.vehicle.0];
        let frame = self.planet_motion();
        let metrics = &mut self.motion_metrics;
        metrics.on_foot_ticks += u64::from(after.is_some());
        metrics.supported_ticks += u64::from(after.is_some_and(SpacelingSnapshot::grounded));
        metrics.ship_landed_ticks += u64::from(self.landing.phase == LandingPhase::Landed);
        if let (Some(before), Some(after)) = (before.snapshot, after) {
            let jumps = after.jumps.saturating_sub(before.jumps);
            metrics.jumps += jumps;
            metrics.knockdowns += after.knockdowns.saturating_sub(before.knockdowns);
            metrics.pilot_support_losses +=
                u64::from(before.grounded() && !after.grounded() && jumps == 0);
        }
        metrics.ship_support_losses += u64::from(
            before.landing.supported_feet > 0
                && self.landing.supported_feet == 0
                && ship.thrust == 0.0,
        );
        metrics.landings += u64::from(
            before.landing.phase != LandingPhase::Landed
                && self.landing.phase == LandingPhase::Landed,
        );
        metrics.departures += u64::from(
            before.landing.phase == LandingPhase::Landed
                && self.landing.phase != LandingPhase::Landed
                && ship.thrust > 0.0,
        );
        // Called before service healing. A destroyed/transformed ship loses
        // its remaining old health, even if its new pod starts with fresh life.
        if before.ship_available {
            metrics.ship_damage +=
                (before.ship_health - if available { ship.life } else { 0.0 }).max(0.0);
        }
        let idle = if let Some(snapshot) = after {
            snapshot.grounded() && snapshot.balance == SpacelingBalance::Balanced
        } else {
            self.landing.phase == LandingPhase::Landed
        } && input.horizontal == 0.0
            && !input.primary_held
            && !input.interact_held
            && !input.brake_held;
        if idle {
            let position = after.map_or(ship.position + SHIP_PIVOT, |snapshot| {
                snapshot.motion.position
            });
            let local = (position - frame.position).rotate_radians(-frame.angle);
            let on_foot = after.is_some();
            let anchor = self
                .idle_anchor
                .filter(|(foot, _)| *foot == on_foot)
                .map_or(local, |(_, anchor)| anchor);
            self.idle_anchor = Some((on_foot, anchor));
            metrics.idle_drift = local.distance_to(anchor);
            metrics.max_idle_drift = metrics.max_idle_drift.max(metrics.idle_drift);
        } else {
            self.idle_anchor = None;
            metrics.idle_drift = 0.0;
        }
    }
}
