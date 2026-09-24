//! Replay to an exact handoff, apply one explicitly chosen short intervention,
//! then let the original mission policies finish the physical match.
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieState, mission::MissionObservationV1, pilot::LandingSiteId},
};
use serde_json::{Value, json};
use spacewars_ai::{
    combat_pilot::CombatIntent,
    mission_pilot::{MaterialMissionPilot, Successor, SuccessorContinuation},
};
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
};

pub struct ContinuationRun {
    actor: usize,
    source_tick: u64,
    selection: Option<Successor>,
    trial: Option<SuccessorContinuation>,
    armed: bool,
    samples: u64,
    capture_trip: bool,
    last_transition: Option<String>,
    log: BufWriter<fs::File>,
}

impl ContinuationRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let selection = super::arg("--continue-successor", "none");
        let capture_trip: bool = super::arg("--continuation-sortie", "false")
            .parse()
            .unwrap();
        if selection == "none" {
            assert!(
                !capture_trip,
                "--continuation-sortie requires an explicit site or observe"
            );
            return None;
        }
        let selection = match selection.as_str() {
            "observe" => None,
            "escape" => Some(Successor::ContinueEscape),
            "combat" => Some(Successor::Combat),
            _ => {
                let (planet, bearing) = selection.split_once(':').expect(
                    "--continue-successor must be none, observe, escape, combat or planet:bearing",
                );
                Some(Successor::Transfer {
                    site: LandingSiteId {
                        planet: planet.parse().expect("invalid planet index"),
                        bearing: bearing.parse().expect("invalid site bearing"),
                    },
                })
            }
        };
        assert!(
            !capture_trip || selection.is_none_or(|s| matches!(s, Successor::Transfer { .. })),
            "--continuation-sortie requires a site or observe; escape keeps its original deadline"
        );
        let actor = super::arg("--continuation-seat", "1").parse().unwrap();
        let source_tick = super::arg("--continuation-tick", "0").parse().unwrap();
        assert!(
            actor < 2 && source_tick > 0,
            "specify a valid continuation seat and source tick"
        );
        Some(Self {
            actor,
            source_tick,
            selection,
            trial: None,
            armed: false,
            samples: 0,
            capture_trip,
            last_transition: None,
            log: BufWriter::new(fs::File::create(out.join("continuation.jsonl")).unwrap()),
        })
    }

    pub fn intent(
        &mut self,
        actor: usize,
        bot: &mut MaterialMissionPilot,
        o: &MissionObservationV1,
    ) -> CombatIntent {
        if actor == self.actor
            && let Some(trial) = &mut self.trial
        {
            trial.intent(bot, o)
        } else {
            bot.intent(o)
        }
    }

    /// Called after policy timing, before the common physical step. The source
    /// command is already committed and may not be replaced by this hook.
    pub fn record(
        &mut self,
        actor: usize,
        bot: &MaterialMissionPilot,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        intent: CombatIntent,
    ) {
        let tick = o.local.combat.recovery.flight.pilot.tick;
        let horizon = if self.capture_trip {
            SuccessorContinuation::MAX_SORTIE_TICKS
        } else {
            360
        };
        if actor != self.actor || !(self.source_tick..=self.source_tick + horizon).contains(&tick) {
            return;
        }
        if tick == self.source_tick && !self.armed {
            assert!(
                bot.successor_comparison(o).is_some(),
                "requested tick is not a successful handoff"
            );
            self.trial = self.selection.map(|selection| {
                if self.capture_trip {
                    let Successor::Transfer { site } = selection else {
                        unreachable!()
                    };
                    SuccessorContinuation::for_capture_trip(bot, o, site)
                } else {
                    SuccessorContinuation::new(bot, o, selection)
                }
                .expect("unusable successor proposal")
            });
            self.armed = true;
        }
        assert!(self.armed, "missed the exact requested source observation");
        if self.capture_trip {
            let report = self.trial.as_ref().map(|t| t.report());
            let trip = report.and_then(|r| r.sortie.as_ref());
            let transition = json!({"stopped":report.and_then(|r|r.stopped_tick),
                "milestones":trip.map(|t|[t.arrived_tick,t.surface_started_tick,t.landed_tick,
                    t.exited_tick,t.claimed_tick,t.boarded_tick,t.departed_tick]),
                "site":trip.and_then(|t|t.selected_site),"goal":bot.telemetry().goal,
                "capture_goal":bot.telemetry().capture.as_ref().map(|t| t.goal)})
            .to_string();
            let changed = self.last_transition.as_ref() != Some(&transition);
            self.last_transition = Some(transition);
            if tick > self.source_tick + 360 && !changed && !tick.is_multiple_of(30) {
                return;
            }
            if report
                .and_then(|r| r.stopped_tick)
                .is_some_and(|stop| tick > (stop + 60).max(self.source_tick + 360))
            {
                return;
            }
        }
        self.write(json!({"tick":tick,"after_ticks":tick-self.source_tick,
            "actor":actor,"observation":o,"actions":intent.encode(PlayerId::from_index(actor).unwrap()),
            "mission":bot.telemetry(),"trial":self.trial.as_ref().map(|t|t.report()),
            "approach_distance":self.trial.as_ref().and_then(|t|t.approach_distance(o)),
            "round":state.match_observation(),
            "combat":[state.combat_telemetry(0),state.combat_telemetry(1)],
            "damage":[state.damage_observation(0),state.damage_observation(1)],
            "solar":[state.solar_exposure(0),state.solar_exposure(1)]}));
        self.samples += 1;
    }

    fn write(&mut self, value: Value) {
        serde_json::to_writer(&mut self.log, &value).unwrap();
        writeln!(self.log).unwrap();
    }

    pub fn report(&mut self, state: &SurfaceSortieState, bot: &MaterialMissionPilot) -> Value {
        self.log.flush().unwrap();
        assert!(
            self.armed,
            "match ended without reaching the requested handoff"
        );
        let mut report = json!({"actor":self.actor,"source_tick":self.source_tick,"selection":self.selection,
            "samples":self.samples,"trial":self.trial.as_ref().map(|t|t.report()),
            "final_tick":state.tick(),"final_observation":state.mission_observation(self.actor, bot.site_request()),
            "round":state.match_observation(),
            "scope":"explicit physical intervention after one common handoff command; at most six seconds including that prefix; normal mission resumes afterward; real opponent, weapons, gravity, contacts and asteroid clock; no forecast selects controls or grants landing permission",
            "timing_scope":"continuation construction and JSON samples are diagnostic overhead outside policy/planning timings; flight controls are included in policy time"});
        if self.capture_trip {
            report["capture_trip"] = json!(true);
            report["unfinished_at_match_end"] = json!(
                self.trial
                    .as_ref()
                    .is_some_and(|t| t.report().stopped_tick.is_none())
            );
            report["scope"] = json!(
                "explicit nominated-site trip after the common handoff command; sixty-second approach and three-minute total limit; fresh local landing and ground/return planning; no forecast tail authorizes controls; normal mission resumes on stop; logs every tick for six seconds, then twice per second and on phase/milestone changes through one second after stop"
            );
        }
        report
    }

    pub fn actor(&self) -> usize {
        self.actor
    }
}
