use super::*;

fn atomic(out: &Path, name: &str, value: &impl Serialize) {
    let temporary = out.join(format!(".{name}"));
    save(&temporary, value);
    fs::rename(temporary, out.join(name)).unwrap();
}

fn observe(
    task: &RepairTask<'_>,
    model: &DenseModel,
    generation: usize,
    out: &Path,
    history: &mut Vec<Value>,
) {
    let traces = out.join(format!("generation-{generation}"));
    fs::create_dir(&traces).unwrap();
    let (case, source) = &task.sources[0];
    let result = episode(&source.lock().unwrap(), case, "model", model, Some(&traces));
    let mut fitness = score(&result);
    let precision = task.pool.map(&task.retained, |case| {
        let mut e =
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true).unwrap();
        save(traces.join(format!("precision-{}.json", case.name)), &e);
        e.trace.clear();
        e
    });
    for e in &precision {
        fitness.retained_completed += e.fitness.completed;
        fitness.retained_position_points += e.fitness.position_points;
        fitness.retained_velocity_points += e.fitness.velocity_points;
        fitness.spare_ticks += e.fitness.spare_ticks;
    }
    history.push(json!({"generation":generation,"genome":model.id(),"fitness":RepairFitness(fitness),"episode":result,"precision":precision}));
    atomic(out, "observations.json", history);
}

pub(super) fn train(
    task: &RepairTask<'_>,
    seed: &DenseModel,
    plan: &Value,
    random_seed: u64,
    root: &Path,
) {
    let out = root.join(format!("seed-{random_seed}"));
    fs::create_dir_all(&out).unwrap();
    let path = out.join("checkpoint.json");
    let configuration = config(plan, random_seed);
    let seeds = vec![seed.genome().clone(); configuration.population];
    let mut run = if path.exists() {
        EvolutionRun::load(&path, task).unwrap()
    } else {
        EvolutionRun::new(
            task,
            seed.genome().clone(),
            seeds.clone(),
            configuration.clone(),
        )
        .unwrap()
    };
    assert_eq!(run.config(), &configuration);
    assert_eq!(run.seeds(), seeds);
    run.save(&path).unwrap();
    let mut history: Vec<Value> = fs::read(out.join("observations.json"))
        .ok()
        .map(|b| serde_json::from_slice(&b).unwrap())
        .unwrap_or_default();
    for row in &history {
        let generation = row["generation"].as_u64().unwrap() as usize;
        if generation == 0 {
            assert_eq!(row["genome"], seed.id());
        } else {
            let winner = &run.history()[generation - 1];
            assert_eq!(row["genome"], winner.best_id);
            assert_eq!(row["fitness"], json!(winner.best_fitness));
        }
    }
    if history.is_empty() {
        observe(task, seed, 0, &out, &mut history);
    }
    let started = std::time::Instant::now();
    loop {
        if let Some(winner) = run.history().last() {
            let generation = run.history().len();
            let first_success = joint(winner.best_fitness)
                && !run.history()[..generation - 1]
                    .iter()
                    .any(|h| joint(h.best_fitness));
            let first_partial = winner.best_fitness.0.retained_completed > 0
                && !run.history()[..generation - 1]
                    .iter()
                    .any(|h| h.best_fitness.0.retained_completed > 0);
            if (first_success
                || first_partial
                || plan["generations_to_observe"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(generation)))
                && !history.iter().any(|r| r["generation"] == generation)
            {
                let model = DenseModel::new(winner.best_genome.clone()).unwrap();
                observe(task, &model, generation, &out, &mut history);
                assert_eq!(
                    history.last().unwrap()["fitness"],
                    json!(winner.best_fitness)
                );
            }
        }
        let before = run.history().len();
        if !run.advance(task).unwrap() {
            break;
        }
        if run.history().len() != before {
            run.save(&path).unwrap();
            eprintln!(
                "{}",
                json!({"seed":random_seed,"generation":run.history().len(),"seconds":started.elapsed().as_secs_f64(),"fitness":run.history().last().unwrap().best_fitness})
            );
        }
    }
    assert_eq!(run, EvolutionRun::load(&path, task).unwrap());
    let mut selected = Vec::new();
    if let Some(winner) = run.history().iter().find(|h| joint(h.best_fitness)) {
        save(out.join("first-joint.json"), &winner.best_genome);
        selected.push(json!({"artifact":"first-joint","generation":winner.generation+1,"genome":winner.best_id,"fitness":winner.best_fitness}));
    }
    let winner = run.history().last().unwrap();
    save(out.join("final.json"), &winner.best_genome);
    selected.push(json!({"artifact":"final","generation":winner.generation+1,"genome":winner.best_id,"fitness":winner.best_fitness}));
    atomic(
        &out,
        "selection.json",
        &json!({"selected":selected,"checkpoint_sha256":hash(&fs::read(path).unwrap())}),
    );
    save(
        out.join("timing.json"),
        &json!({"seconds":started.elapsed().as_secs_f64(),"complete":true}),
    );
}

