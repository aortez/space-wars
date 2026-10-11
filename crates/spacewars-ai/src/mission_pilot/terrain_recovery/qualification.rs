//! Frozen generated-world comparisons using the launcher's Value-bot host flow.
use super::*;
use crate::{
    mission_evaluation::{DEFAULT_WORK, FlagValueShadow},
    mission_policy::{MissionBot, MissionPolicy, MissionRecoveryProfile},
};
use engine_common::{
    Action, DirtMode, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario,
};
use engine_core::planning::Work;
use scenario_spacewars::surface_sortie::{
    SurfaceSortieScenario,
    live_planning::{FlagSurveyPlanner, LiveObjectivePlanner},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    time::{Duration, Instant},
};

mod angular_diagnosis;
mod arrival_diagnosis;
mod connectivity;
mod evidence;
mod ground_rejoin;
mod hatch_arrival;
mod heldout;
mod investigation;
mod placement_cost;
mod profile;
mod round_entry;
use evidence::*;

const STEP: Duration = Duration::from_nanos(16_666_667);
const WORK: Work = Work {
    graph: DEFAULT_WORK.graph,
    physics_queries: 384,
};

#[derive(Debug, Deserialize, Serialize)]
struct Case {
    seed: u64,
    shape: String,
    slumping: bool,
    candidate_seat: Option<usize>,
}

impl Case {
    fn initialize(&self) -> Run {
        self.initialize_with_profile(MissionRecoveryProfile::TerrainV1)
    }

    fn initialize_with_profile(&self, profile: MissionRecoveryProfile) -> Run {
        assert!([10101, 20202].contains(&self.seed));
        self.initialize_match(
            profile,
            MaterialAsteroidSettings {
                interval_seconds: 8,
                severity: MaterialAsteroidSeverity::Mixed,
            },
        )
    }

    fn initialize_match(
        &self,
        profile: MissionRecoveryProfile,
        asteroids: MaterialAsteroidSettings,
    ) -> Run {
        assert!(self.candidate_seat.is_none_or(|s| s < 2));
        let mode = match self.shape.as_str() {
            "round" => DirtMode::WeakRound,
            "angular" => DirtMode::WeakAngular,
            _ => panic!("shape must be round or angular"),
        };
        let mut state = SurfaceSortieScenario::init_material_match(self.seed);
        state.set_match_time_limit(Some(Duration::from_secs(600)));
        let mut config = scenario_spacewars::surface_sortie::dirt_config(mode);
        if !self.slumping {
            config.slumping = None;
        }
        state.enable_loose_terrain_with_config(config).unwrap();
        state.set_asteroid_pressure(asteroids);
        let brains = [0, 1].map(|seat| {
            MissionBot::new(
                MissionPolicy::ValuePlanner,
                BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: self.seed,
                },
                CombatBreakSettings {
                    interval_seconds: 15,
                    duration_seconds: 4,
                },
            )
            .with_recovery_profile(if self.candidate_seat == Some(seat) {
                profile
            } else {
                MissionRecoveryProfile::Standard
            })
        });
        Run {
            state,
            brains,
            evaluation: MissionEvaluator::new(2),
            surveys: LiveObjectivePlanner::new(2, WORK),
            flags: FlagSurveyPlanner::new(2),
            shadow: FlagValueShadow::new(2),
        }
    }
}

#[derive(Clone)]
struct Run {
    state: SurfaceSortieState,
    brains: [MissionBot; 2],
    evaluation: MissionEvaluator,
    surveys: LiveObjectivePlanner,
    flags: FlagSurveyPlanner,
    shadow: FlagValueShadow,
}

#[derive(Serialize)]
struct Decision {
    sites: [Option<LandingSiteId>; 2],
    observations: [MissionObservationV1; 2],
    brains: [MissionTelemetry; 2],
    actions: Vec<Action>,
}

