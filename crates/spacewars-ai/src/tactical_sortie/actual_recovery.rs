//! Opt-in capture abandonment after one completed local actual-hatch attempt.
//! Unknown fallback routes stay unknown; this policy only bounds waiting.
use super::*;
use scenario_spacewars::surface_sortie::live_planning::{
    ActualLocalAttemptFailure, MAX_SURVEY_AGE_TICKS, ObjectiveWorkState,
};

pub const ACTUAL_ROUTE_RECOVERY_PROFILE: &str = "actual_local_failure_abort_v1";
pub const ACTUAL_ROUTE_ABORT_REASON: &str = "actual hatch local attempt exhausted";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ActualRouteAbort {
    pub tick: u64,
    pub generation: u64,
    pub request_tick: u64,
    pub measurement_tick: u64,
    pub objective: LandingObjective,
    pub attempt: ActualLocalAttemptFailure,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ActualRouteRecovery {
    pub abort: Option<ActualRouteAbort>,
}

impl TacticalSortiePilot {
    pub(crate) fn enable_actual_route_recovery(&mut self, enabled: bool) {
        assert!(!enabled || self.commit_descent);
        self.telemetry.actual_route_recovery = enabled.then(ActualRouteRecovery::default);
    }

    /// Called only at the aboard, landed, transfer-ready gate, after checking
    /// any required landing site. Positive current actual routes keep priority.
    pub(super) fn abort_failed_actual_attempt(
        &mut self,
        o: &TacticalSortieObservationV1,
        objective: LandingObjective,
    ) -> bool {
        if self.telemetry.actual_route_recovery.is_none()
            || o.objective_work != Some(ObjectiveWorkState::Pending)
        {
            return false;
        }
        let p = &o.combat.recovery.flight.pilot;
        let Some(evidence) = o.objective_evidence else {
            return false;
        };
        let (
            Some(attempt),
            Some(source),
            Some(generation),
            Some(request_tick),
            Some(measurement_tick),
        ) = (
            evidence.actual_local_failure,
            evidence.source_objective,
            evidence.generation,
            evidence.request_tick,
            evidence.measurement_tick,
        )
        else {
            return false;
        };
        // Match the native request and its original pose/clock. The receipt is
        // historical attempt feedback, not a new measurement of the full graph.
        let matches = |old: LandingObjective| {
            old.matches(objective)
                && old.position.distance_to(objective.position) <= 0.002
                && (old.range - objective.range).abs() <= 0.0001
        };
        if evidence.tick != p.tick
            || evidence.invalidated_by.is_some()
            || evidence.submission_deferred_by.is_some()
            || !matches(evidence.objective)
            || !matches(source)
            || measurement_tick != request_tick
            || request_tick > p.tick
            || p.tick - measurement_tick > MAX_SURVEY_AGE_TICKS
            || attempt.actor != p.owner
            || !attempt.pose.matches(p)
        {
            return false;
        }
        self.telemetry.actual_route_recovery.as_mut().unwrap().abort = Some(ActualRouteAbort {
            tick: p.tick,
            generation,
            request_tick,
            measurement_tick,
            objective,
            attempt,
        });
        self.acquisition_reason("actual_local_attempt_failed");
        self.abort(p.tick, ACTUAL_ROUTE_ABORT_REASON);
        true
    }
}

#[cfg(test)]
mod tests;
