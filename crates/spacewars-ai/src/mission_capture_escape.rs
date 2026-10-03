//! Bounded physical departure after a native actual-hatch capture abort.
//! This consumes existing observations and emits ordinary flight/weapon inputs.
use super::*;
use crate::tactical_sortie::ActualRouteAbort;
use disengagement::{BoundaryGuidance, CLEAR_RANGE, CLEAR_TICKS, DISENGAGEMENT_TICKS};
use scenario_spacewars::surface_sortie::{
    LandingPhase, VehicleId, landing_objective::LandingObjective,
};

pub const CAPTURE_ESCAPE_PROFILE: &str = "actual_capture_escape_v1";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CaptureEscape {
    pub attempts: u32,
    pub separated: u32,
    pub timed_out: u32,
    pub last: Option<CaptureEscapeAttempt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CaptureEscapeAttempt {
    pub abort: ActualRouteAbort,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub observed_tick: u64,
    pub finished_tick: Option<u64>,
    pub reason: Option<&'static str>,
    pub guidance: &'static str,
    pub vehicle: VehicleId,
    pub opponent: PlayerId,
    pub direction: Vec2,
    pub estimated_min_range: f32,
    pub estimated_clearance: f32,
    pub range: Option<f32>,
    pub opening_speed: Option<f32>,
    pub source_clearance: f32,
    pub clear_since: Option<u64>,
    pub boundary: BoundaryGuidance,
    pub controlled_ticks: u64,
    pub laser_ticks: u64,
    pub cannon_ticks: u64,
}

impl MaterialMissionPilot {
    pub(crate) fn configure_capture_escape(&mut self, enabled: bool) {
        assert!(enabled || self.telemetry.escape_travel.is_none());
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        assert!(
            !enabled
                || (self.actual_route_recovery
                    && self.policy == crate::mission_policy::MissionPolicy::ValuePlanner)
        );
        self.telemetry.capture_escape = enabled.then(CaptureEscape::default);
    }

    pub(super) fn capture_escaping(&self) -> bool {
        self.telemetry
            .capture_escape
            .as_ref()
            .and_then(|s| s.last)
            .is_some_and(|a| a.finished_tick.is_none())
    }

    pub(super) fn start_capture_escape(
        &mut self,
        o: &MissionObservationV1,
        abort: ActualRouteAbort,
    ) {
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        if self.telemetry.capture_escape.is_none()
            || self.capture_escaping()
            || self.disengaging()
            || self.capture.is_some()
            || self.recovery.is_some()
            || !o.match_rules
            || !p.controls_armed
            || !p.queries_ready
            || !c.recovery.flight.flight.enabled
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.tick != abort.tick + 1
            || abort.attempt.actor != p.owner
            || p.landing.phase != LandingPhase::Landed
            || !abort.attempt.pose.matches(p)
            || !LandingObjective::read(p)
                .is_some_and(|objective| objective.matches(abort.objective))
        {
            return;
        }
        let Some(target) = c.target.filter(|t| {
            t.visible
                && !t.ground_occluded
                && t.ship_form == Some(ShipForm::Ship)
                && t.health > 0.0
                && t.motion.position.distance_to(p.ship.position) < CLEAR_RANGE
        }) else {
            return;
        };
        let (direction, estimated_min_range, estimated_clearance) =
            disengagement::escape_direction(o, true);
        let deadline_tick = abort.tick + DISENGAGEMENT_TICKS;
        let s = self.telemetry.capture_escape.as_mut().unwrap();
        s.attempts += 1;
        s.last = Some(CaptureEscapeAttempt {
            abort,
            started_tick: p.tick,
            deadline_tick,
            observed_tick: p.tick,
            finished_tick: None,
            reason: None,
            guidance: "arming",
            vehicle: p.vehicle,
            opponent: target.owner,
            direction,
            estimated_min_range,
            estimated_clearance,
            range: Some(p.ship.position.distance_to(target.motion.position)),
            opening_speed: None,
            source_clearance: p.ship.position.distance_to(p.planet.motion.position)
                - p.planet.radius,
            clear_since: None,
            boundary: BoundaryGuidance::default(),
            controlled_ticks: 0,
            laser_ticks: 0,
            cannon_ticks: 0,
        });
        // Early separation may resume destination choice; new pursuit stays
        // deferred only until the same original, non-renewable deadline.
        self.next_pursuit_tick = self.next_pursuit_tick.max(deadline_tick);
        self.event(
            p.tick,
            "capture_escape_started",
            Some("actual hatch capture aborted near opponent"),
        );
    }

    fn end_capture_escape(&mut self, tick: u64, reason: &'static str) {
        let s = self.telemetry.capture_escape.as_mut().unwrap();
        let a = s.last.as_mut().unwrap();
        a.finished_tick = Some(tick);
        a.reason = Some(reason);
        a.guidance = "finished";
        s.separated += u32::from(reason == "separation established");
        s.timed_out += u32::from(reason == "escape deadline");
        self.event(tick, "capture_escape_ended", Some(reason));
    }

    /// Observe the original deadline even while solar avoidance or disarmed
    /// controls take priority. Missing targets never count as measured cover.
    pub(super) fn update_capture_escape(&mut self, o: &MissionObservationV1) {
        if !self.capture_escaping() {
            return;
        }
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let a = self
            .telemetry
            .capture_escape
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        a.observed_tick = p.tick;
        a.guidance = "higher_priority_control";
        if !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.vehicle != a.vehicle
            || !matches!(p.location, PilotLocation::Aboard(_))
            || self.recovery.is_some()
        {
            self.end_capture_escape(p.tick, "recovery required");
            return;
        }
        if !o.match_rules || self.capture.is_some() {
            self.end_capture_escape(p.tick, "surface task takes priority");
            return;
        }
        if p.tick >= a.deadline_tick {
            self.end_capture_escape(p.tick, "escape deadline");
            return;
        }
        let Some(planet) = o
            .planets
            .iter()
            .find(|planet| planet.index == a.abort.objective.planet)
        else {
            self.end_capture_escape(p.tick, "source planet unavailable");
            return;
        };
        a.source_clearance = p.ship.position.distance_to(planet.motion.position) - planet.radius;
        let target = c.target.filter(|t| {
            t.owner == a.opponent && t.ship_form == Some(ShipForm::Ship) && t.health > 0.0
        });
        a.range = target.map(|t| p.ship.position.distance_to(t.motion.position));
        a.opening_speed = target.map(|t| {
            (p.ship.velocity - t.motion.velocity)
                .dot((p.ship.position - t.motion.position).normalized())
        });
        let clear = p.controls_armed
            && p.queries_ready
            && c.recovery.flight.flight.enabled
            && p.landing.supported_feet == 0
            && a.source_clearance > 70.0
            && disengagement::stopping_clearance(o, p.ship.position, p.ship.velocity) > 20.0
            && target.is_some_and(|t| {
                t.ground_occluded
                    || (a.range.unwrap() >= CLEAR_RANGE && a.opening_speed.unwrap() >= 0.0)
            });
        if clear {
            a.clear_since.get_or_insert(p.tick);
        } else {
            a.clear_since = None;
        }
        if a.clear_since
            .is_some_and(|since| p.tick.saturating_sub(since) >= CLEAR_TICKS)
        {
            self.end_capture_escape(p.tick, "separation established");
        }
    }

    pub(super) fn capture_escape_intent(
        &mut self,
        o: &MissionObservationV1,
    ) -> Option<CombatIntent> {
        if !self.capture_escaping() {
            return None;
        }
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        if !c.recovery.flight.flight.enabled || !p.queries_ready {
            self.telemetry
                .capture_escape
                .as_mut()
                .unwrap()
                .last
                .as_mut()
                .unwrap()
                .guidance = if p.queries_ready {
                "flight_disabled"
            } else {
                "queries_unavailable"
            };
            return Some(CombatIntent::default());
        }
        let direction = self
            .telemetry
            .capture_escape
            .as_ref()
            .unwrap()
            .last
            .unwrap()
            .direction;
        let radial = p.ship.position - p.planet.motion.position;
        let up = radial.normalized();
        let falling = (-(p.ship.velocity - p.planet.motion.velocity).dot(up)).max(0.0);
        let mut intent = self.escape_flight(o, direction);
        let a = self
            .telemetry
            .capture_escape
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        a.guidance = if a.boundary.active {
            "boundary"
        } else if p.landing.supported_feet > 0 {
            "lift"
        } else if radial.length() - p.planet.radius < 70.0 + falling * falling / 50.0 {
            "climb"
        } else {
            "escape"
        };
        if a.guidance == "lift" {
            // Supported feet can prevent a flight turn. Use the established
            // radial lift until contact releases, then resume obstacle routing.
            intent.flight.controls = SurfaceSortieAction {
                horizontal: crate::pilot::heading(p, up),
                primary_held: up.dot(Vec2::Y.rotate_radians(p.ship.angle)) > 0.9,
                ..Default::default()
            };
            intent.flight.wings.closed = false;
        }
        a.controlled_ticks += 1;
        // Retain the combat controller's visibility, readiness, aim and ground
        // clearance checks. Its flight inputs cannot replace escape guidance.
        intent.weapons = self.patrol.intent(c).weapons;
        self.telemetry.combat = Some(self.patrol.telemetry().clone());
        a.laser_ticks += u64::from(intent.weapons.laser);
        a.cannon_ticks += u64::from(intent.weapons.cannon);
        Some(intent)
    }
}

#[cfg(test)]
#[path = "mission_capture_escape_tests.rs"]
pub(super) mod tests;
