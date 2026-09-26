//! Shared nominal free-flight motor integration for observational forecasts.
use crate::flight_pilot::FlightIntent;
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    flight::{FlightControlLimits, WING_TRANSITION_SECONDS},
    pilot::PilotMotion,
};

const DT: f32 = 1.0 / 60.0;

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
