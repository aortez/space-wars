//! Shared nominal free-flight motor integration for observational forecasts.
use crate::flight_pilot::FlightIntent;
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    flight::{FlightControlLimits, WING_TRANSITION_SECONDS},
    pilot::PilotMotion,
};

const DT: f32 = 1.0 / 60.0;

/// Optional transfer model. Other callers retain the original point-mass motor.
/// Cache only the hull centroid: controls and motion are never cached here.
#[derive(Clone, Default)]
pub(crate) struct BodyOriginMotor {
    center: Option<(f32, Vec2)>,
}

impl BodyOriginMotor {
    pub(crate) fn advance(
        &mut self,
        motion: &mut PilotMotion,
        sweep: &mut f32,
        intent: FlightIntent,
        frame_velocity: Vec2,
        gravity: Vec2,
    ) -> FlightControlLimits {
        let before = *motion;
        let limits = advance_motor(motion, sweep, intent, frame_velocity, gravity);
        let center = match self.center {
            Some((previous, center)) if previous == *sweep => center,
            _ => {
                let center =
                    scenario_spacewars::surface_sortie::flight::ship_local_center_of_mass(*sweep);
                self.center = Some((*sweep, center));
                center
            }
        };
        // Wing replacement preserves origin velocity before the command. Both
        // lever arms therefore use the new hull, not the previous wing shape.
        let old_offset = center.rotate_radians(before.angle);
        let new_offset = center.rotate_radians(motion.angle);
        let tangent = |v: Vec2| Vec2::new(-v.y, v.x);
        let old_rotation = tangent(old_offset) * before.spin;
        motion.position += old_offset - new_offset + old_rotation * DT;
        motion.velocity += old_rotation - tangent(new_offset) * motion.spin;
        limits
    }
}

pub(crate) fn advance_motor(
    motion: &mut PilotMotion,
    sweep: &mut f32,
    intent: FlightIntent,
    frame_velocity: Vec2,
    gravity: Vec2,
) -> FlightControlLimits {
    let controls = intent.controls;
    let closed = intent.wings.closed;
    *sweep += ((if closed { 1.0 } else { 0.0 }) - *sweep)
        .clamp(-DT / WING_TRANSITION_SECONDS, DT / WING_TRANSITION_SECONDS);
    let limits = FlightControlLimits::for_sweep(*sweep);
    let forward = Vec2::Y.rotate_radians(motion.angle);
    let relative = motion.velocity - frame_velocity;
    let thrust = controls.primary_held || closed && !controls.brake_held;
    let acceleration = gravity
        + if thrust {
            forward * limits.thrust_acceleration * limits.thrust_fraction(relative.dot(forward))
        } else {
            Vec2::ZERO
        }
        + if controls.brake_held {
            limits.braking(relative)
        } else {
            Vec2::ZERO
        };
    let strength = if controls.horizontal != 0.0 || controls.brake_held {
        1.0
    } else {
        0.25
    };
    let desired_spin = -controls.horizontal * limits.turn_speed;
    motion.spin += (desired_spin - motion.spin).clamp(
        -limits.turn_acceleration * strength * DT,
        limits.turn_acceleration * strength * DT,
    );
    motion.angle += motion.spin * DT;
    motion.velocity += acceleration * DT;
    motion.position += motion.velocity * DT;
    limits
}
