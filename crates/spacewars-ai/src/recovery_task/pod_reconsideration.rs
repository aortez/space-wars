//! One measured alternative after a pod approach exhausts its flight budget.
use super::*;
use scenario_spacewars::surface_sortie::pilot::{LANDING_SITE_COUNT, LandingSiteQuery};

const REVIEW_INTERVAL: u64 = 60;
const MAX_REVIEWS: u32 = 15;
const ATTEMPT_TICKS: u64 = 90 * 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PodReconsiderationTelemetry {
    pub blocked_tick: u64,
    pub previous_reason: &'static str,
    pub abandoned_site: PilotLandingSite,
    pub surveys: u32,
    pub last_survey_tick: Option<u64>,
    pub alternative: Option<PilotLandingSite>,
    pub started_tick: Option<u64>,
    pub deadline_tick: Option<u64>,
}

impl RecoverShipTask {
    /// Defaults off. Toggling this does not renew a spent review or attempt.
    pub fn set_pod_reconsideration(&mut self, enabled: bool) {
        self.pod_reconsideration_enabled = enabled;
        self.arm_pod_review();
    }

    pub(super) fn arm_pod_review(&mut self) {
        if !self.pod_reconsideration_enabled
            || self.invalid_observation
            || self.telemetry.status != TaskStatus::Blocked
            || self.telemetry.reason != Some(FLIGHT_BUDGET_REASON)
            || self.telemetry.landing_retries >= 4
            || self.telemetry.exited_tick.is_some()
            || self.telemetry.pod_reconsideration.is_some()
        {
            return;
        }
        let Some(site) = self.site else { return };
        self.telemetry.pod_reconsideration = Some(PodReconsiderationTelemetry {
            blocked_tick: self.telemetry.goal_since,
            previous_reason: FLIGHT_BUDGET_REASON,
            abandoned_site: site,
            surveys: 0,
            last_survey_tick: None,
            alternative: None,
            started_tick: None,
            deadline_tick: None,
        });
    }

    pub(super) fn pod_review_due(&self, tick: u64) -> bool {
        self.pod_reconsideration_enabled
            && !self.invalid_observation
            && self.telemetry.status == TaskStatus::Blocked
            && self.telemetry.reason == Some(FLIGHT_BUDGET_REASON)
            && self
                .telemetry
                .pod_reconsideration
                .as_ref()
                .is_some_and(|r| {
                    r.started_tick.is_none()
                        && r.surveys < MAX_REVIEWS
                        && tick > r.blocked_tick
                        && tick.saturating_sub(r.blocked_tick) <= POD_SITE_WAIT_TICKS
                        && r.last_survey_tick
                            .is_none_or(|last| tick.saturating_sub(last) >= REVIEW_INTERVAL)
                })
    }

    pub(super) fn reconsider_pod(&mut self, o: &RecoveryTaskObservationV1) {
        let p = &o.flight.pilot;
        if !self.pod_review_due(p.tick)
            || !p.queries_ready
            || p.site_query != LandingSiteQuery::Survey
        {
            return;
        }
        let review = self.telemetry.pod_reconsideration.as_mut().unwrap();
        review.surveys += 1;
        review.last_survey_tick = Some(p.tick);
        if !p.controls_armed
            || !p.ship_available
            || p.ship_form != ShipForm::EscapePod
            || p.location != PilotLocation::Aboard(p.vehicle)
            || p.planet.index != review.abandoned_site.id.planet
        {
            return;
        }
        let valid = |s: &&PilotLandingSite| {
            s.id.planet == p.planet.index
                && s.id.bearing < LANDING_SITE_COUNT
                && s.revision == p.planet.revision
                && [
                    s.local_position,
                    s.vehicle_position,
                    s.position,
                    s.normal,
                    s.velocity,
                ]
                .iter()
                .all(|v| v.x.is_finite() && v.y.is_finite())
                && (0.9..=1.1).contains(&s.normal.length_squared())
        };
        // A changed revision alone is not an opportunity. Compare current
        // measurements, never an old world-space point on a rotating planet.
        let previous_distance = o
            .sites
            .iter()
            .filter(valid)
            .find(|s| s.id == review.abandoned_site.id)
            .map(|s| s.vehicle_position.distance_to(p.ship.position));
        let alternative = o
            .sites
            .iter()
            .filter(valid)
            .filter(|s| s.id != review.abandoned_site.id)
            .filter(|s| {
                previous_distance.is_none_or(|distance| {
                    s.vehicle_position.distance_to(p.ship.position) + 1.0 < distance
                })
            })
            .filter(|s| {
                !self.telemetry.landing_rejections.iter().any(|r| {
                    r.site == s.id
                        && p.tick < r.retry_after_tick
                        && r.local_position.distance_to(s.local_position) < 0.5
                })
            })
            .min_by(|a, b| {
                a.vehicle_position
                    .distance_to(p.ship.position)
                    .total_cmp(&b.vehicle_position.distance_to(p.ship.position))
            })
            .copied();
        let Some(alternative) = alternative else {
            return;
        };
        review.alternative = Some(alternative);
        review.started_tick = Some(p.tick);
        review.deadline_tick = Some(p.tick.saturating_add(ATTEMPT_TICKS));
        // Restart approach control only. Preserve all rejection, loss, native
        // rebuild, relocation and replacement history and the original clock.
        self.site = None;
        self.telemetry.site = None;
        self.telemetry.site_search_since = None;
        self.stabilized = false;
        self.final_descent = false;
        self.stabilization_window = None;
        self.settled_since = None;
        self.pwm = 0.0;
        self.goal(RecoveryGoal::StabilizePod, p.tick);
    }

    pub(super) fn check_pod_reconsideration_deadline(&mut self, p: &PilotObservationV1) {
        if self.telemetry.status != TaskStatus::Running
            || self.telemetry.rebuild_boarding.is_some()
            || self
                .telemetry
                .pod_reconsideration
                .as_ref()
                .and_then(|r| r.deadline_tick)
                .is_none_or(|deadline| p.tick <= deadline)
        {
            return;
        }
        // A physical transfer on the last allowed control tick is observable
        // one tick later, including the neutral rearm tick.
        if p.ship_available
            && p.ship_form == ShipForm::Ship
            && p.location == PilotLocation::Aboard(p.vehicle)
        {
            self.goal(RecoveryGoal::Complete, p.tick);
            self.telemetry.completed_tick = Some(p.tick);
        } else {
            self.block("reconsidered pod recovery exceeded ninety seconds", p.tick);
        }
    }
}

#[cfg(test)]
mod tests;