impl Run {
    fn decide(&mut self, candidate: Option<usize>) -> (Decision, Value) {
        let sites = self.brains.each_ref().map(|b| b.site_request());
        let mut actions = Vec::new();
        let mut decision_ms = [0.0; 2];
        let mut planning_ms = 0.0;
        let observations = std::array::from_fn(|seat| {
            let start = Instant::now();
            let (mut o, intent) = if candidate == Some(seat) {
                self.brains[seat].decide_with_terrain_recovery(
                    &mut self.state,
                    Default::default(),
                    &self.evaluation,
                )
            } else {
                let o = self.state.mission_observation_with_cadence(
                    seat,
                    self.brains[seat].sensor_request(),
                    Default::default(),
                );
                let intent = self.brains[seat].intent_with_evaluation(&o, &self.evaluation);
                (o, intent)
            };
            decision_ms[seat] = start.elapsed().as_secs_f64() * 1000.0;
            actions.extend(intent.encode(PlayerId::from_index(seat).unwrap()));
            let decision_observation = o.clone();
            let start = Instant::now();
            let request = self
                .evaluation
                .alternative_request(&o, self.brains[seat].telemetry());
            self.surveys
                .observe_destination_cover(&self.state, seat, &mut o, request);
            self.evaluation.observe(&o, self.brains[seat].telemetry());
            let request = self
                .evaluation
                .flag_request(&o, self.brains[seat].telemetry());
            self.flags.observe(&self.state, seat, &o, request);
            self.shadow
                .observe(&o, &self.evaluation, request, &self.flags.samples());
            planning_ms += start.elapsed().as_secs_f64() * 1000.0;
            decision_observation
        });
        let start = Instant::now();
        let allocation = self.surveys.advance_with_state(&self.state).unwrap();
        let evaluated = self.evaluation.advance(
            self.state.tick(),
            Work {
                graph: WORK.graph - allocation.charged.graph,
                physics_queries: WORK.physics_queries - allocation.charged.physics_queries,
            },
        );
        let busy: Vec<_> = allocation
            .jobs
            .iter()
            .filter(|j| j.charged.physics_queries > 0)
            .map(|j| j.request.actor as usize)
            .collect();
        let remaining = Work {
            graph: WORK.graph - allocation.charged.graph - evaluated.graph,
            physics_queries: WORK.physics_queries - allocation.charged.physics_queries,
        };
        let surveyed = self.flags.advance(&self.state, remaining, &busy).unwrap();
        let shadow = self.shadow.advance(
            self.state.tick(),
            Work {
                graph: remaining.graph - surveyed.charged.graph,
                physics_queries: 0,
            },
        );
        planning_ms += start.elapsed().as_secs_f64() * 1000.0;
        assert!(
            allocation.charged.graph + evaluated.graph + surveyed.charged.graph + shadow.graph
                <= WORK.graph
        );
        assert!(
            allocation.charged.physics_queries + surveyed.charged.physics_queries
                <= WORK.physics_queries
        );
        for seat in 0..2 {
            assert_eq!(
                self.state.rebuild_refinement_enabled(seat),
                Some(
                    candidate == Some(seat) && self.brains[seat].native_rebuild_refinement_active()
                )
            );
        }
        (
            Decision {
                sites,
                observations,
                brains: self.brains.each_ref().map(|b| b.telemetry().clone()),
                actions,
            },
            json!({"decision_ms":decision_ms,"planning_ms":planning_ms,
                "work":{"cover":allocation.charged,"evaluation":evaluated,
                    "flags":surveyed.charged,"shadow":shadow}}),
        )
    }
}

#[test]
#[ignore = "frozen generated-world mission recovery matrix; one case per process"]
fn frozen_mission_recovery_match() {
    let spec = std::env::var_os("SPACEWARS_MISSION_CASE").expect("case JSON path required");
    let case: Case = serde_json::from_slice(&fs::read(spec).unwrap()).unwrap();
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output path required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior output");
    save(out.join("case.json"), &case);
    let mut run = case.initialize();
    let initial_cells = run.state.terrain_diagnostics().occupied_cells;
    save_state(out, "initial", &run);
    let mut trace = Trace::new(out);
    for tick in 0..600 * 60_u64 {
        if run.state.match_outcome().is_some() {
            break;
        }
        assert_eq!(run.state.tick(), tick);
        let had_task = run.brains.each_ref().map(|b| b.recovery.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries);
        let (decision, mut cost) = run.decide(case.candidate_seat);
        trace.decision(out, &case, tick, &decision, &run, had_task, completed);
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
        cost["step_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
        cost["tick"] = json!(tick);
        line(&mut trace.costs, &cost);
        if (tick + 1) % 60 == 0 || run.state.match_outcome().is_some() {
            audit(&run, initial_cells);
            line(
                &mut trace.samples,
                &json!({"tick":tick+1,"state":snapshot(&run)}),
            );
        }
    }
    audit(&run, initial_cells);
    assert!(
        run.state.match_outcome().is_some(),
        "native ten-minute match must finish"
    );
    save_state(out, "final", &run);
    trace.flush();
    save(
        out.join("summary.json"),
        &json!({
            "case":case,"end_tick":run.state.tick(),"match":run.state.match_observation(),
            "metrics":trace.metrics,"initial_cells":initial_cells,
            "brains":run.brains.each_ref().map(|b| b.telemetry()),
            "native_recovery":([0,1].map(|s| run.state.pilot_observation(s,None).recovery)),
            "native_refinement":([0,1].map(|s| run.state.rebuild_refinement_enabled(s))),
            "controller_reset":false,"profile_from_tick_zero":true,
            "policy":"material_mission_v13","host_work":WORK
        }),
    );
    eprintln!(
        "{case:?}: tick {}, {:?}; recoveries {:?}",
        run.state.tick(),
        run.state.match_outcome(),
        run.brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries)
    );
}
