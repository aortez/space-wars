//! Optional present-aim laser fire without changing pursuit clearance guidance.
use super::*;
use crate::{RuleShipBrainConfig, combat_solution};
use engine_common::Action;

pub const PURSUIT_CLIMB_LASER_PROFILE: &str = "pursuit_climb_laser_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PursuitClimbLaserSource {
    Mission,
    Combat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PursuitClimbLaserDecision {
    Requested,
    Unavailable,
    ScheduledBreak,
    NoTarget,
    Occluded,
    Unready,
    InvalidMotion,
    AimOrRange,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PursuitClimbLaserCheck {
    pub tick: u64,
    pub source: PursuitClimbLaserSource,
    pub decision: PursuitClimbLaserDecision,
    pub break_until_tick: Option<u64>,
    pub distance: Option<f32>,
    pub heading_error: Option<f32>,
    /// Exact same-observation intent before adding the laser request.
    pub native_actions: [Action; 3],
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PursuitClimbLaserTelemetry {
    pub checks: u64,
    pub requested_ticks: u64,
    pub last: Option<PursuitClimbLaserCheck>,
}

impl MaterialMissionPilot {
    pub(crate) fn configure_pursuit_climb_laser(&mut self, enabled: bool) {
        assert!(!enabled || self.policy == crate::mission_policy::MissionPolicy::ValuePlanner);
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        self.telemetry.pursuit_climb_laser = enabled.then(PursuitClimbLaserTelemetry::default);
    }

    pub(super) fn pursuit_climb_laser_intent(
        &mut self,
        o: &MissionObservationV1,
        mut intent: CombatIntent,
        source: PursuitClimbLaserSource,
    ) -> CombatIntent {
        if self.telemetry.pursuit_climb_laser.is_none() {
            return intent;
        }
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let break_until_tick = self.patrol.telemetry().breaks.active_until_tick;
        let mut distance = None;
        let mut heading_error = None;
        let decision = if !o.match_rules
            || self.telemetry.goal != MissionGoal::Hunt
            || self.telemetry.pursuit.is_none()
            || self.capture.is_some()
            || self.recovery.is_some()
            || self.escaping_sun
            || !p.controls_armed
            || !p.queries_ready
            || !c.recovery.flight.flight.enabled
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.location == PilotLocation::OnFoot
        {
            PursuitClimbLaserDecision::Unavailable
        } else if break_until_tick.is_some_and(|until| p.tick < until) {
            PursuitClimbLaserDecision::ScheduledBreak
        } else if let Some(target) = c.target {
            if !target.visible || target.ground_occluded {
                PursuitClimbLaserDecision::Occluded
            } else if !c.laser_available {
                PursuitClimbLaserDecision::Unready
            } else {
                let position =
                    (target.motion.position - p.ship.position).rotate_radians(-p.ship.angle);
                let velocity =
                    (target.motion.velocity - p.ship.velocity).rotate_radians(-p.ship.angle);
                if [position.x, position.y, velocity.x, velocity.y, p.ship.spin]
                    .iter()
                    .any(|v| !v.is_finite())
                {
                    PursuitClimbLaserDecision::InvalidMotion
                } else {
                    // Reuse the material combat firing window. Guidance and
                    // break clocks already ran; this never asks them to aim again.
                    let solution = combat_solution(
                        position,
                        velocity,
                        p.ship.spin,
                        c.laser_available,
                        false,
                        &RuleShipBrainConfig {
                            laser_range: 250.0,
                            ..Default::default()
                        },
                    );
                    if !solution.distance.is_finite() || !solution.heading_error.is_finite() {
                        PursuitClimbLaserDecision::InvalidMotion
                    } else {
                        distance = Some(solution.distance);
                        heading_error = Some(solution.heading_error);
                        if solution.intent.laser {
                            PursuitClimbLaserDecision::Requested
                        } else {
                            PursuitClimbLaserDecision::AimOrRange
                        }
                    }
                }
            }
        } else {
            PursuitClimbLaserDecision::NoTarget
        };
        let gate = self.telemetry.pursuit_climb_laser.as_mut().unwrap();
        gate.checks += 1;
        gate.requested_ticks += u64::from(decision == PursuitClimbLaserDecision::Requested);
        gate.last = Some(PursuitClimbLaserCheck {
            tick: p.tick,
            source,
            decision,
            break_until_tick,
            distance,
            heading_error,
            native_actions: intent.encode(p.owner),
        });
        if decision == PursuitClimbLaserDecision::Requested {
            intent.weapons.laser = true;
        }
        intent
    }
}

#[cfg(test)]
#[path = "mission_pursuit_climb_laser_tests.rs"]
mod tests;