pub(super) fn evaluate(
    isolated: &RepairTask<'_>,
    seed: &DenseModel,
    plan: &Value,
    inputs: &Value,
    root: &Path,
) {
    let out = root.join("evaluate");
    fs::create_dir(&out).unwrap();
    let reference_bytes = fs::read(root.join("reference.json")).unwrap();
    assert_eq!(hash(&reference_bytes), plan["reference"]["sha256"]);
    let reference = DenseModel::from_json(&reference_bytes).unwrap();
    assert_eq!(
        reference.id(),
        plan["reference"]["genome"].as_str().unwrap()
    );
    let mut models = vec![
        ("seed".to_string(), seed.clone()),
        ("reference".to_string(), reference),
    ];
    for random_seed in plan["evolution_seeds"].as_array().unwrap() {
        let random_seed = random_seed.as_u64().unwrap();
        let directory = root.join(format!("seed-{random_seed}"));
        let run = EvolutionRun::load(&directory.join("checkpoint.json"), isolated).unwrap();
        assert!(run.complete());
        assert_eq!(run.config(), &config(plan, random_seed));
        let selection: Value =
            serde_json::from_slice(&fs::read(directory.join("selection.json")).unwrap()).unwrap();
        assert_eq!(
            selection["checkpoint_sha256"],
            hash(&fs::read(directory.join("checkpoint.json")).unwrap())
        );
        for row in selection["selected"].as_array().unwrap() {
            let artifact = row["artifact"].as_str().unwrap();
            let model = DenseModel::from_json(
                &fs::read(directory.join(format!("{artifact}.json"))).unwrap(),
            )
            .unwrap();
            let generation = row["generation"].as_u64().unwrap() as usize;
            assert_eq!(row["genome"], model.id());
            assert_eq!(row["fitness"], json!(isolated.evaluate(&model).unwrap()));
            assert_eq!(model.id(), run.history()[generation - 1].best_id);
            if artifact == "first-joint" {
                assert_eq!(
                    generation - 1,
                    run.history()
                        .iter()
                        .position(|h| joint(h.best_fitness))
                        .unwrap()
                );
            } else {
                assert_eq!(generation, 96);
            }
            models.push((format!("seed-{random_seed}-{artifact}"), model));
        }
    }
    save(
        out.join("finalists-freeze.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(REPAIR_PLAN),"models":models.iter().map(|(label,model)|json!({"label":label,"genome":model.id()})).collect::<Vec<_>>()}),
    );
    let all = restore(inputs, "sources");
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let additional: BTreeSet<String> =
        serde_json::from_value(plan["additional_cases"].clone()).unwrap();
    let (cases, diagnostic): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|(c, _)| protected.contains(&c.name) || additional.contains(&c.name));
    let before = records(&cases);
    let diagnostic_before = records(&diagnostic);
    let task = TrainingTask {
        sources: &cases,
        retained: retention_cases(plan),
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: isolated.manifest(),
    };
    let search = Search {
        task: &task,
        protected,
    };
    assert_eq!(
        (cases.len(), diagnostic.len(), task.retained.len()),
        (474, 8, 157)
    );
    let mut reports = Vec::new();
    for (label, model) in &models {
        let mut report = focused::model_report(&search, plan, None, label, "model", model, &out);
        let precision = task.pool.map(&task.retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false).unwrap()
        });
        report["precision_retention"] = json!(precision);
        let repair_traces = out.join(format!("{label}-precision"));
        fs::create_dir(&repair_traces).unwrap();
        for case in &isolated.retained {
            let e = ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true)
                .unwrap();
            save(repair_traces.join(format!("{}.json", case.name)), &e);
        }
        let traces = out.join(format!("{label}-diagnostic"));
        fs::create_dir(&traces).unwrap();
        let episodes = task.pool.map(&diagnostic, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, Some(&traces))
        });
        report["diagnostic"] = json!(episodes);
        report["repair_fitness"] = json!(isolated.evaluate(model).unwrap());
        if label == "seed" || label == "reference" {
            let expected = if label == "seed" {
                "expected"
            } else {
                "reference"
            };
            let actual: Vec<_> = report["approaches"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|g| g["episodes"].as_array().unwrap())
                .chain(report["diagnostic"].as_array().unwrap())
                .collect();
            for e in actual {
                let old = inputs[expected]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["case"] == e["case"])
                    .unwrap();
                assert_eq!(&parsed(e.clone()), old);
            }
        }
        let retained = precision
            .iter()
            .filter(|e| e.fitness.completed == 1)
            .count();
        eprintln!("{label}: {retained}/157 precision retained");
        assert_eq!(records(&cases), before);
        assert_eq!(records(&diagnostic), diagnostic_before);
        reports.push(report);
        atomic(
            &out,
            "results.json",
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(REPAIR_PLAN),"controllers":reports,"production_controls_changed":false,"fresh_generalization_test":false}),
        );
    }
}
