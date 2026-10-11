//! Optional hull comparison for a new ownership-based pursuit only.
use super::*;

pub const PURSUIT_HEALTH_PROFILE: &str = "discretionary_pursuit_hull_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PursuitHealthDecision {
    Admitted,
    WeakerHull,
    InvalidHull,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PursuitHealthCheck {
    pub tick: u64,
    pub opponent: PlayerId,
    pub own_hull: Option<f32>,
    pub opponent_hull: Option<f32>,
    pub decision: PursuitHealthDecision,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PursuitHealthTelemetry {
    pub checks: u64,
    pub admitted: u64,
    pub deferred: u64,
    pub last: Option<PursuitHealthCheck>,
}

impl MaterialMissionPilot {
    pub(crate) fn configure_pursuit_health(&mut self, enabled: bool) {
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        self.telemetry.pursuit_health = enabled.then(PursuitHealthTelemetry::default);
    }

    pub(super) fn admit_discretionary_pursuit(&mut self, o: &MissionObservationV1) -> bool {
        let Some(gate) = &mut self.telemetry.pursuit_health else {
            return true;
        };
        let p = &o.local.combat.recovery.flight.pilot;
        let target = o.local.combat.target.unwrap();
        // Both actors are full ships here. Compare remaining hull units, not
        // an inferred win probability or a cutoff fitted to one observed loss.
        let own_hull = p.ship_health.is_finite().then_some(p.ship_health);
        let opponent_hull = target.health.is_finite().then_some(target.health);
        let decision = match (own_hull, opponent_hull) {
            (Some(own), Some(other)) if own > 0.0 && other > 0.0 => {
                if own >= other {
                    PursuitHealthDecision::Admitted
                } else {
                    PursuitHealthDecision::WeakerHull
                }
            }
            _ => PursuitHealthDecision::InvalidHull,
        };
        let admitted = decision == PursuitHealthDecision::Admitted;
        gate.checks += 1;
        gate.admitted += u64::from(admitted);
        gate.deferred += u64::from(!admitted);
        gate.last = Some(PursuitHealthCheck {
            tick: p.tick,
            opponent: target.owner,
            own_hull,
            opponent_hull,
            decision,
        });
        admitted
    }
}

#[cfg(test)]
#[path = "mission_pursuit_health_tests.rs"]
mod tests;
