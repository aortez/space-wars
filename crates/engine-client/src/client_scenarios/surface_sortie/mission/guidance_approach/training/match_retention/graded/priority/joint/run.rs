use super::*;

fn atomic(out: &Path, name: &str, value: &impl Serialize) {
    let temporary = out.join(format!(".{name}"));
    save(&temporary, value);
    fs::rename(temporary, out.join(name)).unwrap();
}

fn observe(
    task: &JointTask<'_>,
    model: &DenseModel,
    generation: usize,
    plan: &Value,
    out: &Path,
    history: &mut Vec<Value>,
) {
    let traces = out.join(format!("generation-{generation}"));
    fs::create_dir(&traces).unwrap();
    save(traces.join("model.json"), model.genome());
    let fitness = task.evaluate(model).unwrap();
    let selected: BTreeSet<String> =
        serde_json::from_value(plan["match_replay_cases"].clone()).unwrap();
    let cases: Vec<_> = task
        .base
        .sources
        .iter()
        .filter(|(case, _)| selected.contains(&case.name))
        .collect();
    assert_eq!(cases.len(), selected.len());
    let episodes = task.base.pool.map(&cases, |(case, source)| {
        episode(&source.lock().unwrap(), case, "model", model, Some(&traces))
    });
    let selected: Vec<ship_arrival::ArrivalCase> =
        serde_json::from_value(plan["precision_replay_cases"].clone()).unwrap();
    let precision = task.base.pool.map(&selected, |case| {
        let mut e =
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true).unwrap();
        save(traces.join(format!("precision-{}.json", case.name)), &e);
        e.trace.clear();
        e
    });
    history.push(json!({"generation":generation,"genome":model.id(),"fitness":fitness,"episodes":episodes,"precision":precision}));
    atomic(out, "observations.json", history);
}

pub(super) fn train(task: &JointTask<'_>, seed: &DenseModel, plan: &Value, arm: &str, root: &Path) {
    let out = root.join(arm);
    fs::create_dir_all(&out).unwrap();
    let path = out.join("checkpoint.json");
    let configuration = configuration(plan);
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
        observe(task, seed, 0, plan, &out, &mut history);
    }
    let started = std::time::Instant::now();
    loop {
        if let Some(winner) = run.history().last() {
            let generation = run.history().len();
            let first_success = eligible(winner.best_fitness)
                && !run.history()[..generation - 1]
                    .iter()
                    .any(|h| eligible(h.best_fitness));
            assert_eq!(winner.best_fitness.focal_completed, 2);
            if (first_success
                || plan["generations_to_observe"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(generation)))
                && !history.iter().any(|r| r["generation"] == generation)
            {
                let model = DenseModel::new(winner.best_genome.clone()).unwrap();
                observe(task, &model, generation, plan, &out, &mut history);
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
        run.save(&path).unwrap();
        if run.history().len() != before {
            save(
                out.join("workers.json"),
                &task.base.pool.control().snapshot(),
            );
            eprintln!(
                "{}",
                json!({"arm":arm,"generation":run.history().len(),"seconds":started.elapsed().as_secs_f64(),"fitness":run.history().last().unwrap().best_fitness})
            );
        }
    }
    assert_eq!(run, EvolutionRun::load(&path, task).unwrap());
    let mut selected = Vec::new();
    if let Some(winner) = run.history().iter().find(|h| eligible(h.best_fitness)) {
        save(out.join("first-full.json"), &winner.best_genome);
        selected.push(json!({"artifact":"first-full","generation":winner.generation+1,"genome":winner.best_id,"fitness":winner.best_fitness}));
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

fn report(
    task: &JointTask<'_>,
    excluded: &[Source],
    model: &DenseModel,
    label: &str,
    plan: &Value,
    out: &Path,
) -> Value {
    let before = records(task.base.sources);
    let excluded_before = records(excluded);
    let mut reconstructed = JointFitness::default();
    let included: BTreeSet<_> = task
        .base
        .sources
        .iter()
        .map(|(c, _)| c.name.as_str())
        .collect();
    let mut groups = Vec::new();
    for partition in ["protected", "additional", "diagnostic"] {
        let names: BTreeSet<String> =
            serde_json::from_value(plan[format!("{partition}_cases")].clone()).unwrap();
        let cases: Vec<_> = task
            .base
            .sources
            .iter()
            .chain(excluded)
            .filter(|(case, _)| names.contains(&case.name))
            .collect();
        assert_eq!(cases.len(), names.len());
        let traces = out.join(format!("{label}-{partition}"));
        fs::create_dir(&traces).unwrap();
        let episodes = task.base.pool.map(&cases, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, Some(&traces))
        });
        for e in &episodes {
            if included.contains(e.case.as_str()) {
                reconstructed.approach(e, task.focal.contains(&e.case));
            }
        }
        let arrived = episodes
            .iter()
            .filter(|e| e.outcome == Outcome::Arrived)
            .count();
        groups.push(json!({"partition":partition,"arrived":arrived,"episodes":episodes}));
        eprintln!("{label} {partition}: {arrived}/{}", cases.len());
    }
    let precision = task.base.pool.map(&task.base.retained, |case| {
        ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false).unwrap()
    });
    let traces = out.join(format!("{label}-precision"));
    fs::create_dir(&traces).unwrap();
    let selected: Vec<ship_arrival::ArrivalCase> =
        serde_json::from_value(plan["precision_replay_cases"].clone()).unwrap();
    for case in selected {
        let mut e =
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true).unwrap();
        save(traces.join(format!("{}.json", case.name)), &e);
        e.trace.clear();
        assert_eq!(&e, precision.iter().find(|r| r.case == case).unwrap());
    }
    let fitness = task.evaluate(model).unwrap();
    for e in &precision {
        reconstructed.precision(e.fitness);
    }
    assert_eq!(fitness, reconstructed);
    assert_eq!(records(task.base.sources), before);
    assert_eq!(records(excluded), excluded_before);
    eprintln!(
        "{label}: {}/157 precision, {}/619 retained, {}/2 focal",
        fitness.precision_completed, fitness.retained_completed, fitness.focal_completed
    );
    json!({"controller":label,"genome":model.id(),"groups":groups,"precision_retention":precision,"fitness":fitness,"eligible":eligible(fitness)})
}

