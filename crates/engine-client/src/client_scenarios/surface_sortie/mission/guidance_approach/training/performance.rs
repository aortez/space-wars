//! Test-only throughput and stage measurements. No rendering or training changes.
use super::*;
use scenario_spacewars::{PlayerId, SpacewarsStepMetrics};
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

mod parity;

#[derive(Default, Serialize)]
struct Timing {
    calls: u64,
    total_ms: f64,
}

#[derive(Default, Serialize)]
struct Profile {
    stages: BTreeMap<String, Timing>,
    counts: BTreeMap<String, u64>,
}

impl Profile {
    fn record(&mut self, name: impl Into<String>, elapsed: Duration) {
        let sample = self.stages.entry(name.into()).or_default();
        sample.calls += 1;
        sample.total_ms += elapsed.as_secs_f64() * 1000.0;
    }

    fn world(&mut self, prefix: &str, m: SpacewarsStepMetrics) {
        for (name, duration) in [
            ("physics", m.physics_time),
            ("lifecycle", m.lifecycle_time),
            ("gravity", m.gravity_time),
            ("collision", m.collision_time),
            ("workload", m.workload_time),
            ("motion_diagnostics", m.motion_diagnostics_time),
            ("rapier_update", m.rapier.update_time),
            (
                "rapier_collision_detection",
                m.rapier.collision_detection_time,
            ),
            ("rapier_broad_phase", m.rapier.broad_phase_time),
            ("rapier_narrow_phase", m.rapier.narrow_phase_time),
            ("rapier_solver", m.rapier.solver_time),
            ("rapier_ccd", m.rapier.ccd_time.unwrap_or_default()),
        ] {
            self.record(format!("{prefix}.{name}"), duration);
        }
        for (name, value) in [
            ("steps", 1),
            ("active_bodies", m.rapier.active_bodies as u64),
            ("sleeping_bodies", m.rapier.sleeping_bodies as u64),
            ("candidate_pairs", m.rapier.candidate_pairs as u64),
            ("contacts", m.rapier.contacts as u64),
            ("ccd_substeps", m.rapier.ccd_substeps as u64),
        ] {
            *self.counts.entry(format!("{prefix}.{name}")).or_default() += value;
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Profile>> = const { RefCell::new(None) };
}

pub(crate) fn timed<T>(name: &'static str, operation: impl FnOnce() -> T) -> T {
    let started = ACTIVE.with_borrow(|p| p.is_some().then(Instant::now));
    let result = operation();
    if let Some(started) = started {
        let elapsed = started.elapsed();
        ACTIVE.with_borrow_mut(|p| p.as_mut().unwrap().record(name, elapsed));
    }
    result
}

fn measured<T>(operation: impl FnOnce() -> T) -> (T, Profile) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with_borrow_mut(|p| *p = None);
        }
    }
    ACTIVE.with_borrow_mut(|p| {
        assert!(p.is_none());
        *p = Some(Profile::default());
    });
    let _reset = Reset;
    let value = operation();
    let profile = ACTIVE.with_borrow_mut(|p| p.take().unwrap());
    (value, profile)
}

pub(crate) fn record_match_step(prefix: &str, run: &MaterialMissionClientScenario) {
    ACTIVE.with_borrow_mut(|p| {
        let Some(p) = p else { return };
        let sample = run.profile.latest_sample().unwrap();
        p.world(prefix, sample.world);
        for (name, duration) in [
            ("sensors", sample.sensors.into_iter().sum()),
            ("policies", sample.policies.into_iter().sum()),
            ("planning", sample.planning),
            ("scenario", sample.scenario),
            ("total", sample.total),
        ] {
            p.record(format!("{prefix}.{name}"), duration);
        }
    });
}

