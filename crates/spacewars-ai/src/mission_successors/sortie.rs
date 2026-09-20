//! Bounded physical experiment for a complete nominated-site opportunity.
//! Historical forecast tails confer no permission; the local capture task must
//! obtain current landing, objective-route, footing and boarding observations.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SortieContinuationReport {
    pub approach_deadline_tick: u64,
    pub best_approach_distance: f32,
    pub progress_tick: u64,
    pub arrived_tick: Option<u64>,
    pub surface_started_tick: Option<u64>,
    pub selected_site: Option<LandingSiteId>,
    pub landed_tick: Option<u64>,
    pub landing_position_error: Option<f32>,
    pub exited_tick: Option<u64>,
    pub claimed_tick: Option<u64>,
    pub boarded_tick: Option<u64>,
    pub departed_tick: Option<u64>,
    pub surface_failure: Option<&'static str>,
    pub ownership_retained: Option<bool>,
}

#[cfg(test)]
#[path = "sortie_tests.rs"]
mod tests;

impl SuccessorContinuation {
    pub const MAX_SORTIE_TICKS: u64 = 180 * 60;

    /// The same explicit source proposal, with a sixty-second approach and a
    /// three-minute total clock. Both include the already-emitted common tick.
    pub fn for_capture_trip(
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        site: LandingSiteId,
    ) -> Result<Self, &'static str> {
        let mut trial = Self::new(bot, o, Successor::Transfer { site })?;
        let source = trial.report.source_tick;
        trial.report.deadline_tick = source + Self::MAX_SORTIE_TICKS;
        trial.report.sortie = Some(SortieContinuationReport {
            approach_deadline_tick: source + 60 * 60,
            best_approach_distance: trial.approach_distance(o).unwrap(),
            progress_tick: source,
            arrived_tick: None,
            surface_started_tick: None,
            selected_site: None,
            landed_tick: None,
            landing_position_error: None,
            exited_tick: None,
            claimed_tick: None,
            boarded_tick: None,
            departed_tick: None,
            surface_failure: None,
            ownership_retained: None,
        });
        Ok(trial)
    }

    pub(super) fn sortie_intent(
        &mut self,
        bot: &mut MaterialMissionPilot,
        o: &MissionObservationV1,
    ) -> Option<CombatIntent> {
        let p = &o.local.combat.recovery.flight.pilot;
        let approach = self.report.proposal.approach.unwrap();
        let trip = self.report.sortie.as_ref().unwrap();
        let surface = trip.surface_started_tick.is_some();
        let planet = o.planets.iter().find(|p| p.index == approach.site.planet);
        let reason = if bot.context != self.context
            || bot.previous_tick != Some(self.last_tick)
            || p.tick != self.last_tick + 1
        {
            Some("controller reset or nonconsecutive observation")
        } else if bot.recovery.is_some() || !p.ship_available || p.ship_form != ShipForm::Ship {
            Some("recovery required")
        } else if p.tick >= self.report.deadline_tick {
            Some("capture trip deadline")
        } else if planet.is_none() {
            Some("destination absent")
        } else if surface {
            // Normal capture owns on-foot controls, neutral transfer releases,
            // material invalidation and recovery. Do not extend old air-only
            // gates or the source revision across its freshly measured legs.
            if bot.capture.is_none() || bot.telemetry.target != Some(approach.site.planet) {
                Some("capture task ended without recorded departure")
            } else {
                None
            }
        } else if !matches!(p.location, PilotLocation::Aboard(_)) || !p.controls_armed {
            Some("controls unavailable during approach")
        } else if bot.capture.is_some()
            || p.landing.phase != LandingPhase::Flying
            || p.landing.supported_feet > 0
        {
            Some("surface task or contact before approach arrival")
        } else if p.tick >= trip.approach_deadline_tick {
            Some("approach deadline")
        } else if planet.unwrap().revision
            != self
                .report
                .proposal
                .evidence
                .as_ref()
                .unwrap()
                .measurement
                .as_ref()
                .unwrap()
                .revision
        {
            Some("destination material changed")
        } else if planet
            .unwrap()
            .claim
            .as_ref()
            .is_some_and(|c| c.owner == Some(p.owner))
        {
            Some("destination now owned")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.stop(bot, p.tick, reason);
            return None;
        }
        if self.report.first_control_tick.is_none() {
            bot.prepare_handoff_transfer(self.report.source_tick, approach.site.planet);
            bot.event(p.tick, "successor_sortie_started", None);
            self.report.first_control_tick = Some(p.tick);
        }
        if !surface {
            let distance = self.approach_distance(o).unwrap();
            let trip = self.report.sortie.as_mut().unwrap();
            if distance < trip.best_approach_distance - 2.0 {
                trip.best_approach_distance = distance;
                trip.progress_tick = p.tick;
            }
            if p.tick.saturating_sub(trip.progress_tick) >= 20 * 60 {
                self.stop(
                    bot,
                    p.tick,
                    "approach made no distance progress for twenty seconds",
                );
                return None;
            }
            if distance < 10.0
                && (p.ship.velocity - planet.unwrap().velocity_at(approach.position(o))).length()
                    < 18.0
            {
                trip.arrived_tick.get_or_insert(p.tick);
                if p.planet.index == approach.site.planet && p.queries_ready {
                    // This creates a task, not a landing permission. The next
                    // sensor request surveys actual sites and current routes.
                    bot.capture = Some(
                        TacticalCapturePilot::with_planning(
                            bot.context,
                            bot.breaks,
                            bot.policy.objective_planning(),
                        )
                        .requiring_site(approach.site),
                    );
                    bot.solar_detour = None;
                    bot.goal(MissionGoal::Capture, p.tick);
                    bot.event(p.tick, "arrived", Some("explicit successor site trial"));
                    trip.surface_started_tick = Some(p.tick);
                    self.last_tick = p.tick;
                    self.report.applied_controls += 1;
                    return Some(CombatIntent::default());
                }
            }
        }
        self.last_tick = p.tick;
        self.report.applied_controls += 1;
        if surface {
            // The shared mission coordinator retains departure across approach
            // frames and all existing capture/recovery failure handling.
            None
        } else {
            Some(bot.site_approach_intent(o, approach))
        }
    }

    /// Collect milestones only after the shared coordinator has consumed this
    /// tick's real observation, before any future world step. Later normal
    /// captures after a stopped trial cannot be credited to the experiment.
    pub(crate) fn record_sortie(
        &mut self,
        bot: &mut MaterialMissionPilot,
        o: &MissionObservationV1,
    ) {
        if self.report.stopped_tick.is_some() {
            return;
        }
        let Some(trip) = &mut self.report.sortie else {
            return;
        };
        if trip.surface_started_tick.is_none() {
            return;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let site = self.report.proposal.approach.unwrap().site;
        if let Some(capture) = &bot.capture {
            let t = capture.telemetry();
            trip.selected_site = t.site.or(trip.selected_site);
            trip.landed_tick = trip.landed_tick.or(t.landing.landed_tick);
            trip.claimed_tick = trip.claimed_tick.or(t.landing.claimed_tick);
            trip.boarded_tick = trip.boarded_tick.or(t.landing.boarded_tick);
            if p.location == PilotLocation::OnFoot {
                trip.exited_tick.get_or_insert(p.tick);
            }
            if trip.landing_position_error.is_none() && t.landing.landed_tick.is_some() {
                trip.landing_position_error = p
                    .sites
                    .iter()
                    .find(|s| s.id == site)
                    .map(|s| p.ship.position.distance_to(s.vehicle_position));
            }
            trip.surface_failure = t.failure;
            if let Some(reason) = t.failure {
                self.stop(bot, p.tick, reason);
            }
        } else {
            let departed = bot.telemetry.events.last().is_some_and(|e| {
                e.tick == p.tick && e.kind == "departed" && e.planet == Some(site.planet)
            });
            if departed && trip.claimed_tick.is_some() && trip.boarded_tick.is_some() {
                trip.departed_tick = Some(p.tick);
                let retained = o
                    .planets
                    .iter()
                    .find(|p| p.index == site.planet)
                    .and_then(|p| p.claim.as_ref())
                    .is_some_and(|c| c.owner == Some(p.owner));
                trip.ownership_retained = Some(retained);
                // The coordinator has already cleared the completed task. Do
                // not start a second replan while recording that same event.
                self.report.stopped_tick = Some(p.tick);
                self.report.stop_reason = Some(if retained {
                    "capture trip completed"
                } else {
                    "departed after claim and return but ownership was lost"
                });
            } else {
                let reason = bot
                    .telemetry
                    .events
                    .last()
                    .filter(|e| e.tick == p.tick)
                    .and_then(|e| e.reason)
                    .unwrap_or("capture task ended without recorded departure");
                trip.surface_failure = Some(reason);
                self.report.stopped_tick = Some(p.tick);
                self.report.stop_reason = Some(reason);
            }
        }
    }
}
