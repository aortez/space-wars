//! Broader arrival coverage with separate monitoring and final evaluation worlds.
use super::*;

const COVERAGE_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-coverage-v1-plan.json"
));

fn restore(receipt: &Value, field: &str) -> Vec<Source> {
    let cases = receipt[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["case"].clone()).unwrap())
        .collect();
    let restored = sources(cases);
    let actual: Value =
        serde_json::from_slice(&serde_json::to_vec(&records(&restored)).unwrap()).unwrap();
    assert_eq!(actual, receipt[field], "physical sources changed: {field}");
    restored
}

fn discover_cohort(
    plan: &Value,
    cohort: &str,
    model: &DenseModel,
    pool: &EpisodePool,
) -> (Vec<Source>, Value) {
    let config = &plan["cohorts"][cohort];
    let worlds = config["validation_worlds"].as_u64().unwrap() as usize;
    let mut selected = Vec::new();
    let mut audit = Vec::new();
    for seed in config["validation_seeds"].as_array().unwrap() {
        let mut one = plan.clone();
        one["validation_seeds"] = json!([seed]);
        let (cases, missing) = validation(&one);
        let complete = missing.is_empty()
            && focused::complete_world(
                &cases.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>(),
                seed.as_u64().unwrap(),
            );
        let native = pool.map(&cases, |(case, source)| {
            episode(&source.lock().unwrap(), case, "mission", model, None)
        });
        let accepted = complete
            && (config["require_native_arrival"] != true
                || native.iter().all(|e| e.outcome == Outcome::Arrived));
        audit.push(
            json!({"seed":seed,"accepted":accepted,"complete_world":complete,
            "available":records(&cases),"missing":missing,"native":native}),
        );
        if accepted {
            selected.extend(cases);
        }
        if selected.len() == worlds * 4 {
            break;
        }
    }
    assert_eq!(
        selected.len(),
        worlds * 4,
        "insufficient frozen reserve: {cohort}"
    );
    (selected, json!(audit))
}

fn monitored_train(
    task: &Guided<'_, '_, '_>,
    model: &DenseModel,
    plan: &Value,
    monitoring: &[Source],
    out: &Path,
    resume: bool,
) {
    let pool = &task.search.task.pool;
    let path = out.join("checkpoint.json");
    let mut run = if resume {
        EvolutionRun::load(&path, task).unwrap()
    } else {
        EvolutionRun::new(
            task,
            model.genome().clone(),
            vec![model.genome().clone()],
            configuration(plan),
        )
        .unwrap()
    };
    assert_eq!(run.config(), &configuration(plan));
    assert_eq!(run.seeds(), [model.genome().clone()]);
    let old_path = Path::new(plan["prior_directory"].as_str().unwrap()).join("run/checkpoint.json");
    let previous: Value = serde_json::from_slice(&fs::read(old_path).unwrap()).unwrap();
    let monitor_path = out.join("monitoring.json");
    let mut history: Vec<Value> = if monitor_path.exists() {
        serde_json::from_slice(&fs::read(&monitor_path).unwrap()).unwrap()
    } else {
        Vec::new()
    };
    let started = std::time::Instant::now();
    let initial_generations = run.history().len();
    let mut monitor_seconds = 0.0;
    loop {
        // Replay a due observation before advancing, including after a restart
        // between the completed-generation save and its monitoring receipt.
        if let Some(winner) = run.history().last() {
            let generation = run.history().len();
            if plan["monitor_generations"]
                .as_array()
                .unwrap()
                .contains(&json!(generation))
                && !history.iter().any(|h| h["generation"] == generation)
            {
                let begin = std::time::Instant::now();
                let selected = DenseModel::new(winner.best_genome.clone()).unwrap();
                let traces = plan["monitor_trace_generations"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(generation))
                    .then(|| out.join(format!("monitor-generation-{generation}")));
                if let Some(traces) = &traces {
                    fs::create_dir_all(traces).unwrap();
                }
                let episodes = pool.map(monitoring, |(case, source)| {
                    episode(
                        &source.lock().unwrap(),
                        case,
                        "model",
                        &selected,
                        traces.as_deref(),
                    )
                });
                let arrived = episodes
                    .iter()
                    .filter(|e| e.outcome == Outcome::Arrived)
                    .count();
                history.push(json!({"generation":generation,"genome":selected.id(),
                    "training_fitness":winner.best_fitness,"arrived":arrived,"episodes":episodes}));
                let temporary = out.join(".monitoring.json");
                save(&temporary, &history);
                fs::rename(temporary, &monitor_path).unwrap();
                monitor_seconds += begin.elapsed().as_secs_f64();
                eprintln!(
                    "monitor generation {generation}: {arrived}/{}",
                    monitoring.len()
                );
            }
        }
        if !run.advance(task).unwrap() {
            break;
        }
        run.save(&path).unwrap();
        save(out.join("workers.json"), &pool.control().snapshot());
        if run.complete() || run.fitness().iter().all(Option::is_none) {
            if run.history().len() == 1 {
                assert_eq!(
                    json!(run.history()[0].population_ids),
                    previous["run"]["history"][0]["population_ids"],
                    "initial population differs from the retained control"
                );
                save(
                    out.join("initial-population-verification.json"),
                    &json!({"identical":true,
                    "population":run.history()[0].population_ids}),
                );
            }
            eprintln!(
                "{}",
                json!({"generations":run.history().len(),"complete":run.complete(),
                "seconds":started.elapsed().as_secs_f64(),"best":run.history().last().unwrap().best_fitness})
            );
        }
    }
    assert_eq!(run, EvolutionRun::load(&path, task).unwrap());
    let winner = run.history().last().unwrap();
    let selected = DenseModel::new(winner.best_genome.clone()).unwrap();
    save(out.join("finalist.json"), selected.genome());
    save(
        out.join("selection.json"),
        &json!({"genome":selected.id(),"generation":run.history().len(),
        "fitness":winner.best_fitness,"checkpoint_sha256":hash(&fs::read(path).unwrap())}),
    );
    save(
        out.join("timing.json"),
        &json!({"initial_generations":initial_generations,
        "completed_generations":run.history().len(),"complete":run.complete(),
        "evaluation_save_and_monitor_seconds":started.elapsed().as_secs_f64(),"monitor_seconds":monitor_seconds}),
    );
}