fn precision_replay(cases: &[ship_arrival::ArrivalCase], model: &DenseModel) -> (Profile, u64) {
    let mut profile = Profile::default();
    let mut ticks = 0;
    for case in cases {
        // Trace generation and equality checks are excluded from stage timers.
        // The actual evaluator provides the requests, controls and every state.
        let expected =
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true).unwrap();
        let clock = Instant::now();
        let mut world = SurfaceSortieScenario::init_material_flight(
            case.seed,
            1,
            &[(PlayerId::PLAYER_1, case.start)],
        );
        profile.record("world_creation", clock.elapsed());
        let clock = Instant::now();
        for _ in 0..expected.setup_ticks {
            SurfaceSortieScenario::step(
                &mut world,
                &Movement::default().encode(PlayerId::PLAYER_1),
                ship_arrival::DT,
            );
        }
        profile.record("arming_steps", clock.elapsed());
        assert_eq!(world.free_flight_observation(0).unwrap(), expected.initial);
        let binding = expected.trace.first().unwrap().request.binding;
        let clock = Instant::now();
        let mut controller = Controller::new(Policy::Model(model.clone()), binding).unwrap();
        profile.record("controller_creation", clock.elapsed());
        for row in &expected.trace {
            let clock = Instant::now();
            let current = world.free_flight_observation(0).unwrap();
            let features = Features::read(&current, row.target_before);
            let encoded = features.encode().unwrap();
            profile.record("observation_and_features", clock.elapsed());
            assert_eq!(current, row.before);
            assert_eq!(encoded, row.encoded);
            let clock = Instant::now();
            let decision = controller
                .decide(Request {
                    binding,
                    tick: current.tick,
                    target_id: row.target_before.id,
                    features,
                })
                .unwrap();
            profile.record("network_decision", clock.elapsed());
            assert_eq!(Some(decision.movement), row.movement);
            assert_eq!(decision.raw_outputs, row.raw_outputs);
            let clock = Instant::now();
            SurfaceSortieScenario::step(
                &mut world,
                &decision.movement.encode(binding.owner),
                ship_arrival::DT,
            );
            profile.record("scenario_step", clock.elapsed());
            profile.world("world", world.last_step_metrics());
            let clock = Instant::now();
            let after = world.free_flight_observation(0);
            profile.record("post_step_observation", clock.elapsed());
            assert_eq!(after, row.after);
            ticks += 1;
        }
        assert_eq!(world.free_flight_observation(0), expected.final_observation);
    }
    (profile, ticks)
}

fn candidate(
    sources: &[Source],
    retained: &[ship_arrival::ArrivalCase],
    model: &DenseModel,
    workers: usize,
) -> (Value, Vec<Episode>) {
    let started = Instant::now();
    let precision = full_retention::episodes(retained, model, workers).unwrap();
    let precision_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let approaches: Vec<_> = sources
        .iter()
        .map(|(case, source)| episode(source, case, "model", model, None))
        .collect();
    let match_seconds = started.elapsed().as_secs_f64();
    let mut fitness = Fitness::default();
    for e in &precision {
        fitness.retained_completed += e.fitness.completed;
        fitness.retained_position_points += e.fitness.position_points;
        fitness.retained_velocity_points += e.fitness.velocity_points;
        fitness.spare_ticks += e.fitness.spare_ticks;
    }
    for e in &approaches {
        fitness.record_approach(e);
    }
    let precision_ticks: u64 = precision.iter().map(|e| u64::from(e.elapsed_ticks)).sum();
    let precision_setup_and_cleanup: u64 =
        precision.iter().map(|e| u64::from(e.setup_ticks + 1)).sum();
    let match_ticks: u64 = approaches.iter().map(|e| e.elapsed_ticks).sum();
    (
        json!({
            "genome":model.id(),"workers":workers,"fitness":fitness,
            "precision_seconds":precision_seconds,"match_seconds":match_seconds,
            "precision_scored_ticks":precision_ticks,"precision_setup_and_cleanup_ticks":precision_setup_and_cleanup,
            "match_scored_ticks":match_ticks,
            "precision_scored_ticks_per_second":precision_ticks as f64/precision_seconds,
            "match_scored_ticks_per_second":match_ticks as f64/match_seconds,
            "combined_scored_ticks_per_second":(precision_ticks+match_ticks) as f64/(precision_seconds+match_seconds),
            "episodes":retained.len()+sources.len(),
        }),
        approaches,
    )
}

