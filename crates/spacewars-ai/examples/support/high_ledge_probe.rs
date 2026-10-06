//! Offline native-physics forks. The main replay never consumes probe plans.
use engine_common::{Action, Scenario};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieAction, SurfaceSortieScenario, SurfaceSortieState,
        jetpack::{
            MAX_TERRAIN_CROSSINGS, diagnostics::TerrainGapCandidate, flight::LANDING_RESERVE,
        },
        pilot::PilotObservationV1,
        recovery_sensors::RecoveryTaskObservationV1,
    },
};
use serde_json::{Value, json};
use spacewars_ai::{
    BrainReset,
    ground_task::{GroundDestination, GroundGoal, GroundNavigationTask, GroundTelemetry},
};
use std::{collections::BTreeMap, fs, io::Write, path::Path, time::Duration};

const DT: Duration = Duration::from_nanos(16_666_667);

struct Source {
    state: SurfaceSortieState,
    observation: RecoveryTaskObservationV1,
    ground: GroundTelemetry,
    deadline: u64,
}

pub struct HighLedgeProbe {
    seat: usize,
    ticks: Vec<u64>,
    from: u16,
    to: u16,
    sources: Vec<Source>,
    tape: BTreeMap<u64, (Vec<Action>, Value)>,
}

pub fn pilot_key(p: &PilotObservationV1) -> Value {
    json!({"tick":p.tick,"owner":p.owner,"location":p.location,"actor":p.actor,
        "actor_up":p.actor_up,"supported_planet":p.supported_planet,"balanced":p.balanced,
        "relative_speed":p.relative_speed,"planet":p.planet,"ship":p.ship,
        "ship_health":p.ship_health,"ship_form":p.ship_form,"ship_available":p.ship_available,
        "recovery":p.recovery,"gravity":p.gravity,"transfers":p.transfers})
}

fn native_key(state: &SurfaceSortieState) -> Value {
    json!({"tick":state.tick(),"pilots":[pilot_key(&state.pilot_observation(0,None)),
        pilot_key(&state.pilot_observation(1,None))],"round":state.match_observation()})
}

impl HighLedgeProbe {
    pub fn from_args() -> Option<Self> {
        let spec = crate::arg("--probe-high-ledge", "none");
        if spec == "none" {
            return None;
        }
        assert_eq!(crate::arg("--mode", "quiet"), "duel");
        let parts: Vec<_> = spec.split(':').collect();
        assert_eq!(parts.len(), 4, "expected seat:tick,tick:from:to");
        let seat = parts[0].parse().unwrap();
        let ticks: Vec<u64> = parts[1].split(',').map(|t| t.parse().unwrap()).collect();
        let from = parts[2].parse().unwrap();
        let to = parts[3].parse().unwrap();
        assert!(seat < 2 && !ticks.is_empty() && ticks.len() <= 2);
        assert!(ticks.windows(2).all(|t| t[0] < t[1]));
        assert!(from < 512 && to < 512 && from != to);
        Some(Self {
            seat,
            ticks,
            from,
            to,
            sources: Vec::new(),
            tape: BTreeMap::new(),
        })
    }

    pub fn observe(
        &mut self,
        state: &SurfaceSortieState,
        seat: usize,
        observation: &RecoveryTaskObservationV1,
        ground: Option<&GroundTelemetry>,
    ) {
        if seat != self.seat || !self.ticks.contains(&state.tick()) {
            return;
        }
        let ground = ground.expect("probe requires an active ground task");
        let deadline = ground.started_tick.unwrap() + 90 * 60;
        assert!(state.tick() < deadline && deadline - state.tick() <= 90 * 60);
        assert_eq!(observation.flight.pilot.tick, state.tick());
        assert_eq!(observation.ground.as_ref().unwrap().tick, state.tick());
        self.sources.push(Source {
            state: state.clone(),
            observation: observation.clone(),
            ground: ground.clone(),
            deadline,
        });
    }

    pub fn record_step(&mut self, state: &SurfaceSortieState, actions: &[Action]) {
        if self
            .sources
            .iter()
            .any(|s| (s.state.tick()..=s.deadline).contains(&state.tick()))
        {
            assert!(
                self.tape
                    .insert(state.tick(), (actions.to_vec(), native_key(state)))
                    .is_none()
            );
        }
    }

    pub fn finish(self, out: &Path, seed: u64) {
        assert_eq!(
            self.sources
                .iter()
                .map(|s| s.state.tick())
                .collect::<Vec<_>>(),
            self.ticks
        );
        let mut tape = fs::File::create(out.join("high-ledge-reference.jsonl")).unwrap();
        for (tick, (actions, key)) in &self.tape {
            writeln!(
                tape,
                "{}",
                json!({"tick":tick,"actions":actions,"native":key})
            )
            .unwrap();
        }
        let results: Vec<_> = self
            .sources
            .iter()
            .map(|s| {
                let map = s.observation.ground.as_ref().unwrap();
                let candidates = s
                    .state
                    .diagnose_terrain_gap(self.seat, map, self.from, self.to)
                    .unwrap();
                assert!(candidates.len() <= 24);
                let mut control = s.state.clone();
                let mut checked = 0;
                loop {
                    let (actions, expected) = &self.tape[&control.tick()];
                    assert_eq!(
                        native_key(&control),
                        *expected,
                        "control fork changed at {}",
                        control.tick()
                    );
                    checked += 1;
                    if control.tick() == s.deadline {
                        break;
                    }
                    SurfaceSortieScenario::step(&mut control, actions, DT);
                }
                let selected = candidates
                    .iter()
                    .find(|c| !c.ordinary_height_allowed && c.corridor_clear == Some(true));
                let execution = selected.map(|candidate| self.execute(s, candidate, seed, out));
                json!({"tick":s.state.tick(),"seat":self.seat,"ground":s.ground,
                "observation":s.observation,"deadline":s.deadline,
                "control_exact_ticks":checked,"candidates":candidates,
                "selected":selected,"execution":execution})
            })
            .collect();
        fs::write(out.join("high-ledge-probe.json"), serde_json::to_vec_pretty(&json!({
            "schema":1,"seat":self.seat,"ticks":self.ticks,"gap":[self.from,self.to],
            "selection":"first clearance-accepted candidate in margin/height order above the ordinary height bound",
            "scope":"Native cloned worlds; fresh local flag navigator within the original absolute ground deadline. Other inputs replay the recorded tape and do not react to the changed pilot. Not a full mission or production forecast.",
            "launch_charge":0.98,"landing_reserve":LANDING_RESERVE,"flight_limit_ticks":720,
            "results":results})).unwrap()).unwrap();
    }

