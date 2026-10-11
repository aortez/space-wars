//! Recreate an ordinary Picade automatic match and record native physics costs.
//! Matches the Legacy/Planner host order, budgets and saved heavy/Round preset.
//! Timings exclude evidence I/O; no experimental recovery profile is selected.
use engine_common::{
    CombatBreakSettings, DirtMode, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario,
};
use engine_core::planning::Work;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieScenario, live_planning::LiveObjectivePlanner},
};
use serde::Serialize;
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    mission_evaluation::{DEFAULT_WORK, MissionEvaluator},
    mission_policy::{MissionBot, MissionPolicy},
};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const STEP: Duration = Duration::from_nanos(16_666_667);
const WORK: Work = Work {
    graph: DEFAULT_WORK.graph,
    physics_queries: 384,
};

fn argument(name: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == name)
        .unwrap_or_else(|| panic!("{name} required"))[1]
        .clone()
}

fn line(output: &mut BufWriter<File>, value: &impl Serialize) {
    serde_json::to_writer(&mut *output, value).unwrap();
    writeln!(output).unwrap();
}

fn fnv64(bytes: &[u8]) -> String {
    format!(
        "{:016x}",
        bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        })
    )
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn main() {
    let seed: u64 = argument("--seed").parse().unwrap();
    let ticks: u64 = argument("--ticks").parse().unwrap();
    assert!((1..=54_000).contains(&ticks));
    let output = PathBuf::from(argument("--out"));
    fs::create_dir(&output).expect("use a fresh output directory");
    let mut costs = BufWriter::new(File::create(output.join("costs.jsonl")).unwrap());
    let mut actions_log = BufWriter::new(File::create(output.join("actions.jsonl")).unwrap());
    let mut samples = BufWriter::new(File::create(output.join("samples.jsonl")).unwrap());
    let mut state = SurfaceSortieScenario::init_material_match(seed);
    state.set_match_time_limit(Some(Duration::from_secs(900)));
    state.set_asteroid_pressure(MaterialAsteroidSettings {
        interval_seconds: 1,
        severity: MaterialAsteroidSeverity::Heavy,
    });
    state.enable_dirt_mode(DirtMode::Round);
    let initial_cells = state.terrain_diagnostics().occupied_cells;
    let mut bots = [MissionPolicy::Legacy, MissionPolicy::Planner].map(|policy| {
        let seat = usize::from(policy == MissionPolicy::Planner);
        MissionBot::new(
            policy,
            BrainReset {
                actor: PlayerId::from_index(seat).unwrap(),
                episode_seed: seed,
            },
            CombatBreakSettings {
                interval_seconds: 15,
                duration_seconds: 4,
            },
        )
    });
    let mut evaluation = MissionEvaluator::new(2);
    let mut surveys = LiveObjectivePlanner::new(2, WORK);
    fs::write(
        output.join("initial-snapshot.json"),
        SurfaceSortieScenario::observe(&state).payload,
    )
    .unwrap();
    fs::write(output.join("config.json"), serde_json::to_vec_pretty(&json!({
        "seed": seed, "requested_ticks": ticks, "dt_nanos": STEP.as_nanos(), "match_seconds": 900,
        "policies": ["material_mission_v9", "material_mission_v10"], "experimental_recovery": false,
        "dirt_mode": "round", "loose_dirt": true, "asteroid_interval_seconds": 1, "asteroid_severity": "heavy",
        "combat_break_seconds": [15,4], "landing_survey_hz": 4, "work": {"graph": WORK.graph, "physics_queries": WORK.physics_queries},
        "scope": "headless recreation from seed/config; original live inputs and solver snapshot were not captured"
    })).unwrap()).unwrap();

    for _ in 0..ticks {
        if state.match_outcome().is_some() {
            break;
        }
        let before_tick = state.tick();
        let started = Instant::now();
        let mut actions = Vec::new();
        for (seat, bot) in bots.iter_mut().enumerate() {
            let mut observation = state.mission_observation_with_cadence(
                seat,
                bot.sensor_request(),
                Default::default(),
            );
            actions.extend(
                bot.intent_with_evaluation(&observation, &evaluation)
                    .encode(PlayerId::from_index(seat).unwrap()),
            );
            let request = evaluation.alternative_request(&observation, bot.telemetry());
            surveys.observe_destination_cover(&state, seat, &mut observation, request);
            evaluation.observe(&observation, bot.telemetry());
        }
        let allocation = surveys.advance_with_state(&state).unwrap();
        evaluation.advance(
            state.tick(),
            Work {
                graph: WORK.graph - allocation.charged.graph,
                physics_queries: WORK.physics_queries - allocation.charged.physics_queries,
            },
        );
        let decisions_ms = ms(started.elapsed());
        let started = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, STEP);
        let step_ms = ms(started.elapsed());
        let m = state.last_step_metrics();
        let r = m.rapier;
        line(
            &mut costs,
            &json!({
                "tick":state.tick(), "decisions_ms":decisions_ms, "step_ms":step_ms,
                "physics_ms":ms(m.physics_time), "lifecycle_ms":ms(m.lifecycle_time), "workload_ms":ms(m.workload_time),
                "gravity_ms":ms(m.gravity_time), "collision_ms":ms(m.collision_time),
                "rapier_step_ms":ms(r.rapier_step_time), "update_ms":ms(r.update_time), "user_changes_ms":ms(r.user_changes_time),
                "kinematic_ms":ms(r.kinematic_interpolation_time), "collision_detection_ms":ms(r.collision_detection_time),
                "broad_phase_ms":ms(r.broad_phase_time), "final_broad_phase_ms":ms(r.final_broad_phase_time),
                "narrow_phase_ms":ms(r.narrow_phase_time), "islands_ms":ms(r.island_time),
                "island_constraints_ms":ms(r.island_constraints_time), "solver_ms":ms(r.solver_time),
                "ccd_ms":r.ccd_time.map(ms), "ccd_first_impact_ms":ms(r.ccd_first_impact_time), "ccd_clamping_ms":ms(r.ccd_clamping_time),
                "ccd_substeps":r.ccd_substeps, "active_bodies":r.active_bodies, "sleeping_bodies":r.sleeping_bodies,
                "candidate_pairs":r.candidate_pairs, "contact_pairs":r.contact_pairs, "solver_contacts":r.contacts
            }),
        );
        line(
            &mut actions_log,
            &json!({"tick":before_tick,"actions":actions}),
        );
        if state.tick().is_multiple_of(60)
            || state.tick() == ticks
            || state.match_outcome().is_some()
        {
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "terrain audit: {:?}", audit.issues);
            assert_eq!(
                audit.occupied_cells + audit.loose_cells as u64 + audit.removed_cells,
                initial_cells
            );
            assert!(audit.max_speed < 500.0, "speed alarm: {}", audit.max_speed);
            let pairs = state.physics_pair_diagnostics();
            line(
                &mut samples,
                &json!({"tick":state.tick(), "terrain":audit,
                "scenario_fnv64":fnv64(&SurfaceSortieScenario::observe(&state).payload),
                    "pair_diagnostics":{
                        "same_body_candidates":pairs.same_body_candidates,
                        "other_candidates":pairs.other_candidates,
                        "removed_collider_candidates":pairs.removed_collider_candidates,
                        "active_contact_pairs":pairs.active_contact_pairs
                    }, "match":state.match_observation(),
                "bots":bots.each_ref().map(|bot| bot.telemetry())}),
            );
        }
        if state.tick().is_multiple_of(3000) {
            costs.flush().unwrap();
            actions_log.flush().unwrap();
            samples.flush().unwrap();
            eprintln!("tick={} physics_ms={:.3}", state.tick(), ms(m.physics_time));
        }
    }
    costs.flush().unwrap();
    actions_log.flush().unwrap();
    samples.flush().unwrap();
    fs::write(
        output.join("final-snapshot.json"),
        SurfaceSortieScenario::observe(&state).payload,
    )
    .unwrap();
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(
            &json!({"end_tick":state.tick(), "match":state.match_observation(),
        "terrain":state.terrain_diagnostics(), "completed":true}),
        )
        .unwrap(),
    )
    .unwrap();
}
