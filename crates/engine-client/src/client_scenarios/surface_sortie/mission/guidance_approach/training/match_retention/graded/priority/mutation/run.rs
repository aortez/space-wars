use super::*;

fn atomic(out: &Path, name: &str, value: &impl Serialize) {
    let temporary = out.join(format!(".{name}"));
    save(&temporary, value);
    fs::rename(temporary, out.join(name)).unwrap();
}

fn observe(
    task: &MutationTask<'_, '_, '_>,
    model: &DenseModel,
    fitness: GradedFitness,
    generation: usize,
    plan: &Value,
    validation: &[Source],
    out: &Path,
    history: &mut Vec<Value>,
) {
    assert!(eligible(fitness, plan), "winner failed parent retention");
    if generation == 0 {
        let receipt: Value = serde_json::from_slice(
            &fs::read(out.parent().unwrap().join("preflight/preflight.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            json!(fitness),
            receipt["seed_fitness"],
            "seed fitness changed from preflight"
        );
    }
    let traces = out.join(format!("validation-generation-{generation}"));
    fs::create_dir_all(&traces).unwrap();
    let episodes = task
        .guided
        .search
        .task
        .pool
        .map(validation, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, Some(&traces))
        });
    let arrived = episodes
        .iter()
        .filter(|e| e.outcome == Outcome::Arrived)
        .count();
    save(
        out.join(format!("generation-{generation}.json")),
        model.genome(),
    );
    history.push(json!({"generation":generation,"genome":model.id(),"training_fitness":fitness,"arrived":arrived,"episodes":episodes}));
    atomic(out, "validation.json", history);
    eprintln!(
        "{} validation generation {generation}: {arrived}/{}",
        task.name,
        validation.len()
    );
}

