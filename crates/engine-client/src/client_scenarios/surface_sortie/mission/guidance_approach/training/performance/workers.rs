//! Bounded replay benchmark, never evolutionary training. Same source worlds,
//! candidate and complete episode results for every resource configuration.
use super::*;

fn pooled_candidate(
    pool: &EpisodePool,
    sources: &[Source],
    retained: &[ship_arrival::ArrivalCase],
    model: &DenseModel,
) -> (Value, Vec<ship_arrival::ArrivalEpisode>, Vec<Episode>) {
    let started = Instant::now();
    let precision = pool
        .map(retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false)
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let precision_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let approaches = pool.map(sources, |(case, source)| {
        episode(&source.lock().unwrap(), case, "model", model, None)
    });
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
    let ticks: u64 = precision
        .iter()
        .map(|e| u64::from(e.elapsed_ticks))
        .sum::<u64>()
        + approaches.iter().map(|e| e.elapsed_ticks).sum::<u64>();
    (
        json!({"precision_seconds":precision_seconds,"match_seconds":match_seconds,
        "seconds":precision_seconds+match_seconds,"scored_ticks":ticks,
        "scored_ticks_per_second":ticks as f64/(precision_seconds+match_seconds),
        "fitness":fitness,"workers":pool.control().snapshot()}),
        precision,
        approaches,
    )
}

#[test]
#[ignore = "explicit retained v3 input and fresh worker benchmark output required"]
fn learned_guidance_worker_scaling() {
    let input = std::env::var_os("SPACEWARS_TRAINING_PROFILE_INPUT").unwrap();
    let input = Path::new(&input);
    let out = std::env::var_os("SPACEWARS_TRAINING_PROFILE_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("preserve prior worker benchmarks");
    let preflight: Value =
        serde_json::from_slice(&fs::read(input.join("preflight/preflight.json")).unwrap()).unwrap();
    let frozen: Value =
        serde_json::from_slice(&fs::read(input.join("run/checkpoint.json")).unwrap()).unwrap();
    let model = DenseModel::from_json(&fs::read(input.join("run/finalist.json")).unwrap()).unwrap();
    let plan: Value = serde_json::from_slice(LEARNING_PLAN_V3).unwrap();
    let sources = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    for (actual, expected) in records(&sources)
        .iter()
        .zip(preflight["training"].as_array().unwrap())
    {
        for field in ["world_sha256", "mission_sha256"] {
            assert_eq!(actual[field], expected[field]);
        }
    }
    let retained = retention_cases(&plan);
    let expected_precision = full_retention::episodes(&retained, &model, 1).unwrap();
    let (baseline, expected_approaches) = candidate(&sources, &retained, &model, 4);
    let saved = frozen["run"]["history"]
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
    assert_eq!(&baseline["fitness"], saved);
    let pool = EpisodePool::new(WorkerConfig::default()).unwrap();
    let mut report = json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "source_hash_parity":true,"historical_fitness_parity":true,
        "episodes_per_candidate":retained.len()+sources.len(),"baseline":baseline,
        "benchmarks":[],"auto_samples":[],"complete":false});
    for workers in [1, 4, 2, 8, 8, 2, 4, 1, 2, 1, 8, 4] {
        pool.control()
            .configure(WorkerConfig::Fixed { workers })
            .unwrap();
        let (sample, precision, approaches) = pooled_candidate(&pool, &sources, &retained, &model);
        assert_eq!(precision, expected_precision);
        assert_eq!(approaches, expected_approaches);
        assert_eq!(&sample["fitness"], saved);
        eprintln!("{sample}");
        report["benchmarks"].as_array_mut().unwrap().push(sample);
        save(out.join("workers.json"), &report);
    }
    // A bounded continuous replay allows the two-second controller to settle.
    // This uses real total-machine CPU readings, including concurrent builds.
    pool.control().configure(WorkerConfig::default()).unwrap();
    let started = Instant::now();
    std::thread::scope(|scope| {
        let mut load_started = false;
        while started.elapsed() < Duration::from_secs(56) {
            if !load_started && started.elapsed() >= Duration::from_secs(20) {
                load_started = true;
                let count = (pool.control().snapshot().capacity / 2).max(1).min(8);
                report["competing_load"] = json!({"threads":count,
                    "start_seconds":started.elapsed().as_secs_f64(),"end_seconds":36});
                for index in 0..count {
                    scope.spawn(move || {
                        let mut n = index as u64;
                        while started.elapsed() < Duration::from_secs(36) {
                            for _ in 0..10_000 {
                                n = std::hint::black_box(
                                    n.wrapping_mul(6364136223846793005).wrapping_add(1),
                                );
                            }
                        }
                        std::hint::black_box(n);
                    });
                }
            }
            let (mut sample, precision, approaches) =
                pooled_candidate(&pool, &sources, &retained, &model);
            assert_eq!(precision, expected_precision);
            assert_eq!(approaches, expected_approaches);
            sample["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
            eprintln!("auto: {sample}");
            report["auto_samples"].as_array_mut().unwrap().push(sample);
            save(out.join("workers.json"), &report);
        }
    });
    let (after, approaches) = candidate(&sources, &retained, &model, 4);
    assert_eq!(approaches, expected_approaches);
    report["baseline_after"] = after;
    report["complete"] = json!(true);
    report["episode_result_parity"] = json!(true);
    save(out.join("workers.json"), &report);
}
