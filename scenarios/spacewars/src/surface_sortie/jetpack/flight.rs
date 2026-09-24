//! Shared flight commands for the real controller and its bounded prediction.
use super::*;

pub const LAUNCH_CHARGE: f32 = 0.98;
pub const LANDING_RESERVE: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FlightPhase {
    Lift,
    Cross,
    Descend,
}

#[derive(Debug, Clone, Copy)]
pub struct FlightSample {
    pub radius: f32,
    pub radial_speed: f32,
    /// Error to the start in Lift, and the destination in the other phases.
    pub error: f32,
    pub lateral_speed: f32,
    pub frame_speed: f32,
    pub air_speed: f32,
    pub supported: bool,
    pub previous_jump: bool,
}

pub fn flight_command(
    plan: &CrossingPlan,
    phase: &mut FlightPhase,
    s: FlightSample,
) -> SurfaceSortieAction {
    let height = spaceling_geometry::HALF_HEIGHT;
    let target_radius = if *phase == FlightPhase::Descend {
        plan.destination.length() + height * 0.6
    } else {
        plan.cruise_radius
    };
    let desired_rise = ((target_radius - s.radius) * 1.8).clamp(-6.0, 7.0);
    let mut burn = s.radial_speed < desired_rise;
    if *phase == FlightPhase::Descend
        && s.radius < plan.destination.length() + height + 0.6
        && s.error.abs() < CROSSING_ARRIVAL_RANGE
    {
        burn = false;
    }
    // A get-up can consume the launch press. Release before retrying on ground.
    if *phase == FlightPhase::Lift && s.supported && s.previous_jump {
        burn = false;
    }
    SurfaceSortieAction {
        primary_held: burn,
        horizontal: (((s.error * 1.8).clamp(-8.0, 8.0) + s.frame_speed) / s.air_speed)
            .clamp(-1.0, 1.0),
        ..Default::default()
    }
}

pub fn advance_phase(plan: &CrossingPlan, phase: &mut FlightPhase, s: FlightSample) {
    if *phase == FlightPhase::Lift && s.radius > plan.cruise_radius - 0.4 {
        *phase = FlightPhase::Cross;
    } else if *phase == FlightPhase::Cross && s.error.abs() < 0.5 && s.lateral_speed.abs() < 1.0 {
        *phase = FlightPhase::Descend;
    }
}