pub(super) fn train(
    task: &MutationTask<'_, '_, '_>,
    parent: &DenseModel,
    plan: &Value,
    validation: &[Source],
    root: &Path,
    resume: bool,
) {
    let out = root.join(task.name);
    if !resume {
        fs::create_dir(&out).unwrap();
    }
    let path = out.join("checkpoint.json");
    let mut run = if resume {
        EvolutionRun::load(&path, task).unwrap()
    } else {
        EvolutionRun::new(
            task,
            parent.genome().clone(),
            vec![parent.genome().clone()],
            arm_configuration(plan, task.name),
        )
        .unwrap()
    };
    assert_eq!(run.config(), &arm_configuration(plan, task.name));
    assert_eq!(run.seeds(), [parent.genome().clone()]);
    run.save(&path).unwrap();
    let monitor = out.join("validation.json");
    let mut history: Vec<Value> = if monitor.exists() {
        serde_json::from_slice(&fs::read(monitor).unwrap()).unwrap()
    } else {
        Vec::new()
    };
    // Every observation is tied to the exact saved training winner. A completed
    // generation lacking its receipt is observed before advancing on resume.
    for row in &history {
        let generation = row["generation"].as_u64().unwrap() as usize;
        assert!(
            plan["validation_generations"]
                .as_array()
                .unwrap()
                .contains(&json!(generation))
        );
        if generation == 0 {
            assert_eq!(row["genome"], parent.id());
        } else {
            let winner = &run.history()[generation - 1];
            assert_eq!(row["genome"], winner.best_id);
            assert_eq!(row["training_fitness"], json!(winner.best_fitness));
        }
    }
    let started = std::time::Instant::now();
    if history.is_empty() {
        observe(
            task,
            parent,
            task.evaluate(parent).unwrap().0,
            0,
            plan,
            validation,
            &out,
            &mut history,
        );
    }
    loop {
        if let Some(winner) = run.history().last() {
            let generation = run.history().len();
            assert!(eligible(winner.best_fitness.0, plan));
            if plan["validation_generations"]
                .as_array()
                .unwrap()
                .contains(&json!(generation))
                && !history.iter().any(|row| row["generation"] == generation)
            {
                let model = DenseModel::new(winner.best_genome.clone()).unwrap();
                observe(
                    task,
                    &model,
                    winner.best_fitness.0,
                    generation,
                    plan,
                    validation,
                    &out,
                    &mut history,
                );
            }
        }
        if !run.advance(task).unwrap() {
            break;
        }
        run.save(&path).unwrap();
        save(
            out.join("workers.json"),
            &task.guided.search.task.pool.control().snapshot(),
        );
        if run.complete() || run.fitness().iter().all(Option::is_none) {
            eprintln!(
                "{}",
                json!({"arm":task.name,"generations":run.history().len(),"seconds":started.elapsed().as_secs_f64(),"best":run.history().last().unwrap().best_fitness})
            );
        }
    }
    assert_eq!(run, EvolutionRun::load(&path, task).unwrap());
    assert_eq!(
        history.len(),
        plan["validation_generations"].as_array().unwrap().len()
    );
    let selected = &history[choose(&history, plan)];
    let generation = selected["generation"].as_u64().unwrap();
    let chosen = DenseModel::from_json(
        &fs::read(out.join(format!("generation-{generation}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(chosen.id(), selected["genome"].as_str().unwrap());
    save(out.join("selected.json"), chosen.genome());
    save(
        out.join("fixed-final.json"),
        &run.history().last().unwrap().best_genome,
    );
    atomic(
        &out,
        "selection.json",
        &json!({"selected":selected,"checkpoint_sha256":hash(&fs::read(path).unwrap()),
        "fixed_final_genome":run.history().last().unwrap().best_id,"selection_rule":plan["selection"]}),
    );
    save(
        out.join("timing.json"),
        &json!({"seconds":started.elapsed().as_secs_f64(),"complete":run.complete()}),
    );
}

fn finalists(
    task: &MutationTask<'_, '_, '_>,
    plan: &Value,
    root: &Path,
) -> Vec<(String, DenseModel)> {
    let out = root.join(task.name);
    let run = EvolutionRun::load(&out.join("checkpoint.json"), task).unwrap();
    assert!(run.complete());
    assert_eq!(run.config(), &arm_configuration(plan, task.name));
    let history: Vec<Value> =
        serde_json::from_slice(&fs::read(out.join("validation.json")).unwrap()).unwrap();
    let selected = &history[choose(&history, plan)];
    let model = DenseModel::from_json(&fs::read(out.join("selected.json")).unwrap()).unwrap();
    assert_eq!(model.id(), selected["genome"].as_str().unwrap());
    assert_eq!(
        json!(task.evaluate(&model).unwrap()),
        selected["training_fitness"]
    );
    let fixed = DenseModel::new(run.history().last().unwrap().best_genome.clone()).unwrap();
    assert_eq!(
        task.evaluate(&fixed).unwrap(),
        run.history().last().unwrap().best_fitness
    );
    vec![
        (format!("{}-selected", task.name), model),
        (format!("{}-240", task.name), fixed),
    ]
}

pub(super) fn evaluate(
    standard: &MutationTask<'_, '_, '_>,
    small: &MutationTask<'_, '_, '_>,
    seeds: &[DenseModel],
    plan: &Value,
    validation: &[Source],
    diagnostic: &[Source],
    root: &Path,
) {
    let out = root.join("evaluate");
    fs::create_dir(&out).unwrap();
    let a = EvolutionRun::load(&root.join("standard/checkpoint.json"), standard).unwrap();
    let b = EvolutionRun::load(&root.join("small/checkpoint.json"), small).unwrap();
    assert_eq!(a.history()[0].population_ids, b.history()[0].population_ids);
    assert_eq!(json!(a.history()[0].fitness), json!(b.history()[0].fitness));
    let mut models = vec![
        ("seed-24".into(), seeds[0].clone()),
        ("baseline-135".into(), seeds[1].clone()),
    ];
    models.extend(finalists(standard, plan, root));
    models.extend(finalists(small, plan, root));
    save(
        out.join("finalists-freeze.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(MUTATION_PLAN),"initial_population_identical":true,
        "models":models.iter().map(|(label,model)|json!({"label":label,"genome":model.id()})).collect::<Vec<_>>(),
        "checkpoints":[hash(&fs::read(root.join("standard/checkpoint.json")).unwrap()),hash(&fs::read(root.join("small/checkpoint.json")).unwrap())]}),
    );
    // No qualification world is discovered until all candidate identities are fixed.
    let (qualification, audit) = cohort(plan, "qualification");
    save(
        out.join("qualification-sources.json"),
        &json!({"sources":records(&qualification),"discovery":audit}),
    );
    let search = standard.guided.search;
    let mut reports = Vec::new();
    for (label, driver, model) in models
        .iter()
        .map(|(l, m)| (l.as_str(), "model", m))
        .chain(std::iter::once(("native", "mission", &seeds[0])))
    {
        let mut report = focused::model_report(search, plan, None, label, driver, model, &out);
        if driver == "model" {
            report["settling"] = json!(full_retention::settling(
                &ship_curriculum::evaluate(Policy::Model(model.clone())).unwrap(),
                plan,
                model,
                driver,
                label,
                &out
            ));
            let fitness = standard.evaluate(model).unwrap().0;
            report["fitness"] = json!(fitness);
            report["retention_eligible"] = json!(eligible(fitness, plan));
        }
        let mut groups = Vec::new();
        for (group, cases) in [
            ("validation", validation),
            ("qualification", qualification.as_slice()),
            ("diagnostic", diagnostic),
        ] {
            let traces = out.join(format!("{label}-{group}"));
            fs::create_dir(&traces).unwrap();
            let episodes = search.task.pool.map(cases, |(case, source)| {
                episode(&source.lock().unwrap(), case, driver, model, Some(&traces))
            });
            let arrived = episodes
                .iter()
                .filter(|e| e.outcome == Outcome::Arrived)
                .count();
            eprintln!("{label} {group}: {arrived}/{}", cases.len());
            groups.push(json!({"partition":group,"arrived":arrived,"episodes":episodes}));
        }
        report["evaluation"] = json!(groups);
        reports.push(report);
        atomic(
            &out,
            "results.json",
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(MUTATION_PLAN),"controllers":reports,"production_controls_changed":false,"live_integration_qualified":false}),
        );
    }
}
