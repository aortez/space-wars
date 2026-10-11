//! Opt-in bounded escape from fresh hostile fire before the first capture site.
use super::*;
use disengagement::{BoundaryGuidance, CLEAR_RANGE, CLEAR_TICKS, DISENGAGEMENT_TICKS};
use engine_common::Action;
use scenario_spacewars::surface_sortie::{LandingPhase, VehicleId};

pub const ACQUISITION_DEFENSE_PROFILE: &str = "airborne_acquisition_defense_v1";
pub const ACQUISITION_CLEARANCE_PROFILE: &str = "airborne_acquisition_clearance_v1";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AcquisitionClearance {
    pub checks: u32,
    pub rejected: u32,
    pub last: Option<AcquisitionClearanceCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AcquisitionClearanceCheck {
    pub tick: u64,
    pub decision: &'static str,
    pub planet: usize,
    pub vehicle: VehicleId,
    pub opponent: PlayerId,
    pub hit_source: &'static str,
    pub capture: CaptureTelemetry,
    pub native_actions: [Action; 3],
    pub direction: Vec2,
    pub estimated_min_range: f32,
    pub estimated_clearance: f32,
}

fn clearance_decision(clearance: f32) -> &'static str {
    if !clearance.is_finite() {
        "nonfinite_forecast"
    } else if clearance < 0.0 {
        "negative_clearance"
    } else {
        "admitted"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AcquisitionDefense {
    pub attempts: u32,
    pub separated: u32,
    pub timed_out: u32,
    pub last: Option<AcquisitionDefenseAttempt>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AcquisitionDefenseAttempt {
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub observed_tick: u64,
    pub finished_tick: Option<u64>,
    pub reason: Option<&'static str>,
    pub guidance: &'static str,
    pub planet: usize,
    pub vehicle: VehicleId,
    pub opponent: PlayerId,
    pub hit_source: &'static str,
    /// Native current-tick acquisition and its intent before the handoff.
    pub capture: CaptureTelemetry,
    pub native_actions: [Action; 3],
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
    pub(crate) fn configure_acquisition_defense(&mut self, enabled: bool) {
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        assert!(!enabled || self.policy == crate::mission_policy::MissionPolicy::ValuePlanner);
        self.telemetry.acquisition_defense = enabled.then(AcquisitionDefense::default);
        if !enabled {
            self.telemetry.acquisition_clearance = None;
        }
    }

    pub(crate) fn configure_acquisition_clearance(&mut self, enabled: bool) {
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        assert!(!enabled || self.telemetry.acquisition_defense.is_some());
        self.telemetry.acquisition_clearance = enabled.then(AcquisitionClearance::default);
    }

    fn acquisition_defending(&self) -> bool {
        self.telemetry
            .acquisition_defense
            .as_ref()
            .and_then(|s| s.last.as_ref())
            .is_some_and(|a| a.finished_tick.is_none())
    }

    pub(super) fn acquisition_defense_handoff(
        &mut self,
        o: &MissionObservationV1,
        native: CombatIntent,
    ) -> CombatIntent {
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        if self.telemetry.acquisition_defense.is_none()
            || self.acquisition_defending()
            || self.capture_escaping()
            || self.disengaging()
            || self.recovery.is_some()
            || self.escaping_sun
            || !o.match_rules
            || !p.controls_armed
            || !p.queries_ready
            || !c.recovery.flight.flight.enabled
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.landing.phase != LandingPhase::Flying
            || p.landing.supported_feet != 0
            || self.telemetry.target != Some(p.planet.index)
            || c.weapons.last_hit_taken_tick != Some(p.tick)
            || !matches!(c.weapons.last_hit_source, Some("laser" | "cannon"))
        {
            return native;
        }
        let Some(task) = self.capture.as_ref().filter(|t| t.awaiting_first_site()) else {
            return native;
        };
        let capture = task.telemetry();
        if !capture.acquisition.is_some_and(|a| {
            a.tick == p.tick
                && a.planet == p.planet.index
                && a.revision == p.planet.revision
                && a.selected_site.is_none()
                && a.survey_rejected_by.is_none()
                && matches!(a.reason, "scan_deferred" | "candidates_rejected")
        }) || capture.started_tick.is_none_or(|tick| tick > p.tick)
            || capture.landing.landed_tick.is_some()
            || capture.landing.claimed_tick.is_some()
            || capture.landing.boarded_tick.is_some()
            || capture.objective_route.is_some()
        {
            return native;
        }
        let Some(target) = c.target.filter(|t| {
            t.visible
                && !t.ground_occluded
                && t.owner != p.owner
                && t.ship_form == Some(ShipForm::Ship)
                && t.health > 0.0
                && t.motion.position.distance_to(p.ship.position) < 300.0
        }) else {
            return native;
        };
        let capture = capture.clone();
        let (direction, estimated_min_range, estimated_clearance) =
            disengagement::escape_direction(o, true);
        if let Some(gate) = &mut self.telemetry.acquisition_clearance {
            let decision = clearance_decision(estimated_clearance);
            gate.checks += 1;
            gate.last = Some(AcquisitionClearanceCheck {
                tick: p.tick,
                decision,
                planet: p.planet.index,
                vehicle: p.vehicle,
                opponent: target.owner,
                hit_source: c.weapons.last_hit_source.unwrap(),
                capture: capture.clone(),
                native_actions: native.encode(p.owner),
                direction,
                estimated_min_range,
                estimated_clearance,
            });
            if decision != "admitted" {
                gate.rejected += 1;
                // Keep the native task and intent. A rejected proposal starts
                // neither the escape clock nor the source-planet cooldown.
                return native;
            }
        }
        let deadline_tick = p.tick + DISENGAGEMENT_TICKS;
        let state = self.telemetry.acquisition_defense.as_mut().unwrap();
        state.attempts += 1;
        state.last = Some(AcquisitionDefenseAttempt {
            started_tick: p.tick,
            deadline_tick,
            observed_tick: p.tick,
            finished_tick: None,
            reason: None,
            guidance: "arming",
            planet: p.planet.index,
            vehicle: p.vehicle,
            opponent: target.owner,
            hit_source: c.weapons.last_hit_source.unwrap(),
            capture,
            native_actions: native.encode(p.owner),
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
        self.end_pursuit(p.tick, "defending exposed acquisition");
        self.reconsider(p.tick, "fresh weapon hit before first landing site", true);
        self.next_pursuit_tick = self.next_pursuit_tick.max(deadline_tick);
        self.event(
            p.tick,
            "acquisition_defense_started",
            Some("fresh weapon hit before first landing site"),
        );
        self.acquisition_defense_intent(o).unwrap()
    }

    fn end_acquisition_defense(&mut self, tick: u64, reason: &'static str) {
        let state = self.telemetry.acquisition_defense.as_mut().unwrap();
        let a = state.last.as_mut().unwrap();
        a.finished_tick = Some(tick);
        a.reason = Some(reason);
        a.guidance = "finished";
        state.separated += u32::from(reason == "separation established");
        state.timed_out += u32::from(reason == "defense deadline");
        self.event(tick, "acquisition_defense_ended", Some(reason));
    }

    /// Runs before safety/disabled-control returns, so they cannot renew the clock.
    pub(super) fn update_acquisition_defense(&mut self, o: &MissionObservationV1) {
        if !self.acquisition_defending() {
            return;
        }
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let a = self
            .telemetry
            .acquisition_defense
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
            self.end_acquisition_defense(p.tick, "recovery required");
            return;
        }
        if !o.match_rules
            || self.capture.is_some()
            || p.landing.supported_feet > 0
            || p.landing.phase == LandingPhase::Landed
        {
            self.end_acquisition_defense(p.tick, "surface task takes priority");
            return;
        }
        if p.tick >= a.deadline_tick {
            self.end_acquisition_defense(p.tick, "defense deadline");
            return;
        }
        let Some(planet) = o.planets.iter().find(|v| v.index == a.planet) else {
            self.end_acquisition_defense(p.tick, "source planet unavailable");
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
            .is_some_and(|tick| p.tick.saturating_sub(tick) >= CLEAR_TICKS)
        {
            self.end_acquisition_defense(p.tick, "separation established");
        }
    }

    pub(super) fn acquisition_defense_intent(
        &mut self,
        o: &MissionObservationV1,
    ) -> Option<CombatIntent> {
        if !self.acquisition_defending() {
            return None;
        }
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        if !c.recovery.flight.flight.enabled || !p.queries_ready {
            self.telemetry
                .acquisition_defense
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
            .acquisition_defense
            .as_ref()
            .unwrap()
            .last
            .as_ref()
            .unwrap()
            .direction;
        let radial = p.ship.position - p.planet.motion.position;
        let falling =
            (-(p.ship.velocity - p.planet.motion.velocity).dot(radial.normalized())).max(0.0);
        let mut intent = self.escape_flight(o, direction);
        let a = self
            .telemetry
            .acquisition_defense
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        a.guidance = if a.boundary.active {
            "boundary"
        } else if radial.length() - p.planet.radius < 70.0 + falling * falling / 50.0 {
            "climb"
        } else {
            "escape"
        };
        a.controlled_ticks += 1;
        // Weapons use the existing controller including scheduled breaks. Its
        // flight action never replaces the escape/ground/boundary guidance.
        intent.weapons = self.patrol.intent(c).weapons;
        self.telemetry.combat = Some(self.patrol.telemetry().clone());
        a.laser_ticks += u64::from(intent.weapons.laser);
        a.cannon_ticks += u64::from(intent.weapons.cannon);
        Some(intent)
    }
}

#[cfg(test)]
#[path = "mission_acquisition_defense_tests.rs"]
mod tests;
