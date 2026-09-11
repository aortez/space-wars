//! Local flight assist and contact-based landing, opt-in to this fixture only.
//! No pose snaps, docking constraints, or extra gravity solve.

use super::*;

const ASSIST_HEIGHT: f32 = 25.0;
const FULL_ASSIST_HEIGHT: f32 = 8.0;
const ASSIST_ANGLE: f32 = 50.0;
const FULL_ASSIST_ANGLE: f32 = 15.0;
const LANDED_ANGLE: f32 = 20.0;
const SETTLE_SECONDS: f32 = 0.25;
const DESCENT_SPEED: f32 = 2.0;
const MAX_DESCENT_ACCELERATION: f32 = 30.0;
const MAX_LATERAL_ACCELERATION: f32 = 12.0;
pub(super) const THRUST_ACCELERATION: f32 = 45.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LandingPhase {
    #[default]
    Flying,
    Assisted,
    Settling,
    Landed,
}

impl LandingPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Flying => "FLYING",
            Self::Assisted => "LANDING ASSIST",
            Self::Settling => "SETTLING",
            Self::Landed => "LANDED",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct LandingTelemetry {
    /// Approach/contact frame. Only supported feet and the settled gate prove landing.
    pub planet: Option<usize>,
    pub phase: LandingPhase,
    pub altitude: f32,
    /// Radial clearance beneath each rear foot; rays do not prove contact.
    pub foot_clearances: [f32; 2],
    pub angle_degrees: f32,
    /// Positive is moving toward the ground; measured in the surface frame.
    pub descent_speed: f32,
    pub lateral_speed: f32,
    pub relative_spin: f32,
    pub supported_feet: usize,
    /// An earned landing retains two live feet across a voxel-corner normal change.
    pub corner_support: bool,
    pub assist_strength: f32,
    pub settled_seconds: f32,
}

impl LandingTelemetry {
    pub(super) fn measure(
        physics: &physics::SpacewarsPhysics,
        index: usize,
        planet_index: usize,
        planet: &PlanetState,
    ) -> Self {
        let Some(motion) = physics.world.motion(physics.ship_body(index)) else {
            return Self::default();
        };
        let surface = motion::SurfaceFrame::read(physics, planet_index);
        let up = (motion.position - surface.position).normalized();
        let right = Vec2::new(up.y, -up.x);
        let relative = physics
            .world
            .velocity_at_point(physics.ship_body(index), motion.position)
            .unwrap()
            - motion::point_velocity(surface, motion.position);
        let angle_degrees = Vec2::Y
            .rotate_radians(motion.angle)
            .dot(up)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees();
        let (feet, foot_radius) = physics.landing_geometry(index);
        let foot_clearances = feet.map(|foot| {
            if physics.material_planets.contains(&planet_index) {
                let point = motion.position + foot.rotate_radians(motion.angle);
                return physics
                    .material_ground_ray(planet_index, point + up * 0.1, -up, ASSIST_HEIGHT + 1.0)
                    .map_or(ASSIST_HEIGHT + 1.0, |hit| hit.distance - 0.1 - foot_radius);
            }
            (motion.position + foot.rotate_radians(motion.angle)).distance_to(surface.position)
                - planet.radius * BODY_BOUNDS_RADIUS_SCALE
                - foot_radius
        });
        let altitude = foot_clearances.into_iter().fold(f32::INFINITY, f32::min);
        let near =
            ((ASSIST_HEIGHT - altitude) / (ASSIST_HEIGHT - FULL_ASSIST_HEIGHT)).clamp(0.0, 1.0);
        let aligned =
            ((ASSIST_ANGLE - angle_degrees) / (ASSIST_ANGLE - FULL_ASSIST_ANGLE)).clamp(0.0, 1.0);
        // Smooth both transitions so crossing the cone/height boundary is not a switch.
        let smooth = |x: f32| x * x * (3.0 - 2.0 * x);
        Self {
            planet: Some(planet_index),
            altitude,
            foot_clearances,
            angle_degrees,
            descent_speed: -relative.dot(up),
            lateral_speed: relative.dot(right),
            relative_spin: motion.angular_velocity - surface.angular_velocity,
            supported_feet: physics.landing_feet_supported(index, planet_index, up),
            assist_strength: smooth(near) * smooth(aligned),
            ..Self::default()
        }
    }

