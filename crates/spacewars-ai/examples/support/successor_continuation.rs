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
    log: BufWriter<fs::File>,
}

impl ContinuationRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let selection = super::arg("--continue-successor", "none");
        if selection == "none" {
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
        if actor != self.actor || !(self.source_tick..=self.source_tick + 360).contains(&tick) {
            return;
        }
        if tick == self.source_tick && !self.armed {
            assert!(
                bot.successor_comparison(o).is_some(),
                "requested tick is not a successful handoff"
            );
            self.trial = self.selection.map(|selection| {
                SuccessorContinuation::new(bot, o, selection).expect("unusable successor proposal")
            });
            self.armed = true;
        }
        assert!(self.armed, "missed the exact requested source observation");
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
        json!({"actor":self.actor,"source_tick":self.source_tick,"selection":self.selection,
            "samples":self.samples,"trial":self.trial.as_ref().map(|t|t.report()),
            "final_tick":state.tick(),"final_observation":state.mission_observation(self.actor, bot.site_request()),
            "round":state.match_observation(),
            "scope":"explicit physical intervention after one common handoff command; at most six seconds including that prefix; normal mission resumes afterward; real opponent, weapons, gravity, contacts and asteroid clock; no forecast selects controls or grants landing permission",
            "timing_scope":"continuation construction and JSON samples are diagnostic overhead outside policy/planning timings; flight controls are included in policy time"})
    }

    pub fn actor(&self) -> usize {
        self.actor
    }
}
