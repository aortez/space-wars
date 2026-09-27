//! One explicit destination nomination in a deterministic replay. This is an
//! offline intervention, not a policy recommendation or a planner-budget job.
use engine_core::planning::{PlanningJob, WorkKind};
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        PilotLocation, SurfaceSortieState,
        mission::MissionObservationV1,
        pilot::{LANDING_SITE_COUNT, LandingSiteId},
    },
};
use serde_json::{Value, json};
use spacewars_ai::{
    combat_pilot::CombatIntent,
    mission_evaluation::{MissionEvaluator, TransferSource},
    mission_pilot::MissionGoal,
    mission_policy::MissionBot,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub struct TransferProbeRun {
    seat: usize,
    tick: u64,
    destination: usize,
    source: Option<Value>,
    outcome: Option<Value>,
    trace: BufWriter<File>,
    defer_pursuit: bool,
    control_comparison: Option<Value>,
    forecast_enabled: bool,
    forecast: Option<Value>,
    schedule: Option<super::transfer_schedule::TransferScheduleRun>,
    acquisition: Option<super::acquisition_probe::AcquisitionProbe>,
    landing_reference: Option<LandingSiteId>,
    landing_choice: Option<Value>,
}

impl TransferProbeRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let destination = super::arg("--probe-transfer-destination", "none");
        let acquisition = match super::arg("--probe-acquisition-seconds", "none").as_str() {
            "none" => None,
            seconds => Some(super::acquisition_probe::AcquisitionProbe::new(
                seconds
                    .parse()
                    .expect("acquisition seconds must be an integer"),
            )),
        };
        let landing_reference =
            match super::arg("--probe-landing-reference-bearing", "none").as_str() {
                "none" => None,
                bearing => {
                    assert!(
                        acquisition.is_some(),
                        "landing comparison needs acquisition continuation"
                    );
                    let bearing = bearing
                        .parse()
                        .expect("reference bearing must be an integer");
                    assert!(bearing < LANDING_SITE_COUNT);
                    Some(LandingSiteId {
                        planet: destination
                            .parse()
                            .expect("landing comparison needs a destination"),
                        bearing,
                    })
                }
            };
        let schedule = super::transfer_schedule::TransferScheduleRun::from_args(out);
        let forecast_enabled = match super::arg("--forecast-transfer", "false").as_str() {
            "true" => true,
            "false" => false,
            _ => panic!("--forecast-transfer must be true or false"),
        };
        if destination == "none" {
            assert!(
                !forecast_enabled && schedule.is_none() && acquisition.is_none(),
                "transfer forecast needs a source nomination"
            );
            assert_eq!(super::arg("--probe-transfer-tick", "none"), "none");
            assert_eq!(super::arg("--probe-transfer-seat", "none"), "none");
            assert_eq!(
                super::arg("--probe-transfer-pursuit", "ordinary"),
                "ordinary"
            );
            return None;
        }
        let seat: usize = super::arg("--probe-transfer-seat", "0").parse().unwrap();
        let tick: u64 = super::arg("--probe-transfer-tick", "none")
            .parse()
            .expect("probe needs source tick");
        let seconds: u64 = super::arg("--seconds", "180").parse().unwrap();
        let acquisition_ticks = acquisition.as_ref().map_or(0, |a| a.horizon_ticks);
        assert!(
            seat < 2 && tick + 3600 + acquisition_ticks < seconds * 60,
            "probe needs full transfer and optional acquisition horizons"
        );
        if acquisition.is_some() {
            assert_eq!(super::arg("--trace", "false"), "true");
            assert_eq!(super::arg("--trace-start-tick", "0"), "0");
            assert!(
                super::arg("--trace-end-tick", "0").parse::<u64>().unwrap()
                    > tick + 3600 + acquisition_ticks
            );
            assert!(
                schedule.is_none(),
                "acquisition continuation must not extend a transfer forecast schedule"
            );
        }
        assert_eq!(super::arg("--mode", "quiet"), "duel");
        assert_eq!(super::arg("--match", "false"), "true");
        assert_eq!(super::arg("--evaluate-missions", "false"), "true");
        assert_eq!(super::arg("--continue-successor", "none"), "none");
        assert_eq!(super::arg("--require-finish", "false"), "false");
        let defer_pursuit = match super::arg("--probe-transfer-pursuit", "ordinary").as_str() {
            "ordinary" => false,
            "defer_new" => true,
            _ => panic!("--probe-transfer-pursuit must be ordinary or defer_new"),
        };
        Some(Self {
            seat,
            tick,
            destination: destination.parse().unwrap(),
            source: None,
            outcome: None,
            trace: BufWriter::new(File::create(out.join("transfer-probe.jsonl")).unwrap()),
            defer_pursuit,
            control_comparison: None,
            forecast_enabled,
            forecast: None,
            schedule,
            acquisition,
            landing_reference,
            landing_choice: None,
        })
    }

    pub fn intent(
        &mut self,
        seat: usize,
        bot: &mut MissionBot,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
    ) -> Option<CombatIntent> {
        let p = &o.local.combat.recovery.flight.pilot;
        if seat != self.seat {
            return None;
        }
        if self.defer_pursuit && self.source.is_some() && !self.done() && p.tick > self.tick {
            // Same-state comparison only: this clone does not evolve a second
            // world and never supplies controls to the physics step.
            let mut ordinary = bot.clone();
            let normal = ordinary.intent_with_evaluation(o, evaluator);
            let intent =
                bot.intent_for_transfer_calibration(o, evaluator, self.destination, self.tick);
            let t = ordinary.telemetry();
            self.control_comparison = Some(json!({
                "ordinary_new_pursuit":t.pursuit.as_ref().is_some_and(|pursuit| pursuit.started_tick == p.tick),
                "deferred_new_pursuit":t.pursuit.as_ref().is_some_and(|pursuit| pursuit.started_tick == p.tick)
                    && bot.telemetry().pursuit.is_none(),
                "ordinary_goal":t.goal,"ordinary_target":t.target,"ordinary_actions":normal.encode(p.owner),
                "ordinary_pursuit":t.pursuit,"same_intent":normal == intent,
                "same_mission":ordinary.telemetry() == bot.telemetry(),
            }));
            return Some(intent);
        }
        if p.tick != self.tick || self.source.is_some() {
            return None;
        }
        // Capture before intent: the pilot's same-tick command cache must not
        // have consumed this source. The runner audits the exact frozen prefix.
        let source = TransferSource::from_observation(o);
        let before = bot.telemetry().clone();
        let diagnostic = source.diagnose(self.destination);
        let (intent, nomination) =
            bot.intent_with_destination_probe(o, evaluator, self.destination);
        self.source = Some(
            json!({"tick":p.tick,"transfer_source":source,"mission_before":before,
                                 "diagnostic":diagnostic,"nomination":nomination}),
        );
        if !nomination.accepted {
            self.stop(p.tick, "nomination_refused");
        }
        Some(intent)
    }

    pub fn record(
        &mut self,
        seat: usize,
        bot: &MissionBot,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        intent: CombatIntent,
    ) {
        if seat != self.seat || self.source.is_none() {
            return;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let t = bot.telemetry();
        if self.acquisition.is_some()
            && self
                .outcome
                .as_ref()
                .is_some_and(|r| r["tick"].as_u64().unwrap() < p.tick)
        {
            // Transfer telemetry and pursuit deferral end at the original
            // handoff. The caller now supplies its ordinary controller intent.
            if self.outcome.as_ref().unwrap()["reason"] == "arrived" {
                self.acquisition
                    .as_mut()
                    .unwrap()
                    .observe(self.destination, t, o);
                self.compare_landing_choice(bot, o);
            }
            return;
        }
        if self.forecast_enabled && p.tick == self.tick && self.forecast.is_none() {
            let started = Instant::now();
            let environment = if self.source.as_ref().unwrap()["nomination"]["accepted"] == true {
                state.transfer_environment()
            } else {
                Err("source nomination refused")
            };
            let source_environment = environment.as_ref().ok().cloned();
            let result = environment
                .and_then(|environment| bot.forecast_nominated_transfer(o, environment, 3600));
            let construction_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            self.forecast = Some(match result {
                Ok(mut job) => {
                    let mut charged = 0_u64;
                    while let Some(work) = job.next_work() {
                        assert_eq!(work, WorkKind::Graph);
                        job.step();
                        charged += 1;
                    }
                    assert_eq!(charged, job.output().unwrap().charged_graph);
                    json!({"environment":source_environment,"source_actions":intent.encode(p.owner),
                        "report":job.output(),"unknown":null,"construction_ms":construction_ms,
                        "prediction_ms":started.elapsed().as_secs_f64()*1000.0,
                        "charged_graph":charged,"physics_queries":0})
                }
                Err(reason) => {
                    json!({"environment":source_environment,"report":null,"unknown":reason,
                    "source_actions":intent.encode(p.owner),"construction_ms":construction_ms,
                    "prediction_ms":started.elapsed().as_secs_f64()*1000.0,"charged_graph":0,"physics_queries":0})
                }
            });
        }
        let contacts = state.landing_diagnostics(seat, None);
        let damage = state.damage_observation(seat);
        let solver_contact = contacts["hull"]["count"].as_u64().unwrap_or(0) > 0
            || contacts["feet"]
                .as_array()
                .is_some_and(|feet| feet.iter().any(|f| f["count"].as_u64().unwrap_or(0) > 0));
        let debris_contact = damage
            .last_contact_tick
            .is_some_and(|tick| tick > self.tick);
        if let Some(schedule) = &mut self.schedule {
            schedule.observe(
                bot,
                state,
                o,
                intent,
                p.tick == self.tick,
                contacts
                    .is_object()
                    .then_some(solver_contact || debris_contact),
            );
        }
        let arrived = t
            .events
            .iter()
            .any(|e| e.tick == p.tick && e.kind == "arrived" && e.planet == Some(self.destination));
        if !self.done() {
            // Interruption takes precedence over same-tick arrival. An arrival
            // is the real coordinator handoff, including its queries_ready gate.
            let outcome = if !p.ship_available
                || p.ship_health <= 0.0
                || p.ship_form != ShipForm::Ship
                || p.location == PilotLocation::OnFoot
            {
                Some("ship_or_pilot_lost")
            } else if t.goal == MissionGoal::Recover || t.recovery.is_some() {
                Some("recovery")
            } else if solver_contact || debris_contact {
                Some("solver_or_debris_contact")
            } else if arrived {
                Some("arrived")
            } else if t.target != Some(self.destination) {
                Some("retargeted")
            } else if p.tick >= self.tick + 3600 {
                Some("timeout")
            } else {
                None
            };
            if let Some(reason) = outcome {
                self.stop(p.tick, reason);
                if reason == "arrived"
                    && let Some(acquisition) = &mut self.acquisition
                {
                    acquisition.start(p.tick);
                    acquisition.observe(self.destination, t, o);
                }
            }
        }
        self.compare_landing_choice(bot, o);
        serde_json::to_writer(&mut self.trace, &json!({
            "tick":p.tick,"ship":p.ship,"frame":p.planet.index,"target":t.target,"goal":t.goal,
            "queries_ready":p.queries_ready,"ship_available":p.ship_available,
            "recovery_active":t.goal == MissionGoal::Recover || t.recovery.is_some(),
            "match_finished":state.match_outcome().is_some(),"health":p.ship_health,"form":p.ship_form,"location":p.location,
            "planets":o.planets.iter().map(|planet| json!({"index":planet.index,"motion":planet.motion,"radius":planet.radius})).collect::<Vec<_>>(),
            "avoidance":t.avoidance,"reason":t.reason,"arrived":arrived,
            "solver_contact":solver_contact,"debris_contact":debris_contact,"damage":damage,
            "solver_contacts":contacts,"next_actions":intent.encode(p.owner),
            "terminal":self.outcome,
            "pursuit_control":self.control_comparison.take(),
        })).unwrap();
        writeln!(self.trace).unwrap();
    }

    fn compare_landing_choice(&mut self, bot: &MissionBot, o: &MissionObservationV1) {
        let Some(reference) = self.landing_reference else {
            return;
        };
        let p = &o.local.combat.recovery.flight.pilot;
        if self.landing_choice.is_some()
            || !self
                .acquisition
                .as_ref()
                .is_some_and(|a| a.selected_now(p.tick))
        {
            return;
        }
        // The caller passes the exact immutable observation used for this
        // tick's intent and dense trace. No world queries or controller updates.
        let started = Instant::now();
        let result = bot.landing_choice_comparison(o, reference);
        let assessment_ms = started.elapsed().as_secs_f64() * 1000.0;
        let (report, unknown) = match result {
            Ok(report) => (Some(report), None),
            Err(reason) => (None, Some(reason)),
        };
        self.landing_choice = Some(json!({
            "reference":reference,"observation_tick":p.tick,"actor":p.owner,
            "report":report,"unknown":unknown,"assessment_ms":assessment_ms,"physics_queries":0,
        }));
    }

    fn stop(&mut self, tick: u64, reason: &'static str) {
        self.outcome = Some(
            json!({"tick":tick,"reason":reason,"elapsed_ticks":tick.saturating_sub(self.tick)}),
        );
    }

    pub fn seat(&self) -> usize {
        self.seat
    }

    pub fn done(&self) -> bool {
        self.outcome.is_some()
    }

    pub fn run_done(&self) -> bool {
        self.done()
            && self
                .acquisition
                .as_ref()
                .is_none_or(|a| self.outcome.as_ref().unwrap()["reason"] != "arrived" || a.done())
    }

    pub fn advance(&mut self, tick: u64, remaining: engine_core::planning::Work) -> f64 {
        self.schedule
            .as_mut()
            .map_or(0.0, |s| s.advance(tick, remaining))
    }

    pub fn finish(&mut self, state: &SurfaceSortieState, bot: &MissionBot) -> Value {
        if !self.done() {
            self.stop(
                state.tick(),
                if state.match_outcome().is_some() {
                    "match_finished"
                } else {
                    "runner_ended"
                },
            );
        }
        if self.source.is_some()
            && self
                .outcome
                .as_ref()
                .is_some_and(|r| r["reason"] == "match_finished" || r["reason"] == "runner_ended")
        {
            let o = state.mission_observation_with_cadence(
                self.seat,
                bot.sensor_request(),
                scenario_spacewars::surface_sortie::mission::LandingSurveyCadence::FourHz,
            );
            // Preserve the original transfer endpoint row on a censored run.
            // Acquisition never inspects this synthetic finish observation.
            let acquisition = self.acquisition.take();
            self.record(self.seat, bot, state, &o, CombatIntent::default());
            self.acquisition = acquisition;
        }
        self.trace.flush().unwrap();
        let mut report = json!({"schema":1,"seat":self.seat,"source_tick":self.tick,"destination":self.destination,
            "source":self.source,"outcome":self.outcome,"horizon_ticks":3600,
            "pursuit_policy":if self.defer_pursuit {"defer_new"} else {"ordinary"},
            "forecast":self.forecast,
            "scope":"Solver contacts include positive separation and do not prove impact. External destination nomination retains controller gates. Optional defer_new suppresses new pursuit only during the nominated transfer after its source tick; existing pursuit, recovery and safety retain priority. Source is pre-intent; terminal next_actions are not executed. Measures handoff, not landing/capture or match strength. Same-state ordinary-control comparisons are not independent physical trajectories. Diagnostic work and IO are outside live planner fuel."});
        if let Some(schedule) = &mut self.schedule {
            report["forecast_schedule"] = schedule.finish(state.tick());
        }
        if let Some(acquisition) = &mut self.acquisition {
            report["acquisition"] =
                acquisition.finish(state.tick(), state.match_outcome().is_some());
            report["scope"] = json!(
                "Transfer trace ends at its original handoff or interruption. With acquisition continuation, the handoff command is executed unless acquisition also terminates on that tick; the final acquisition command is not executed. New pursuit is deferred only before handoff. All later controller intents are ordinary. Acquisition observes first site choice, not landing/capture or match strength; historical reference sites are never forced. Diagnostic work and IO are outside live planner fuel."
            );
        }
        if let Some(reference) = self.landing_reference {
            report["landing_choice"] = self.landing_choice.clone().unwrap_or_else(|| {
                json!({
                    "reference":reference,"observation_tick":null,"actor":null,"report":null,
                    "unknown":"no observed site choice","assessment_ms":0.0,"physics_queries":0,
                })
            });
        }
        report
    }
}
