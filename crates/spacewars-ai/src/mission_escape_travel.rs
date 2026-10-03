//! One bounded destination transfer after a completed actual-hatch escape.
use super::*;
use scenario_spacewars::surface_sortie::{LandingPhase, VehicleId};

pub const ESCAPE_TRAVEL_PROFILE: &str = "escape_travel_commitment_v1";
pub(super) const TRANSFER_TICKS: u64 = 60 * 60;
pub(super) const TRANSFER_PROGRESS_TICKS: u64 = 20 * 60;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct EscapeTravel {
    pub attempts: u32,
    pub capture_handoffs: u32,
    pub timed_out: u32,
    pub last: Option<EscapeTravelAttempt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EscapeTravelAttempt {
    pub escape: CaptureEscapeAttempt,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub observed_tick: u64,
    pub finished_tick: Option<u64>,
    pub reason: Option<&'static str>,
    pub guidance: &'static str,
    pub vehicle: VehicleId,
    pub destination: Option<usize>,
    pub selected_tick: Option<u64>,
    pub progress_tick: u64,
    pub distance: Option<f32>,
    pub deferred_pursuit_ticks: u64,
    pub last_deferred_tick: Option<u64>,
    pub last_deferred_reason: Option<&'static str>,
    pub travel_ticks: u64,
    pub laser_ticks: u64,
    pub cannon_ticks: u64,
}

impl MaterialMissionPilot {
    pub(crate) fn configure_escape_travel(&mut self, enabled: bool) {
        assert!(enabled || self.telemetry.transfer_approach.is_none());
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        assert!(!enabled || self.telemetry.capture_escape.is_some());
        self.telemetry.escape_travel = enabled.then(EscapeTravel::default);
    }

    pub(super) fn committing_escape_travel(&self) -> bool {
        self.telemetry
            .escape_travel
            .as_ref()
            .and_then(|s| s.last)
            .is_some_and(|a| a.finished_tick.is_none())
    }

    pub(super) fn prepare_escape_travel(&mut self, o: &MissionObservationV1) {
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let Some(escape) = self.telemetry.capture_escape.as_ref().and_then(|s| s.last) else {
            return;
        };
        if self.telemetry.escape_travel.is_none()
            || self.committing_escape_travel()
            || escape.finished_tick != Some(p.tick)
            || !matches!(
                escape.reason,
                Some("escape deadline" | "separation established")
            )
            || self
                .telemetry
                .escape_travel
                .as_ref()
                .and_then(|s| s.last)
                .is_some_and(|a| a.escape.abort.tick == escape.abort.tick)
            || self.capture.is_some()
            || self.recovery.is_some()
            || self.disengaging()
            || self.telemetry.target.is_some()
            || self.telemetry.pursuit.is_some()
            || !o.match_rules
            || !p.controls_armed
            || !p.queries_ready
            || !c.recovery.flight.flight.enabled
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.vehicle != escape.vehicle
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.landing.phase != LandingPhase::Flying
            || p.landing.supported_feet != 0
        {
            return;
        }
        let s = self.telemetry.escape_travel.as_mut().unwrap();
        s.attempts += 1;
        s.last = Some(EscapeTravelAttempt {
            escape,
            started_tick: p.tick,
            deadline_tick: p.tick + TRANSFER_TICKS,
            observed_tick: p.tick,
            finished_tick: None,
            reason: None,
            guidance: "selecting",
            vehicle: p.vehicle,
            destination: None,
            selected_tick: None,
            progress_tick: p.tick,
            distance: None,
            deferred_pursuit_ticks: 0,
            last_deferred_tick: None,
            last_deferred_reason: None,
            travel_ticks: 0,
            laser_ticks: 0,
            cannon_ticks: 0,
        });
        self.event(
            p.tick,
            "escape_travel_started",
            Some("selecting one post-escape transfer"),
        );
    }

    pub(super) fn end_escape_travel(&mut self, tick: u64, reason: &'static str) {
        if !self.committing_escape_travel() {
            return;
        }
        let s = self.telemetry.escape_travel.as_mut().unwrap();
        let a = s.last.as_mut().unwrap();
        a.observed_tick = tick;
        a.finished_tick = Some(tick);
        a.reason = Some(reason);
        a.guidance = "finished";
        s.capture_handoffs += u32::from(reason == "capture task started");
        s.timed_out += u32::from(reason == "transfer deadline");
        self.event(tick, "escape_travel_ended", Some(reason));
    }

    pub(super) fn observe_escape_travel(&mut self, o: &MissionObservationV1) {
        if !self.committing_escape_travel() {
            return;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let a = self
            .telemetry
            .escape_travel
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        a.observed_tick = p.tick;
        a.guidance = "higher_priority_control";
        a.progress_tick = self.progress_tick;
        let destination = a
            .destination
            .and_then(|index| o.planets.iter().find(|v| v.index == index));
        a.distance = destination.map(|v| p.ship.position.distance_to(v.motion.position));
        let reason = if !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.vehicle != a.vehicle
            || !matches!(p.location, PilotLocation::Aboard(_))
            || self.recovery.is_some()
        {
            Some("recovery required")
        } else if !o.match_rules {
            Some("match rules unavailable")
        } else if self.capture.is_some() {
            Some("capture task started")
        } else if p.tick >= a.deadline_tick {
            Some("transfer deadline")
        } else if a.destination.is_some()
            && (self.telemetry.target != a.destination
                || Some(self.selected_tick) != a.selected_tick)
        {
            Some("destination changed")
        } else if a.destination.is_some()
            && destination.is_none_or(|v| {
                v.claim
                    .as_ref()
                    .is_some_and(|claim| claim.owner == Some(p.owner))
            })
        {
            Some("destination unavailable or secured")
        } else if a.destination.is_some()
            && p.tick.saturating_sub(self.progress_tick) > TRANSFER_PROGRESS_TICKS
        {
            Some("transfer exhausted its progress budget")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.end_escape_travel(p.tick, reason);
        }
    }

    pub(super) fn defer_escape_travel_pursuit(
        &mut self,
        o: &MissionObservationV1,
        reason: &'static str,
    ) -> bool {
        if !self.committing_escape_travel() {
            return false;
        }
        let tick = o.local.combat.recovery.flight.pilot.tick;
        let a = self
            .telemetry
            .escape_travel
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        a.deferred_pursuit_ticks += 1;
        a.last_deferred_tick = Some(tick);
        a.last_deferred_reason = Some(reason);
        true
    }

    pub(super) fn finish_escape_travel_frame(
        &mut self,
        o: &MissionObservationV1,
        intent: &mut CombatIntent,
    ) {
        if !self.committing_escape_travel() {
            return;
        }
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let a = self
            .telemetry
            .escape_travel
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        if a.destination.is_none() {
            if self.telemetry.target.is_some()
                && self.selected_tick == a.started_tick
                && matches!(
                    self.telemetry.goal,
                    MissionGoal::Transfer | MissionGoal::Launch
                )
            {
                a.destination = self.telemetry.target;
                a.selected_tick = Some(self.selected_tick);
            } else {
                self.end_escape_travel(
                    p.tick,
                    if self.capture.is_some() {
                        "capture task started"
                    } else {
                        "no transfer selected"
                    },
                );
                return;
            }
        }
        // Observe same-tick replan/capture transitions before adding weapons.
        self.observe_escape_travel(o);
        if !self.committing_escape_travel() {
            return;
        }
        if !p.controls_armed
            || !p.queries_ready
            || !c.recovery.flight.flight.enabled
            || !matches!(
                self.telemetry.goal,
                MissionGoal::Transfer | MissionGoal::Launch
            )
        {
            return;
        }
        // Keep the ordinary transfer flight unchanged; reuse only the current
        // combat controller's defensively eligible weapon actions.
        intent.weapons = self.patrol.intent(c).weapons;
        self.telemetry.combat = Some(self.patrol.telemetry().clone());
        let a = self
            .telemetry
            .escape_travel
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        a.guidance = if self.telemetry.goal == MissionGoal::Launch {
            "launch"
        } else {
            "transfer"
        };
        a.travel_ticks += 1;
        a.laser_ticks += u64::from(intent.weapons.laser);
        a.cannon_ticks += u64::from(intent.weapons.cannon);
    }
}

#[cfg(test)]
#[path = "mission_escape_travel_tests.rs"]
pub(super) mod tests;
