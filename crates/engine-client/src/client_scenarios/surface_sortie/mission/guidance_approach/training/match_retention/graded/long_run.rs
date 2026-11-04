//! Increase the graded run's budget without changing its training evaluator.
use super::*;

const LONG_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-graded-long-v1-plan.json"
));

fn frozen_sources(receipt: &Value, field: &str) -> Vec<Source> {
    sources(
        receipt[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| serde_json::from_value(r["case"].clone()).unwrap())
            .collect(),
    )
}

#[test]
#[ignore = "240-generation graded comparison; explicit mode and fresh output required"]
fn learned_guidance_graded_long() {
    let plan: Value = serde_json::from_slice(LONG_PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_LONG_MODE").unwrap();
    assert!(["preflight", "train", "resume", "evaluate"].contains(&mode.as_str()));
    let root = std::env::var_os("SPACEWARS_LONG_ROOT").unwrap();
    let root = Path::new(&root);
    let out = root.join(match mode.as_str() {
        "train" | "resume" => "run",
        _ => &mode,
    });
    if mode != "resume" {
        fs::create_dir(&out).expect("retain earlier evidence");
    }
    let baseline_bytes = fs::read(root.join("baseline.json")).unwrap();
    assert_eq!(hash(&baseline_bytes), plan["baseline_file_sha256"]);
    let baseline = DenseModel::from_json(&baseline_bytes).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"].as_str().unwrap());
    let previous_bytes = fs::read(root.join("previous-preflight.json")).unwrap();
    assert_eq!(hash(&previous_bytes), plan["previous_preflight_sha256"]);
    let previous: Value = serde_json::from_slice(&previous_bytes).unwrap();
    let training = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    let inspected = frozen_sources(&previous, "validation");
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let probes = brake_probes(&training, &protected);
    let retained = retention_cases(&plan);
    let readback =
        |v: &Value| serde_json::from_slice::<Value>(&serde_json::to_vec(v).unwrap()).unwrap();
    assert_eq!(readback(&json!(records(&training))), previous["training"]);
    assert_eq!(
        readback(&json!(records(&inspected))),
        previous["validation"]
    );
    assert_eq!(readback(&json!(retained)), previous["retention_cases"]);
    assert_eq!(readback(&json!(probes)), previous["brake_probes"]);
    let expected =
        (mode != "preflight").then(|| fs::read(root.join("preflight/preflight.json")).unwrap());
    let (reserved, discovery) = if let Some(bytes) = &expected {
        let receipt: Value = serde_json::from_slice(bytes).unwrap();
        (
            frozen_sources(&receipt, "validation"),
            receipt["validation_discovery"].clone(),
        )
    } else {
        focused::discover(&plan)
    };
    assert_eq!(
        reserved.len(),
        16,
        "freeze four complete worlds before training"
    );
    let receipt = serde_json::to_vec_pretty(&json!({
        "runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(LONG_PLAN),"baseline":baseline.id(),
        "training":records(&training),"protected_cases":protected,
        "retention_cases":retained,"brake_probes":probes,
        "inspected":records(&inspected),"validation":records(&reserved),
        "validation_discovery":discovery
    }))
    .unwrap();
    if let Some(expected) = expected {
        assert_eq!(
            receipt, expected,
            "frozen physical sources or runtime changed"
        );
    }
    fs::write(out.join("plan.json"), LONG_PLAN).unwrap();
    fs::write(out.join("preflight.json"), &receipt).unwrap();
    if mode == "preflight" {
        return;
    }
    let task = TrainingTask {
        sources: &training,
        retained,
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: TaskManifest {
            id: plan["experiment"].as_str().unwrap().into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"plan_utf8":std::str::from_utf8(LONG_PLAN).unwrap(),
                "preflight_sha256":hash(&receipt)}),
        },
    };
    let search = Search {
        task: &task,
        protected,
    };
    let graded = Guided {
        search: &search,
        probes: &probes,
        arm: Arm::Graded,
    };
    if mode != "evaluate" {
        train(
            &graded,
            &[baseline],
            &plan,
            &out,
            mode == "resume",
            &task.pool,
        );
        return;
    }
    let run = EvolutionRun::load(&root.join("run/checkpoint.json"), &graded).unwrap();
    assert!(
        run.complete(),
        "freeze final generation before fresh validation"
    );
    assert_eq!(run.config(), &configuration(&plan));
    let winner = run.history().last().unwrap();
    let long = DenseModel::new(winner.best_genome.clone()).unwrap();
    let short_bytes = fs::read(root.join("short.json")).unwrap();
    assert_eq!(hash(&short_bytes), plan["short_file_sha256"]);
    let short = DenseModel::from_json(&short_bytes).unwrap();
    assert_eq!(short.id(), plan["short_genome"].as_str().unwrap());
    assert_eq!(graded.evaluate(&long).unwrap(), winner.best_fitness);
    assert_eq!(
        graded.evaluate(&short).unwrap(),
        run.history()[23].best_fitness
    );
    let mut reports = Vec::new();
    for (label, model) in [("short", &short), ("long", &long)] {
        let mut report =
            focused::model_report(&search, &plan, Some(&reserved), label, "model", model, &out);
        let traces = out.join(format!("{label}-inspected"));
        fs::create_dir(&traces).unwrap();
        let episodes = task.pool.map(&inspected, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, Some(&traces))
        });
        let arrived = episodes
            .iter()
            .filter(|e| e.outcome == Outcome::Arrived)
            .count();
        report["inspected"] = json!({"arrived":arrived,"episodes":episodes});
        report["passed"] = json!(report["passed"] == true && arrived == inspected.len());
        report["fitness"] = json!(graded.evaluate(model).unwrap());
        reports.push(report);
        save(
            out.join("results.json"),
            &json!({
                "runtime":engine_agent::evolution::runtime_id().unwrap(),
                "plan_sha256":hash(LONG_PLAN),"controllers":reports,
                "production_controls_changed":false
            }),
        );
    }
}

#[test]
fn long_recipe_preserves_training_except_generation_budget() {
    let original: Value = serde_json::from_slice(PLAN).unwrap();
    let long: Value = serde_json::from_slice(LONG_PLAN).unwrap();
    let mut config = configuration(&long);
    assert_eq!(config.generations, 240);
    config.generations = 24;
    assert_eq!(config, configuration(&original));
    for key in [
        "baseline_genome",
        "baseline_file_sha256",
        "training_cases",
        "protected_cases",
        "additional_cases",
        "retention_partitions",
        "retention_deduplicate",
        "brake_progress",
        "fitness_order",
        "precision_traces",
    ] {
        assert_eq!(long[key], original[key], "changed training field: {key}");
    }
    let inspected: BTreeSet<_> = original["training_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["seed"].as_u64().unwrap())
        .chain(
            original["validation_seeds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap()),
        )
        .collect();
    assert!(
        long["validation_seeds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| !inspected.contains(&v.as_u64().unwrap()))
    );
    assert_eq!(retention_cases(&long).len(), 157);
    assert_eq!(long["budget"]["training_episodes"], 16 * 240 * 255);
    assert_eq!(hash(PLAN), long["previous_plan_sha256"]);
}