#[test]
#[ignore = "explicit retained v3 evidence and fresh profiling output required"]
fn learned_guidance_training_profile() {
    let input = std::env::var_os("SPACEWARS_TRAINING_PROFILE_INPUT").unwrap();
    let input = Path::new(&input);
    let out = std::env::var_os("SPACEWARS_TRAINING_PROFILE_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("preserve prior profile runs");
    let clock = Instant::now();
    let runtime = engine_agent::evolution::runtime_id().unwrap();
    let runtime_identity_seconds = clock.elapsed().as_secs_f64();
    let plan: Value = serde_json::from_slice(LEARNING_PLAN_V3).unwrap();
    let frozen: Value =
        serde_json::from_slice(&fs::read(input.join("run/checkpoint.json")).unwrap()).unwrap();
    let preflight: Value =
        serde_json::from_slice(&fs::read(input.join("preflight/preflight.json")).unwrap()).unwrap();
    let parent = DenseModel::from_json(GENOME).unwrap();
    let finalist =
        DenseModel::from_json(&fs::read(input.join("run/finalist.json")).unwrap()).unwrap();
    let mutant = DenseModel::new(
        serde_json::from_value(frozen["run"]["population"][7]["genome"].clone()).unwrap(),
    )
    .unwrap();
    let clock = Instant::now();
    let sources = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    let setup_seconds = clock.elapsed().as_secs_f64();
    let source_records = records(&sources);
    let expected_sources = preflight["training"].as_array().unwrap();
    assert_eq!(source_records.len(), expected_sources.len());
    for (actual, expected) in source_records.iter().zip(expected_sources) {
        // Compare exact serialized-state hashes, avoiding a second JSON parse's
        // f64 rounding of the original native f32 observations.
        for field in ["world_sha256", "mission_sha256"] {
            assert_eq!(actual[field], expected[field], "{field}");
        }
        let case: Case = serde_json::from_value(expected["case"].clone()).unwrap();
        assert_eq!(actual["case"], serde_json::to_value(case).unwrap());
    }
    let retained = retention_cases(&plan);
    let mut report = json!({
        "runtime":runtime,"original_runtime":frozen["run"]["runtime"],
        "runtime_identity_seconds":runtime_identity_seconds,"source_setup_seconds":setup_seconds,
        "source_prefix_ticks":sources.iter().map(|(c,_)|c.source_tick).sum::<u64>(),
        "source_world_and_controller_hashes_match_frozen_training":true,"precision_cases":retained.len(),
        "match_cases":sources.len(),"benchmarks":[],
        "note":"Elapsed wall times on this host; stage timers are nested. Precision profile replays exact evaluator controls and states. No rendering.",
    });
    // Re-encode a historical checkpoint only to measure its storage cost. This
    // copy is never resumed; normal training still enforces the runtime gate.
    let historical: EvolutionRun<Fitness> = serde_json::from_value(frozen["run"].clone()).unwrap();
    let encoded = historical.encode().unwrap();
    assert_eq!(
        encoded,
        fs::read(input.join("run/checkpoint.json")).unwrap()
    );
    let clock = Instant::now();
    for _ in 0..32 {
        historical
            .save(&out.join("checkpoint-io-copy.json"))
            .unwrap();
    }
    report["checkpoint_io"] = json!({"saves":32,"bytes_per_save":encoded.len(),
        "seconds":clock.elapsed().as_secs_f64(),"byte_parity":true});
    save(out.join("profile.json"), &report);
    for (label, model, workers) in [
        ("parent", &parent, 4),
        ("finalist", &finalist, 4),
        ("mutant", &mutant, 4),
        ("finalist", &finalist, 1),
        ("finalist", &finalist, 8),
        ("finalist", &finalist, 4),
        ("finalist", &finalist, 8),
        ("finalist", &finalist, 1),
        ("finalist", &finalist, 4),
    ] {
        let (mut sample, _) = candidate(&sources, &retained, model, workers);
        let expected = frozen["run"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|h| {
                h["population_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|id| id.as_str() == Some(model.id()))
                    .map(|index| &h["fitness"][index])
            })
            .unwrap();
        assert_eq!(&sample["fitness"], expected);
        sample["label"] = json!(label);
        sample["fitness_matches_frozen_run"] = json!(true);
        eprintln!("{}", sample);
        report["benchmarks"].as_array_mut().unwrap().push(sample);
        save(out.join("profile.json"), &report);
    }
    let (baseline, expected) = candidate(&sources, &retained, &finalist, 4);
    let (approaches, stages) = measured(|| {
        timed("match_episodes", || {
            sources
                .iter()
                .map(|(c, s)| episode(s, c, "model", &finalist, None))
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(approaches, expected);
    report["match_profile"] = json!(stages);
    report["match_profile_parity"] = json!(true);
    report["match_profile_control"] = baseline;
    save(out.join("profile.json"), &report);
    let (precision, ticks) = precision_replay(&retained, &finalist);
    report["precision_profile"] = json!(precision);
    report["precision_replayed_ticks"] = json!(ticks);
    report["precision_profile_parity"] = json!(true);
    report["complete"] = json!(true);
    save(out.join("profile.json"), &report);
    eprintln!(
        "Profile complete; all saved scores, profiled match episodes and precision tick states match."
    );
}