pub(super) fn preflight(
    training: &[Source],
    excluded: &[Source],
    plan: &Value,
    inputs: &Value,
    initial: &[Value],
    root: &Path,
) {
    let out = root.join("preflight");
    fs::create_dir(&out).unwrap();
    let task = task(training, plan, "preflight");
    let prior = Path::new(plan["prior_directory"].as_str().unwrap());
    let mut reports = Vec::new();
    for name in ["balanced", "precision", "reference"] {
        let model = model(root, plan, name);
        let result = report(&task, excluded, &model, name, plan, &out);
        let old = &inputs["expected"][name];
        for group in result["groups"].as_array().unwrap() {
            let partition = group["partition"].as_str().unwrap();
            let expected = if partition == "diagnostic" {
                &old["diagnostic"]
            } else {
                &old["approaches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|g| g["partition"] == partition)
                    .unwrap()["episodes"]
            };
            for e in group["episodes"].as_array().unwrap() {
                let name = e["case"].as_str().unwrap();
                assert_eq!(
                    &parsed(e.clone()),
                    expected
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|r| r["case"] == name)
                        .unwrap()
                );
                let file = format!("{name}-model.jsonl");
                let previous =
                    plan["models"][result["controller"].as_str().unwrap()]["prior_controller"]
                        .as_str()
                        .unwrap();
                assert_eq!(
                    fs::read(out.join(format!(
                        "{}-{partition}/{file}",
                        result["controller"].as_str().unwrap()
                    )))
                    .unwrap(),
                    fs::read(prior.join(format!("evaluate/{previous}-{partition}/{file}")))
                        .unwrap()
                );
            }
        }
        for e in result["precision_retention"].as_array().unwrap() {
            let mut expected = old["precision_retention"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["case"] == e["case"])
                .unwrap()
                .clone();
            expected["runtime"] = json!(engine_agent::evolution::runtime_id().unwrap());
            assert_eq!(parsed(e.clone()), expected);
        }
        reports.push(result);
    }
    save(
        out.join("preflight.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(JOINT_PLAN),"sources":initial,"controllers":reports,"byte_parity":true}),
    );
}

pub(super) fn evaluate(training: &[Source], excluded: &[Source], plan: &Value, root: &Path) {
    let out = root.join("evaluate");
    fs::create_dir(&out).unwrap();
    let mut models = vec![
        ("balanced-seed".to_string(), model(root, plan, "balanced")),
        ("precision-seed".to_string(), model(root, plan, "precision")),
        ("reference".to_string(), model(root, plan, "reference")),
    ];
    for arm in plan["arms"].as_array().unwrap() {
        let arm = arm.as_str().unwrap();
        let task = task(training, plan, arm);
        let directory = root.join(arm);
        let run = EvolutionRun::load(&directory.join("checkpoint.json"), &task).unwrap();
        assert!(run.complete());
        assert_eq!(run.config(), &configuration(plan));
        assert_eq!(
            run.seeds(),
            vec![model(root, plan, arm).genome().clone(); 16]
        );
        let selection: Value =
            serde_json::from_slice(&fs::read(directory.join("selection.json")).unwrap()).unwrap();
        assert_eq!(
            selection["checkpoint_sha256"],
            hash(&fs::read(directory.join("checkpoint.json")).unwrap())
        );
        let first = run.history().iter().position(|h| eligible(h.best_fitness));
        assert_eq!(
            selection["selected"].as_array().unwrap().len(),
            if first.is_some() { 2 } else { 1 }
        );
        for row in selection["selected"].as_array().unwrap() {
            let artifact = row["artifact"].as_str().unwrap();
            let generation = row["generation"].as_u64().unwrap() as usize;
            assert_eq!(
                generation,
                if artifact == "first-full" {
                    first.unwrap() + 1
                } else {
                    48
                }
            );
            let model = DenseModel::from_json(
                &fs::read(directory.join(format!("{artifact}.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(model.id(), row["genome"].as_str().unwrap());
            assert_eq!(model.id(), run.history()[generation - 1].best_id);
            assert_eq!(
                task.evaluate(&model).unwrap(),
                run.history()[generation - 1].best_fitness
            );
            assert_eq!(
                json!(run.history()[generation - 1].best_fitness),
                row["fitness"]
            );
            models.push((format!("{arm}-{artifact}"), model));
        }
    }
    save(
        out.join("finalists-freeze.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(JOINT_PLAN),"models":models.iter().map(|(label,m)|json!({"label":label,"genome":m.id()})).collect::<Vec<_>>()}),
    );
    let task = task(training, plan, "evaluate");
    let mut reports = Vec::new();
    for (label, model) in models {
        reports.push(report(&task, excluded, &model, &label, plan, &out));
        atomic(
            &out,
            "results.json",
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(JOINT_PLAN),"controllers":reports,"production_controls_changed":false,"fresh_generalization_test":false}),
        );
    }
}