#[test]
#[ignore = "broader 240-generation comparison; explicit mode and fresh output required"]
fn learned_guidance_coverage() {
    let plan: Value = serde_json::from_slice(COVERAGE_PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_COVERAGE_MODE").unwrap();
    assert!(["preflight", "train", "resume", "evaluate"].contains(&mode.as_str()));
    let root = std::env::var_os("SPACEWARS_COVERAGE_ROOT").unwrap();
    let root = Path::new(&root);
    let out = root.join(if mode == "train" || mode == "resume" {
        "run"
    } else {
        &mode
    });
    if mode != "resume" {
        fs::create_dir(&out).expect("retain earlier evidence");
    }
    let prior = Path::new(plan["prior_directory"].as_str().unwrap());
    for (name, digest) in plan["prior_files"].as_object().unwrap() {
        assert_eq!(hash(&fs::read(prior.join(name)).unwrap()), *digest);
    }
    let baseline = DenseModel::from_json(&fs::read(prior.join("baseline.json")).unwrap()).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"].as_str().unwrap());
    let original: Value =
        serde_json::from_slice(&fs::read(prior.join("preflight/preflight.json")).unwrap()).unwrap();
    let original_plan: Value =
        serde_json::from_slice(&fs::read(prior.join("plan.json")).unwrap()).unwrap();
    let old_protected = serde_json::from_value(original_plan["protected_cases"].clone()).unwrap();
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["known_protected_cases"].clone()).unwrap();
    let mut known = restore(&original, "training");
    let probes = brake_probes(&known, &old_protected);
    assert_eq!(
        serde_json::from_slice::<Value>(&serde_json::to_vec(&json!(probes)).unwrap()).unwrap(),
        original["brake_probes"]
    );
    known.extend(restore(&original, "inspected"));
    known.extend(restore(&original, "validation"));
    assert_eq!(known.len(), 130);
    let pool = EpisodePool::new(worker_config()).unwrap();
    let receipt_path = root.join("preflight/preflight.json");
    if mode == "preflight" {
        let mut receipt = json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(COVERAGE_PLAN),"known":records(&known),"brake_probes":probes,
            "retention_cases":retention_cases(&plan)});
        for cohort in ["expansion", "monitoring", "qualification"] {
            let (selected, discovery) = discover_cohort(&plan, cohort, &baseline, &pool);
            receipt[cohort] = json!(records(&selected));
            receipt[format!("{cohort}_discovery")] = discovery;
            eprintln!("frozen {cohort}: {} starts", selected.len());
        }
        save(receipt_path, &receipt);
        return;
    }
    let receipt_bytes = fs::read(&receipt_path).unwrap();
    let receipt: Value = serde_json::from_slice(&receipt_bytes).unwrap();
    assert_eq!(
        receipt["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(receipt["plan_sha256"], hash(COVERAGE_PLAN));
    assert_eq!(
        serde_json::from_slice::<Value>(&serde_json::to_vec(&records(&known)).unwrap()).unwrap(),
        receipt["known"]
    );
    let monitoring = restore(&receipt, "monitoring");
    let mut host_cases = Vec::new();
    let mut training = Vec::new();
    for source in known {
        if protected.contains(&source.0.name) {
            training.push(source);
        } else {
            host_cases.push(source);
        }
    }
    assert_eq!(training.len(), 128);
    assert_eq!(host_cases.len(), 2);
    training.extend(restore(&receipt, "expansion"));
    let task = TrainingTask {
        sources: &training,
        retained: retention_cases(&plan),
        pool,
        manifest: TaskManifest {
            id: plan["experiment"].as_str().unwrap().into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({
                "plan_utf8":std::str::from_utf8(COVERAGE_PLAN).unwrap(),"preflight_sha256":hash(&receipt_bytes)}),
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
        monitored_train(
            &graded,
            &baseline,
            &plan,
            &monitoring,
            &out,
            mode == "resume",
        );
        return;
    }
    let run = EvolutionRun::load(&root.join("run/checkpoint.json"), &graded).unwrap();
    assert!(run.complete());
    let winner = run.history().last().unwrap();
    let final_model = DenseModel::new(winner.best_genome.clone()).unwrap();
    assert_eq!(graded.evaluate(&final_model).unwrap(), winner.best_fitness);
    let previous: Value =
        serde_json::from_slice(&fs::read(prior.join("run/checkpoint.json")).unwrap()).unwrap();
    let mut models = plan["comparators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|generation| {
            let row = &previous["run"]["history"][generation.as_u64().unwrap() as usize - 1];
            let model =
                DenseModel::new(serde_json::from_value(row["best_genome"].clone()).unwrap())
                    .unwrap();
            assert_eq!(model.id(), row["best_id"].as_str().unwrap());
            (format!("original-{}", generation.as_u64().unwrap()), model)
        })
        .collect::<Vec<_>>();
    models.push(("coverage-240".into(), final_model));
    save(
        out.join("finalists-freeze.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(COVERAGE_PLAN),"models":models.iter().map(|(label, model)|json!({"label":label,"genome":model.id()})).collect::<Vec<_>>(),
        "checkpoint_sha256":hash(&fs::read(root.join("run/checkpoint.json")).unwrap())}),
    );
    let qualification = restore(&receipt, "qualification");
    let mut reports = Vec::new();
    for (label, model) in &models {
        // Without a validation set this helper evaluates the training groups and
        // precision endpoints; the three remaining groups are reported explicitly.
        let mut report = focused::model_report(&search, &plan, None, label, "model", model, &out);
        report["settling"] = json!(full_retention::settling(
            &ship_curriculum::evaluate(Policy::Model(model.clone())).unwrap(),
            &plan,
            model,
            "model",
            label,
            &out
        ));
        let mut groups = Vec::new();
        for (group, cases) in [
            ("monitoring", &monitoring),
            ("qualification", &qualification),
            ("host-diagnostic", &host_cases),
        ] {
            let traces = out.join(format!("{label}-{group}"));
            fs::create_dir(&traces).unwrap();
            let episodes = task.pool.map(cases, |(case, source)| {
                episode(&source.lock().unwrap(), case, "model", model, Some(&traces))
            });
            let arrived = episodes
                .iter()
                .filter(|e| e.outcome == Outcome::Arrived)
                .count();
            eprintln!("{label} {group}: {arrived}/{}", cases.len());
            groups.push(json!({"partition":group,"arrived":arrived,"episodes":episodes}));
        }
        report["evaluation"] = json!(groups);
        report["fitness"] = json!(graded.evaluate(model).unwrap());
        report["live_integration_qualified"] = json!(false);
        reports.push(report);
        save(
            out.join("results.json"),
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(COVERAGE_PLAN),"controllers":reports,"production_controls_changed":false}),
        );
    }
}
