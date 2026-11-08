//! Repair precision while preserving one learned match approach.
use super::*;

mod run;

const REPAIR_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-repair-v1-plan.json"
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

// Serialize the familiar score fields, but make the changed precedence explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct RepairFitness(Fitness);

impl Ord for RepairFitness {
    fn cmp(&self, other: &Self) -> Ordering {
        let key = |f: Fitness| {
            [
                f.approach_completed,
                f.retained_completed,
                f.retained_position_points,
                f.retained_velocity_points,
                f.approach_speed_points,
                f.approach_position_points,
                f.spare_ticks,
            ]
        };
        key(self.0).cmp(&key(other.0))
    }
}

impl PartialOrd for RepairFitness {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct RepairTask<'a>(TrainingTask<'a>);

impl<'a> std::ops::Deref for RepairTask<'a> {
    type Target = TrainingTask<'a>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EvaluationTask for RepairTask<'_> {
    type Fitness = RepairFitness;
    fn manifest(&self) -> TaskManifest {
        self.0.manifest()
    }
    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError> {
        self.0.evaluate(model).map(RepairFitness)
    }
}

fn task<'a>(cases: &'a [Source]) -> RepairTask<'a> {
    let plan: Value = serde_json::from_slice(REPAIR_PLAN).unwrap();
    RepairTask(TrainingTask {
        sources: cases,
        retained: serde_json::from_value(plan["precision_training"].clone()).unwrap(),
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: TaskManifest {
            id: "recovery-guidance-repair-v1".into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"plan_utf8":std::str::from_utf8(REPAIR_PLAN).unwrap()}),
        },
    })
}

fn joint(f: RepairFitness) -> bool {
    f.0.approach_completed == 1 && f.0.retained_completed == 5
}

#[test]
fn repair_protects_focal_arrival_before_precision_and_uses_declared_progress_order() {
    let plan: Value = serde_json::from_slice(REPAIR_PLAN).unwrap();
    let order = plan["fitness_order"].as_array().unwrap();
    for (index, field) in order.iter().enumerate() {
        let mut better = json!(Fitness::default());
        better[field.as_str().unwrap()] = json!(1);
        let mut worse = json!(Fitness::default());
        for lower in &order[index + 1..] {
            worse[lower.as_str().unwrap()] = json!(100_000);
        }
        let better: RepairFitness = serde_json::from_value(better).unwrap();
        let worse: RepairFitness = serde_json::from_value(worse).unwrap();
        assert!(better > worse);
    }
    let empty = Vec::new();
    let repair = task(&empty);
    assert_eq!(repair.retained.len(), 5);
    let all = retention_cases(&plan);
    for case in &repair.retained {
        assert!(all.contains(case));
    }
    let a = config(&plan, 1);
    let mut b = config(&plan, 2);
    b.seed = 1;
    assert_eq!(a, b);
    assert_eq!(
        (a.population, a.generations, a.mutation_scale),
        (16, 96, 0.05)
    );
    assert!(!joint(RepairFitness(Fitness {
        retained_completed: 5,
        ..Fitness::default()
    })));
    assert!(joint(RepairFitness(Fitness {
        approach_completed: 1,
        retained_completed: 5,
        ..Fitness::default()
    })));
}

#[test]
#[ignore = "precision repair diagnostic; explicit mode and frozen inputs required"]
fn learned_guidance_repair() {
    let mode = std::env::var("SPACEWARS_REPAIR_MODE").unwrap();
    assert!(["preflight", "train", "evaluate"].contains(&mode.as_str()));
    let root = std::env::var_os("SPACEWARS_REPAIR_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), REPAIR_PLAN);
    let plan: Value = serde_json::from_slice(REPAIR_PLAN).unwrap();
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
                    "evaluate/seed-3-first-success-additional"
                } else {
                    "preflight/native"
                };
                let file = format!("{}-{driver}.jsonl", case.name);
                assert_eq!(
                    fs::read(traces.join(&file)).unwrap(),
                    fs::read(prior.join(format!("{previous}/{file}"))).unwrap()
                );
                assert_eq!(e.outcome, Outcome::Arrived);
                episodes.push(json!({"fitness":score(&e),"episode":e}));
            }
            results.push(json!({"controller":label,"episodes":episodes}));
        }
        let repair = task(&cases[..1]);
        let mut precision = Vec::new();
        for case in &repair.retained {
            let e =
                ship_arrival::run_episode(case.clone(), Policy::Model(seed.clone()), true).unwrap();
            save(out.join(format!("precision-{}.json", case.name)), &e);
            let mut expected = inputs["precision_expected"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["case"]["name"] == case.name)
                .unwrap()
                .clone();
            expected["runtime"] = json!(engine_agent::evolution::runtime_id().unwrap());
            let mut endpoint = e.clone();
            endpoint.trace.clear();
            assert_eq!(parsed(json!(endpoint)), expected);
            precision.push(endpoint);
        }
        assert_eq!(records(&cases), initial);
        save(
            receipt_path,
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(REPAIR_PLAN),"sources":initial,"controllers":results,"precision":precision,"trace_parity":true}),
        );
        return;
    }
    let receipt: Value = serde_json::from_slice(&fs::read(receipt_path).unwrap()).unwrap();
    assert_eq!(
        receipt["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(receipt["plan_sha256"], hash(REPAIR_PLAN));
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