    fn execute(
        &self,
        source: &Source,
        candidate: &TerrainGapCandidate,
        seed: u64,
        out: &Path,
    ) -> Value {
        let mut state = source.state.clone();
        let owner = PlayerId::from_index(self.seat).unwrap();
        let mut task = GroundNavigationTask::new(
            BrainReset {
                actor: owner,
                episode_seed: seed,
            },
            GroundDestination::Flag,
        );
        let mut trace =
            fs::File::create(out.join(format!("high-ledge-flight-{}.jsonl", state.tick())))
                .unwrap();
        let mut launched = None;
        let mut lowest_charge = 1.0_f32;
        loop {
            let mut o = state.recovery_task_observation(self.seat, None);
            let mut measurement = None;
            if let Some(map) = &o.ground {
                measurement = state
                    .diagnose_terrain_gap(self.seat, map, self.from, self.to)
                    .and_then(|all| {
                        all.into_iter()
                            .find(|c| c.margin == candidate.margin && c.height == candidate.height)
                    })
                    .filter(|c| c.corridor_clear == Some(true));
            }
            let Some(j) = o.jetpack.as_mut() else {
                let receipt = json!({"tick":state.tick(),"pilot":pilot_key(&o.flight.pilot),
                    "native":native_key(&state),"stop":"equipment_unavailable"});
                writeln!(
                    trace,
                    "{}",
                    json!({"observation":receipt,"actions":[],"applied":false})
                )
                .unwrap();
                return json!({"reason":"equipment_unavailable","launched_tick":launched,
                    "last_tick":state.tick(),"lowest_charge":lowest_charge,"last":receipt});
            };
            lowest_charge = lowest_charge.min(j.charge);
            let charge = j.charge;
            let already_measured = measurement.as_ref().is_some_and(|c| {
                j.terrain_crossings
                    .iter()
                    .any(|old| old.same_corridor(&c.plan))
            });
            let capacity = measurement.is_none()
                || already_measured
                || j.terrain_crossings.len() < MAX_TERRAIN_CROSSINGS;
            if capacity
                && !already_measured
                && let Some(c) = &measurement
            {
                j.terrain_crossings.push(c.plan);
            }
            let mut reason = if state.tick() >= source.deadline {
                Some("ground_deadline")
            } else if launched.is_some_and(|t| state.tick() - t >= 720) {
                Some("flight_deadline")
            } else if charge < LANDING_RESERVE {
                Some("fuel_reserve")
            } else if !capacity {
                Some("sensor_capacity")
            } else if state.match_outcome().is_some() {
                Some("round_ended")
            } else {
                None
            };
            let action = if reason.is_none() {
                task.step(&o)
            } else {
                SurfaceSortieAction::default()
            };
            let t = task.telemetry();
            if t.goal == GroundGoal::JetpackLift && action.primary_held {
                launched.get_or_insert(state.tick());
            }
            if t.jetpack_crossings > 0 {
                reason = Some("crossing_complete");
            } else if t.goal == GroundGoal::Blocked {
                reason = Some("controller_blocked");
            }
            let p = &o.flight.pilot;
            let foot = p.actor.map(|a| {
                (a.position
                    - p.actor_up * scenario_spacewars::spaceling_geometry::HALF_HEIGHT
                    - p.planet.motion.position)
                    .rotate_radians(-p.planet.motion.angle)
            });
            let receipt = json!({"tick":state.tick(),"pilot":pilot_key(p),"ground":t,
                "jetpack":o.jetpack,"measurement":measurement,"foot":foot,
                "native":native_key(&state),"stop":reason});
            let mut actions = self.tape[&state.tick()].0.clone();
            let mut replaced = 0;
            for existing in &mut actions {
                if SurfaceSortieAction::decode(existing).is_some_and(|(p, _)| p == owner) {
                    *existing = action.encode(owner);
                    replaced += 1;
                }
            }
            assert_eq!(replaced, 1);
            writeln!(
                trace,
                "{}",
                json!({"observation":receipt,"actions":actions,"applied":reason.is_none()})
            )
            .unwrap();
            if let Some(reason) = reason {
                return json!({"reason":reason,"launched_tick":launched,"last_tick":state.tick(),
                    "lowest_charge":lowest_charge,"last":receipt});
            }
            SurfaceSortieScenario::step(&mut state, &actions, DT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pilot_key_keeps_motion_and_recovery_but_excludes_survey_requests() {
        let state = SurfaceSortieScenario::init_material_jetpack(42, 1);
        let p = state.pilot_observation(0, None);
        let key = pilot_key(&p);
        assert_eq!(key["tick"], 0);
        assert_eq!(key["ship"], serde_json::to_value(p.ship).unwrap());
        assert!(key.get("recovery").is_some() && key.get("sites").is_none());
    }
}
