//! Learn one retained failure without breeding on the retention curriculum.
use super::*;

mod run;

const ISOLATION_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-isolation-v1-plan.json"
));

fn parsed(value: Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn subset(inputs: &Value, names: &[&str]) -> Vec<Source> {
    let selected: Vec<_> = inputs["sources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| names.contains(&r["case"]["name"].as_str().unwrap()))
        .cloned()
        .collect();
    assert_eq!(selected.len(), names.len());
    restore(&json!({"sources":selected}), "sources")
}

fn score(episode: &Episode) -> Fitness {
    let mut fitness = Fitness::default();
    fitness.record_approach(episode);
    fitness
}

fn config(plan: &Value, seed: u64) -> EvolutionConfig {
    let mut config = configuration(plan);
    config.seed = seed;
    config
}

fn task<'a>(cases: &'a [Source]) -> TrainingTask<'a> {
    TrainingTask {
        sources: cases,
        retained: Vec::new(),
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: TaskManifest {
            id: "recovery-guidance-isolation-v1".into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"plan_utf8":std::str::from_utf8(ISOLATION_PLAN).unwrap()}),
        },
    }
}

#[test]
fn isolation_scores_completion_before_progress_and_contains_no_retention() {
    let failure = Fitness {
        approach_speed_points: 1000,
        approach_position_points: 1000,
        spare_ticks: 300,
        ..Fitness::default()
    };
    let arrival = Fitness {
        approach_completed: 1,
        ..Fitness::default()
    };
    assert!(arrival > failure);
    let slow_far = Fitness {
        approach_speed_points: 900,
        approach_position_points: 500,
        ..Fitness::default()
    };
    let fast_close = Fitness {
        approach_speed_points: 899,
        approach_position_points: 999,
        ..Fitness::default()
    };
    assert!(slow_far > fast_close);
    let empty: Vec<Source> = Vec::new();
    let isolated = task(&empty);
    assert!(isolated.retained.is_empty());
    let plan: Value = serde_json::from_slice(ISOLATION_PLAN).unwrap();
    let a = config(&plan, 1);
    let mut b = config(&plan, 2);
    b.seed = 1;
    assert_eq!(a, b);
    assert_eq!(
        (a.population, a.generations, a.mutation_scale),
        (16, 96, 0.05)
    );
}

#[test]
#[ignore = "single-failure diagnostic; explicit mode and frozen inputs required"]
fn learned_guidance_isolation() {
    let mode = std::env::var("SPACEWARS_ISOLATION_MODE").unwrap();
    assert!(["preflight", "train", "evaluate"].contains(&mode.as_str()));
    let root = std::env::var_os("SPACEWARS_ISOLATION_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), ISOLATION_PLAN);
    let plan: Value = serde_json::from_slice(ISOLATION_PLAN).unwrap();
    let bytes = fs::read(root.join("inputs.json")).unwrap();
    assert_eq!(hash(&bytes), plan["inputs_sha256"]);
    let inputs: Value = serde_json::from_slice(&bytes).unwrap();
    let bytes = fs::read(root.join("seed.json")).unwrap();
    assert_eq!(hash(&bytes), plan["seed"]["sha256"]);
    let seed = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(seed.id(), plan["seed"]["genome"].as_str().unwrap());
    let names = [
        plan["training_case"].as_str().unwrap(),
        plan["shape_check"].as_str().unwrap(),
    ];
    let receipt_path = root.join("preflight/preflight.json");
    if mode == "preflight" {
        let out = root.join("preflight");
        fs::create_dir(&out).unwrap();
        let cases = subset(&inputs, &names);
        let initial = records(&cases);
        let prior = Path::new(plan["prior_directory"].as_str().unwrap());
        let mut results = Vec::new();
        for (label, driver, expected) in [
            ("seed", "model", "expected"),
            ("native", "mission", "native"),
        ] {
            let traces = out.join(label);
            fs::create_dir(&traces).unwrap();
            let mut episodes = Vec::new();
            for (case, source) in &cases {
                let e = episode(&source.lock().unwrap(), case, driver, &seed, Some(&traces));
                let old = inputs[expected]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["case"] == case.name)
                    .unwrap();
                assert_eq!(&parsed(json!(e)), old);
                let previous = if label == "seed" {
                    "small-selected"
                } else {
                    "native"
                };
                let file = format!("{}-{driver}.jsonl", case.name);
                assert_eq!(
                    fs::read(traces.join(&file)).unwrap(),
                    fs::read(prior.join(format!("evaluate/{previous}-additional/{file}"))).unwrap()
                );
                assert_eq!(e.outcome == Outcome::Arrived, driver == "mission");
                episodes.push(json!({"fitness":score(&e),"episode":e}));
            }
            results.push(json!({"controller":label,"episodes":episodes}));
        }
        assert_eq!(records(&cases), initial);
        save(
            receipt_path,
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(ISOLATION_PLAN),"sources":initial,"controllers":results,"trace_parity":true}),
        );
        return;
    }
    let receipt: Value = serde_json::from_slice(&fs::read(receipt_path).unwrap()).unwrap();
    assert_eq!(
        receipt["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(receipt["plan_sha256"], hash(ISOLATION_PLAN));
    let training = subset(&inputs, &names[..1]);
    assert_eq!(
        parsed(json!(records(&training))),
        json!([receipt["sources"][0].clone()])
    );
    let task = task(&training);
    if mode == "train" {
        for value in plan["evolution_seeds"].as_array().unwrap() {
            run::train(&task, &seed, &plan, value.as_u64().unwrap(), root);
        }
    } else {
        run::evaluate(&task, &seed, &plan, &inputs, root);
    }
}
