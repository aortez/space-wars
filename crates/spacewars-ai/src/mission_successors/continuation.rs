//! Explicit headless intervention, never selected by a production policy or by
//! a forecast score. The shared mission entry point retains recovery, solar,
//! observation-version and repeated-tick guards. Surface permissions stay live.
use super::*;
use scenario_spacewars::surface_sortie::LandingPhase;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContinuationReport {
    pub source_tick: u64,
    pub successor: Successor,
    pub proposal: SuccessorOption,
    /// Includes the already-emitted common handoff command. Escape cannot
    /// extend its original attempt's deadline, even for this experiment.
    pub deadline_tick: u64,
    pub first_control_tick: Option<u64>,
    pub applied_controls: u64,
    pub stopped_tick: Option<u64>,
    pub stop_reason: Option<&'static str>,
}

#[derive(Clone)]
pub struct SuccessorContinuation {
    report: ContinuationReport,
    context: BrainReset,
    direction: Vec2,
    last_tick: u64,
}

impl SuccessorContinuation {
    /// Pins an explicitly chosen proposal just after the handoff command was
    /// emitted. Does not run the comparison job, replace that command, inspect
    /// future state, or claim that the sampled landing remains usable.
    pub fn new(
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        successor: Successor,
    ) -> Result<Self, &'static str> {
        let job = bot
            .successor_comparison(o)
            .ok_or("not a current successful escape handoff")?;
        let proposal = job
            .report
            .options
            .into_iter()
            .find(|option| option.successor == successor)
            .ok_or("successor was not proposed at this handoff")?;
        if let Some(reason) = proposal.not_evaluated {
            return Err(reason);
        }
        let source_tick = job.report.source_tick;
        let deadline_tick = if successor == Successor::ContinueEscape {
            (source_tick + FORECAST_TICKS).min(job.report.escape_deadline_tick)
        } else {
            source_tick + FORECAST_TICKS
        };
        Ok(Self {
            report: ContinuationReport {
                source_tick,
                successor,
                proposal,
                deadline_tick,
                first_control_tick: None,
                applied_controls: 0,
                stopped_tick: None,
                stop_reason: None,
            },
            context: bot.context,
            direction: bot
                .telemetry
                .disengagement
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .direction,
            last_tick: source_tick,
        })
    }

    pub fn report(&self) -> &ContinuationReport {
        &self.report
    }

    /// Call instead of the mission's ordinary intent only for this explicit
    /// trial. Once finished, the same entry point resumes ordinary decisions.
    pub fn intent(
        &mut self,
        bot: &mut MaterialMissionPilot,
        o: &MissionObservationV1,
    ) -> CombatIntent {
        bot.intent_with_continuation(o, Some(self))
    }

    pub fn approach_distance(&self, o: &MissionObservationV1) -> Option<f32> {
        let approach = self.report.proposal.approach?;
        o.planets.iter().find(|p| p.index == approach.site.planet)?;
        Some(
            o.local
                .combat
                .recovery
                .flight
                .pilot
                .ship
                .position
                .distance_to(approach.position(o)),
        )
    }

    pub(crate) fn stop(&mut self, bot: &mut MaterialMissionPilot, tick: u64, reason: &'static str) {
        if self.report.stopped_tick.is_none() {
            self.report.stopped_tick = Some(tick);
            self.report.stop_reason = Some(reason);
            // Let the normal coordinator choose again, with the motor state
            // actually reached and no retained right to land at the proposal.
            if self.report.first_control_tick.is_some()
                && bot.context == self.context
                && bot.previous_tick == Some(self.last_tick)
            {
                bot.reconsider(tick, "successor experiment ended", false);
            }
        }
    }

    pub(crate) fn flight_intent(
        &mut self,
        bot: &mut MaterialMissionPilot,
        o: &MissionObservationV1,
    ) -> Option<CombatIntent> {
        if self.report.stopped_tick.is_some() {
            return None;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let reason = if bot.context != self.context
            || bot.previous_tick != Some(self.last_tick)
            || p.tick != self.last_tick + 1
        {
            Some("controller reset or nonconsecutive observation")
        } else if bot.recovery.is_some()
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !matches!(p.location, PilotLocation::Aboard(_))
        {
            Some("recovery required")
        } else if !p.controls_armed {
            Some("controls unavailable")
        } else if bot.capture.is_some()
            || p.landing.phase != LandingPhase::Flying
            || p.landing.supported_feet > 0
        {
            Some("surface task or contact")
        } else if p.tick >= self.report.deadline_tick {
            Some(if self.report.successor == Successor::ContinueEscape {
                "escape deadline or trial horizon"
            } else {
                "trial horizon"
            })
        } else if let Some(approach) = self.report.proposal.approach {
            let revision = self
                .report
                .proposal
                .evidence
                .as_ref()
                .unwrap()
                .measurement
                .as_ref()
                .unwrap()
                .revision;
            match o
                .planets
                .iter()
                .find(|planet| planet.index == approach.site.planet)
            {
                None => Some("destination absent"),
                Some(planet) if planet.revision != revision => Some("destination material changed"),
                Some(planet)
                    if planet
                        .claim
                        .as_ref()
                        .is_some_and(|c| c.owner == Some(p.owner)) =>
                {
                    Some("destination now owned")
                }
                Some(planet)
                    if self.approach_distance(o).unwrap() < 10.0
                        && (p.ship.velocity - planet.velocity_at(approach.position(o)))
                            .length()
                            < 18.0 =>
                {
                    Some("approach reached; fresh surface planning required")
                }
                _ => None,
            }
        } else if o
            .local
            .combat
            .target
            .is_none_or(|target| target.health <= 0.0 || target.ship_form != Some(ShipForm::Ship))
        {
            Some("opponent no longer an armed ship")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.stop(bot, p.tick, reason);
            return None;
        }
        if self.report.first_control_tick.is_none() {
            bot.prepare_handoff_transfer(
                self.report.source_tick,
                self.report.proposal.approach.map_or(0, |a| a.site.planet),
            );
            bot.event(p.tick, "successor_experiment_started", None);
            self.report.first_control_tick = Some(p.tick);
        }
        self.last_tick = p.tick;
        self.report.applied_controls += 1;
        Some(match self.report.successor {
            Successor::Transfer { .. } => {
                bot.site_approach_intent(o, self.report.proposal.approach.unwrap())
            }
            Successor::ContinueEscape => bot.escape_flight(o, self.direction),
            // Actual visibility, supply, recoil, hits and enemy decisions remain
            // physical. The forecast's clear-sight hypothesis is not copied.
            Successor::Combat => bot.hunt(o),
        })
    }
}
