//! Relative closing-speed limits during a committed transfer.
use super::*;
use scenario_spacewars::surface_sortie::VehicleId;

pub const TRANSFER_SPEED_PROFILE: &str = "relative_transfer_speed_v1";
const FEASIBILITY_EPSILON: f32 = 0.001;
const MAX_CORRECTION: f32 = 55.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TransferSpeed {
    pub evaluated_ticks: u64,
    pub limited_ticks: u64,
    pub braking_ticks: u64,
    pub infeasible_ticks: u64,
    pub last: Option<TransferSpeedSample>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferSpeedSample {
    pub tick: u64,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub selected_tick: u64,
    pub vehicle: VehicleId,
    pub destination: usize,
    pub desired_before: Vec2,
    pub desired_after: Vec2,
    pub limited: bool,
    pub force_brake: bool,
    pub feasible: bool,
    pub candidates_checked: u32,
    pub limits: Vec<TransferSpeedLimit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TransferSpeedLimit {
    pub planet: usize,
    pub normal: Vec2,
    pub clearance: f32,
    pub closing_speed: f32,
    pub turn_seconds: f32,
    pub deceleration: f32,
    pub maximum_closing_speed: f32,
    pub minimum_world_normal_speed: f32,
}

fn limits(o: &MissionObservationV1, destination: usize) -> Vec<TransferSpeedLimit> {
    let f = &o.local.combat.recovery.flight;
    let p = &f.pilot;
    // Reserve a half-turn at the observed turn limit plus wing-opening time.
    // Like boundary guidance, use half nominal braking and reserve inward gravity.
    // These are guidance estimates; contacts and future motion remain physical.
    let turn_seconds = std::f32::consts::PI / f.flight.limits.turn_speed.max(0.1) + 0.6;
    o.planets
        .iter()
        .filter(|v| v.index != destination)
        .map(|v| {
            let offset = p.ship.position - v.motion.position;
            let normal = offset.normalized();
            let clearance = offset.length() - v.radius;
            let closing_speed = (-(p.ship.velocity - v.motion.velocity).dot(normal)).max(0.0);
            let deceleration = (f.flight.limits.brake_acceleration * 0.5
                - (-p.gravity.dot(normal)).max(0.0))
            .clamp(5.0, 25.0);
            let reserve = deceleration * turn_seconds;
            let maximum_closing_speed =
                (reserve * reserve + 2.0 * deceleration * (clearance - 70.0).max(0.0)).sqrt()
                    - reserve;
            TransferSpeedLimit {
                planet: v.index,
                normal,
                clearance,
                closing_speed,
                turn_seconds,
                deceleration,
                maximum_closing_speed,
                minimum_world_normal_speed: v.motion.velocity.dot(normal) - maximum_closing_speed,
            }
        })
        .collect()
}

// In two dimensions the closest feasible velocity is the original velocity,
// a projection onto one constraint, or an intersection of two constraints.
fn project(desired: Vec2, limits: &[TransferSpeedLimit]) -> (Option<Vec2>, u32) {
    let feasible = |v: Vec2| {
        limits.iter().all(|l| {
            l.normal.length_squared() > 0.5
                && v.dot(l.normal) >= l.minimum_world_normal_speed - FEASIBILITY_EPSILON
        })
    };
    let mut checked = 1;
    if feasible(desired) {
        return (Some(desired), checked);
    }
    let mut best: Option<(Vec2, f32)> = None;
    let mut consider = |candidate: Vec2| {
        checked += 1;
        let distance = (candidate - desired).length_squared();
        if feasible(candidate)
            && distance <= MAX_CORRECTION * MAX_CORRECTION
            && best.is_none_or(|(_, previous)| distance < previous)
        {
            best = Some((candidate, distance));
        }
    };
    for (i, a) in limits.iter().enumerate() {
        if a.normal.length_squared() <= 0.5 {
            continue;
        }
        consider(
            desired
                + a.normal
                    * ((a.minimum_world_normal_speed - desired.dot(a.normal))
                        / a.normal.length_squared()),
        );
        for b in &limits[..i] {
            let det = a.normal.x * b.normal.y - a.normal.y * b.normal.x;
            if det.abs() <= 0.0001 {
                continue;
            }
            consider(Vec2::new(
                (a.minimum_world_normal_speed * b.normal.y
                    - a.normal.y * b.minimum_world_normal_speed)
                    / det,
                (a.normal.x * b.minimum_world_normal_speed
                    - a.minimum_world_normal_speed * b.normal.x)
                    / det,
            ));
        }
    }
    (best.map(|(v, _)| v), checked)
}

impl MaterialMissionPilot {
    pub(crate) fn configure_transfer_speed(&mut self, enabled: bool) {
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        assert!(!enabled || self.telemetry.transfer_approach.is_some());
        self.telemetry.transfer_speed = enabled.then(TransferSpeed::default);
    }

    pub(super) fn transfer_velocity(
        &mut self,
        o: &MissionObservationV1,
        desired: Vec2,
    ) -> (Vec2, bool) {
        let p = &o.local.combat.recovery.flight.pilot;
        let Some(approach) = self
            .telemetry
            .transfer_approach
            .as_ref()
            .and_then(|s| s.last)
            .filter(|s| s.tick == p.tick && s.used_for_transfer)
        else {
            return (desired, false);
        };
        let Some(state) = self.telemetry.transfer_speed.as_mut() else {
            return (desired, false);
        };
        let limits = limits(o, approach.destination);
        let (projected, candidates_checked) = project(desired, &limits);
        let feasible = projected.is_some();
        let after = projected.unwrap_or(desired);
        let limited = after != desired;
        let force_brake = !feasible
            || limits
                .iter()
                .any(|l| l.closing_speed > l.maximum_closing_speed + FEASIBILITY_EPSILON);
        state.evaluated_ticks += 1;
        state.limited_ticks += u64::from(limited);
        state.braking_ticks += u64::from(force_brake);
        state.infeasible_ticks += u64::from(!feasible);
        state.last = Some(TransferSpeedSample {
            tick: p.tick,
            started_tick: approach.started_tick,
            deadline_tick: approach.deadline_tick,
            selected_tick: approach.selected_tick,
            vehicle: approach.vehicle,
            destination: approach.destination,
            desired_before: desired,
            desired_after: after,
            limited,
            force_brake,
            feasible,
            candidates_checked,
            limits,
        });
        (after, force_brake)
    }
}

#[cfg(test)]
#[path = "mission_transfer_speed_tests.rs"]
mod tests;
