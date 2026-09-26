//! One explicit destination nomination in a deterministic replay. This is an
//! offline intervention, not a policy recommendation or a planner-budget job.
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{PilotLocation, SurfaceSortieState, mission::MissionObservationV1},
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
}

impl TransferProbeRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let destination = super::arg("--probe-transfer-destination", "none");
        if destination == "none" {
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
        assert!(
            seat < 2 && tick + 3600 < seconds * 60,
            "probe needs full 60-second horizon"
        );
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
        let contacts = state.landing_diagnostics(seat, None);
        let damage = state.damage_observation(seat);
        let solver_contact = contacts["hull"]["count"].as_u64().unwrap_or(0) > 0
            || contacts["feet"]
                .as_array()
                .is_some_and(|feet| feet.iter().any(|f| f["count"].as_u64().unwrap_or(0) > 0));
        let debris_contact = damage
            .last_contact_tick
            .is_some_and(|tick| tick > self.tick);
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
            }
        }
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
            self.record(self.seat, bot, state, &o, CombatIntent::default());
        }
        self.trace.flush().unwrap();
        json!({"schema":1,"seat":self.seat,"source_tick":self.tick,"destination":self.destination,
            "source":self.source,"outcome":self.outcome,"horizon_ticks":3600,
            "pursuit_policy":if self.defer_pursuit {"defer_new"} else {"ordinary"},
            "scope":"Solver contacts include positive separation and do not prove impact. External destination nomination retains controller gates. Optional defer_new suppresses new pursuit only during the nominated transfer after its source tick; existing pursuit, recovery and safety retain priority. Source is pre-intent; terminal next_actions are not executed. Measures handoff, not landing/capture or match strength. Same-state ordinary-control comparisons are not independent physical trajectories. Diagnostic work and IO are outside live planner fuel."})
    }
}