    pub(super) fn update(
        &mut self,
        physics: &physics::SpacewarsPhysics,
        index: usize,
        planet_index: usize,
        planet: &PlanetState,
        ship: &ShipState,
        dt: f32,
    ) {
        let mut next = Self::measure(physics, index, planet_index, planet);
        if self.phase == LandingPhase::Landed
            && self.planet == next.planet
            && !ship.dead
            && ship.form == ShipForm::Ship
            && physics.material_planets.contains(&planet_index)
            && let Some(body) = physics.world.motion(physics.ship_body(index))
        {
            let surface = motion::SurfaceFrame::read(physics, planet_index);
            let strict_feet = next.supported_feet;
            next.supported_feet = physics
                .parked_landing_support_contacts(
                    index,
                    planet_index,
                    (body.position - surface.position).normalized(),
                )
                .into_iter()
                .flatten()
                .count();
            next.corner_support = next.supported_feet == 2 && strict_feet < 2;
        }
        next.assist_strength *= 1.0 - flight::sweep(ship);
        let wings_open = !ship.wings_closed && flight::sweep(ship) <= 0.001;
        let settled = !ship.dead
            && wings_open
            && ship.thrust == 0.0
            && next.supported_feet == 2
            && next.angle_degrees < LANDED_ANGLE
            && next.descent_speed.abs() < SETTLED_SPEED
            && next.lateral_speed.abs() < SETTLED_SPEED
            && next.relative_spin.abs() < 0.2;
        next.settled_seconds = if settled {
            let previous = if self.planet == next.planet {
                self.settled_seconds
            } else {
                0.0
            };
            (previous + dt).min(SETTLE_SECONDS)
        } else {
            0.0
        };
        next.phase = if next.settled_seconds >= SETTLE_SECONDS {
            LandingPhase::Landed
        } else if wings_open && next.supported_feet > 0 && next.angle_degrees < LANDED_ANGLE {
            LandingPhase::Settling
        } else if next.assist_strength > 0.0 && ship.thrust == 0.0 {
            LandingPhase::Assisted
        } else {
            LandingPhase::Flying
        };
        *self = next;
    }
}

impl SurfacePilot {
    pub(crate) fn vehicle_index(&self) -> usize {
        self.vehicle.0
    }

    pub(crate) fn control_vehicle(
        &mut self,
        physics: &mut physics::SpacewarsPhysics,
        ship: &ShipState,
        planets: &[PlanetState],
        dt: f32,
    ) {
        if ship.dead || (ship.form == ShipForm::EscapePod && self.recovery.is_none()) {
            return;
        }
        self.select_approach_planet(physics, planets);
        let planet = &planets[self.planet];
        let body = physics.ship_body(self.vehicle.0);
        let Some(motion) = physics.world.motion(body) else {
            return;
        };
        let surface = motion::SurfaceFrame::read(physics, self.planet);
        let mut landing = LandingTelemetry::measure(physics, self.vehicle.0, self.planet, planet);
        let flight_enabled = self.flight_enabled && ship.form == ShipForm::Ship;
        let sweep = if flight_enabled {
            flight::sweep(ship)
        } else {
            0.0
        };
        let limits = flight::FlightControlLimits::for_sweep(sweep);
        landing.assist_strength *= 1.0 - sweep;
        let up = (motion.position - surface.position).normalized();
        let right = Vec2::new(up.y, -up.x);
        let forward = Vec2::Y.rotate_radians(motion.angle);
        let relative = physics
            .world
            .velocity_at_point(body, motion.position)
            .unwrap()
            - motion::point_velocity(surface, motion.position);
        let thrust = ship.thrust;
        let governor = if flight_enabled {
            limits.thrust_fraction(relative.dot(forward))
        } else {
            1.0
        };
        let thrust_direction = if self.pod_righting_lift(physics, ship, &landing, dt) {
            up
        } else {
            forward
        };
        let mut acceleration = thrust_direction * (thrust * limits.thrust_acceleration * governor);
        if ship.brake > 0.0 {
            acceleration += limits.braking(relative);
        } else if thrust == 0.0 {
            acceleration += right
                * (-landing.lateral_speed * 3.0)
                    .clamp(-MAX_LATERAL_ACCELERATION, MAX_LATERAL_ACCELERATION)
                * landing.assist_strength;
            if landing.supported_feet == 0 {
                // A gentle nonzero sink rate, not hover. Gravity comes from the
                // shared solver. Bounded thrust cannot rescue a very fast fall.
                let gravity = self.ship_gravity_delta / dt;
                let correction = (landing.descent_speed - DESCENT_SPEED) * 6.0 - gravity.dot(up);
                acceleration +=
                    up * correction.clamp(0.0, MAX_DESCENT_ACCELERATION) * landing.assist_strength;
            }
        }
        physics
            .world
            .apply_velocity_delta(body, acceleration * dt, true);

        // Rate control, never automatic orientation. The pilot must point the
        // nose outward. Stronger damping near touchdown removes unwanted spin.
        let desired_spin = -ship.turn * limits.turn_speed
            + if landing.assist_strength > 0.0 {
                surface.angular_velocity
            } else {
                0.0
            };
        let strength = if ship.turn != 0.0 || ship.brake > 0.0 {
            1.0
        } else {
            0.25 + 0.75 * landing.assist_strength
        };
        let spin_delta = (desired_spin - motion.angular_velocity).clamp(
            -limits.turn_acceleration * strength * dt,
            limits.turn_acceleration * strength * dt,
        );
        let velocity = physics
            .world
            .motion(body)
            .expect("existing ship")
            .linear_velocity;
        physics
            .world
            .set_velocity(body, velocity, motion.angular_velocity + spin_delta, true);
    }
}
