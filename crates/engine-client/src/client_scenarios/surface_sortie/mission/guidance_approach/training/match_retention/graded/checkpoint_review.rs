//! Retrospective transfer checks without changing the completed experiment.
use super::*;
use std::collections::BTreeMap;

const REVIEW_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-checkpoint-review-v1-plan.json"
));

fn frozen_sources(receipt: &Value, field: &str) -> Vec<Source> {
    let result = sources(
        receipt[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| serde_json::from_value(r["case"].clone()).unwrap())
            .collect(),
    );
    let actual: Value =
        serde_json::from_slice(&serde_json::to_vec(&records(&result)).unwrap()).unwrap();
    assert_eq!(actual, receipt[field], "physical sources changed: {field}");
    result
}

#[test]
#[ignore = "retrospective checkpoint diagnostics; explicit fresh output required"]
fn learned_guidance_checkpoint_review() {
    let plan: Value = serde_json::from_slice(REVIEW_PLAN).unwrap();
    let prior = Path::new(plan["prior_directory"].as_str().unwrap());
    for (name, digest) in plan["prior_files"].as_object().unwrap() {
        assert_eq!(hash(&fs::read(prior.join(name)).unwrap()), *digest);
    }
    let root = std::env::var_os("SPACEWARS_REVIEW_ROOT").unwrap();
    let out = Path::new(&root).join("evaluate");
    fs::create_dir(&out).expect("retain prior evidence");
    let recipe: Value =
        serde_json::from_slice(&fs::read(prior.join("plan.json")).unwrap()).unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(prior.join("preflight/preflight.json")).unwrap()).unwrap();
    let checkpoint: Value =
        serde_json::from_slice(&fs::read(prior.join("run/checkpoint.json")).unwrap()).unwrap();
    let history = checkpoint["run"]["history"].as_array().unwrap();
    assert_eq!(history.len(), 240);
    let training = frozen_sources(&receipt, "training");
    let inspected = frozen_sources(&receipt, "inspected");
    let monitoring = frozen_sources(&receipt, "validation");
    let protected = serde_json::from_value(recipe["protected_cases"].clone()).unwrap();
    let probes = brake_probes(&training, &protected);
    let task = TrainingTask {
        sources: &training,
        retained: retention_cases(&recipe),
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: TaskManifest {
            id: plan["experiment"].as_str().unwrap().into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"plan_utf8":std::str::from_utf8(REVIEW_PLAN).unwrap()}),
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
    let mut reports = Vec::new();
    for generation in plan["full_generations"].as_array().unwrap() {
        let index = generation.as_u64().unwrap() as usize - 1;
        let row = &history[index];
        let model =
            DenseModel::new(serde_json::from_value(row["best_genome"].clone()).unwrap()).unwrap();
        assert_eq!(model.id(), row["best_id"].as_str().unwrap());
        let fitness: GradedFitness = serde_json::from_value(row["best_fitness"].clone()).unwrap();
        assert_eq!(graded.evaluate(&model).unwrap(), fitness);
        let label = format!("generation-{}", index + 1);
        save(out.join(format!("{label}.json")), model.genome());
        let mut report = focused::model_report(
            &search,
            &recipe,
            Some(&monitoring),
            &label,
            "model",
            &model,
            &out,
        );
        let traces = out.join(format!("{label}-inspected"));
        fs::create_dir(&traces).unwrap();
        let episodes = task.pool.map(&inspected, |(case, source)| {
            episode(
                &source.lock().unwrap(),
                case,
                "model",
                &model,
                Some(&traces),
            )
        });
        report["inspected"] = json!({
            "arrived":episodes.iter().filter(|e| e.outcome == Outcome::Arrived).count(),
            "episodes":episodes
        });
        report["fitness"] = json!(fitness);
        report["generation"] = json!(index + 1);
        report["qualification_evidence"] = json!(false);
        reports.push(report);
        save(out.join("full-results.json"), &reports);
    }
    let mut cached = BTreeMap::<String, Value>::new();
    let mut monitoring_history = Vec::new();
    for generation in plan["monitor_generations"].as_array().unwrap() {
        let index = generation.as_u64().unwrap() as usize - 1;
        let row = &history[index];
        let model =
            DenseModel::new(serde_json::from_value(row["best_genome"].clone()).unwrap()).unwrap();
        assert_eq!(model.id(), row["best_id"].as_str().unwrap());
        let evaluated = cached.entry(model.id().into()).or_insert_with(|| {
            let episodes = task.pool.map(&monitoring, |(case, source)| {
                episode(&source.lock().unwrap(), case, "model", &model, None)
            });
            json!({
                "arrived":episodes.iter().filter(|e| e.outcome == Outcome::Arrived).count(),
                "episodes":episodes
            })
        });
        monitoring_history.push(json!({
            "generation":index + 1,"genome":model.id(),"training_fitness":row["best_fitness"],
            "monitoring":evaluated
        }));
        save(out.join("monitoring-history.json"), &monitoring_history);
    }
    let model =
        DenseModel::new(serde_json::from_value(history[239]["best_genome"].clone()).unwrap())
            .unwrap();
    let mut controls = Vec::new();
    for driver in plan["controls"].as_array().unwrap() {
        let driver = driver.as_str().unwrap();
        let traces = out.join(format!("{driver}-monitoring"));
        fs::create_dir(&traces).unwrap();
        let episodes = task.pool.map(&monitoring, |(case, source)| {
            episode(&source.lock().unwrap(), case, driver, &model, Some(&traces))
        });
        controls.push(json!({"controller":driver,
            "arrived":episodes.iter().filter(|e| e.outcome == Outcome::Arrived).count(),
            "episodes":episodes}));
    }
    save(out.join("controls.json"), &controls);
    save(
        out.join("receipt.json"),
        &json!({
            "runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(REVIEW_PLAN),"physical_sources_identical":true,
            "full_generations":plan["full_generations"],"monitor_generations":monitoring_history.len(),
            "unique_monitor_genomes":cached.len(),"qualification_evidence":false,
            "production_controls_changed":false
        }),
    );
}
