//! Restore full retention while preserving the newly acquired approach pair.
use super::*;

mod run;

const JOINT_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-joint-retention-v1-plan.json"
));

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JointFitness {
    focal_completed: u32,
    retained_completed: u32,
    precision_completed: u32,
    retained_match_completed: u32,
    precision_position_points: u32,
    precision_velocity_points: u32,
    approach_speed_points: u32,
    approach_position_points: u32,
    spare_ticks: u32,
}

impl JointFitness {
    fn precision(&mut self, score: ship_arrival::ArrivalFitness) {
        self.precision_completed += score.completed;
        self.retained_completed += score.completed;
        self.precision_position_points += score.position_points;
        self.precision_velocity_points += score.velocity_points;
        self.spare_ticks += score.spare_ticks;
    }

    fn approach(&mut self, result: &Episode, focal: bool) {
        let mut quality = Fitness::default();
        quality.record_approach(result);
        if focal {
            self.focal_completed += quality.approach_completed;
        } else {
            self.retained_match_completed += quality.approach_completed;
            self.retained_completed += quality.approach_completed;
        }
        self.approach_speed_points += quality.approach_speed_points;
        self.approach_position_points += quality.approach_position_points;
        self.spare_ticks += quality.spare_ticks;
    }
}

struct JointTask<'a> {
    base: TrainingTask<'a>,
    focal: BTreeSet<String>,
}

impl EvaluationTask for JointTask<'_> {
    type Fitness = JointFitness;

    fn manifest(&self) -> TaskManifest {
        self.base.manifest()
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError> {
        let mut score = JointFitness::default();
        for result in self.base.pool.map(&self.base.retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false)
        }) {
            score.precision(result?.fitness);
        }
        for result in self.base.pool.map(self.base.sources, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, None)
        }) {
            score.approach(&result, self.focal.contains(&result.case));
        }
        Ok(score)
    }
}

fn parsed(value: Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn task<'a>(sources: &'a [Source], plan: &Value, arm: &str) -> JointTask<'a> {
    JointTask {
        base: TrainingTask {
            sources,
            retained: retention_cases(plan),
            pool: EpisodePool::new(worker_config()).unwrap(),
            manifest: TaskManifest {
                id: format!("recovery-guidance-joint-retention-v1-{arm}"),
                contract: spacewars_ai::guidance::contract(),
                configuration: json!({"arm":arm,"plan_utf8":std::str::from_utf8(JOINT_PLAN).unwrap()}),
            },
        },
        focal: serde_json::from_value(plan["focal_cases"].clone()).unwrap(),
    }
}

fn eligible(f: JointFitness) -> bool {
    f.focal_completed == 2
        && f.retained_completed == 619
        && f.precision_completed == 157
        && f.retained_match_completed == 462
}

fn model(root: &Path, plan: &Value, name: &str) -> DenseModel {
    let specification = &plan["models"][name];
    let bytes = fs::read(root.join(specification["file"].as_str().unwrap())).unwrap();
    assert_eq!(hash(&bytes), specification["sha256"]);
    let model = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(model.id(), specification["genome"].as_str().unwrap());
    model
}

#[test]
fn joint_retention_protects_focal_success_then_total_success_before_precision_ties() {
    let plan: Value = serde_json::from_slice(JOINT_PLAN).unwrap();
    let order = plan["fitness_order"].as_array().unwrap();
    for (index, field) in order.iter().enumerate() {
        let mut better = json!(JointFitness::default());
        better[field.as_str().unwrap()] = json!(1);
        let mut worse = json!(JointFitness::default());
        for lower in &order[index + 1..] {
            worse[lower.as_str().unwrap()] = json!(100_000);
        }
        assert!(
            serde_json::from_value::<JointFitness>(better).unwrap()
                > serde_json::from_value::<JointFitness>(worse).unwrap()
        );
    }
    let balanced = JointFitness {
        focal_completed: 2,
        retained_completed: 600,
        precision_completed: 149,
        retained_match_completed: 451,
        ..JointFitness::default()
    };
    let precision = JointFitness {
        retained_completed: 595,
        precision_completed: 156,
        retained_match_completed: 439,
        ..balanced
    };
    assert!(balanced > precision);
    assert!(!eligible(balanced));
    let full = JointFitness {
        retained_completed: 619,
        precision_completed: 157,
        retained_match_completed: 462,
        ..balanced
    };
    assert!(eligible(full));
    assert!(!eligible(JointFitness {
        focal_completed: 1,
        ..full
    }));
    let retained: BTreeSet<String> =
        serde_json::from_value(plan["retained_match_cases"].clone()).unwrap();
    let focal: BTreeSet<String> = serde_json::from_value(plan["focal_cases"].clone()).unwrap();
    assert_eq!(
        (retained.len(), focal.len(), retention_cases(&plan).len()),
        (462, 2, 157)
    );
    assert!(retained.is_disjoint(&focal));
    let config = configuration(&plan);
    assert_eq!(
        (config.population, config.generations, config.seed),
        (16, 48, 1)
    );
}

#[test]
#[ignore = "full-retention continuation; frozen plan and explicit mode required"]
fn learned_guidance_joint_retention() {
    let mode = std::env::var("SPACEWARS_JOINT_MODE").unwrap();
    assert!(["preflight", "train", "evaluate"].contains(&mode.as_str()));
    let root = std::env::var_os("SPACEWARS_JOINT_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), JOINT_PLAN);
    let plan: Value = serde_json::from_slice(JOINT_PLAN).unwrap();
    let bytes = fs::read(root.join("inputs.json")).unwrap();
    assert_eq!(hash(&bytes), plan["inputs_sha256"]);
    let inputs: Value = serde_json::from_slice(&bytes).unwrap();
    let all = restore(&inputs, "sources");
    let initial = records(&all);
    let included: BTreeSet<String> = plan["retained_match_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(plan["focal_cases"].as_array().unwrap())
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    let (training, excluded): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|(case, _)| included.contains(&case.name));
    assert_eq!((training.len(), excluded.len()), (464, 18));
    let receipt_path = root.join("preflight/preflight.json");
    if mode == "preflight" {
        run::preflight(&training, &excluded, &plan, &inputs, &initial, root);
    } else {
        let receipt: Value = serde_json::from_slice(&fs::read(receipt_path).unwrap()).unwrap();
        assert_eq!(
            receipt["runtime"],
            engine_agent::evolution::runtime_id().unwrap()
        );
        assert_eq!(receipt["plan_sha256"], hash(JOINT_PLAN));
        assert_eq!(receipt["sources"], parsed(json!(initial)));
        if mode == "train" {
            for arm in plan["arms"].as_array().unwrap() {
                let arm = arm.as_str().unwrap();
                let task = task(&training, &plan, arm);
                run::train(&task, &model(root, &plan, arm), &plan, arm, root);
            }
        } else {
            run::evaluate(&training, &excluded, &plan, root);
        }
    }
}
