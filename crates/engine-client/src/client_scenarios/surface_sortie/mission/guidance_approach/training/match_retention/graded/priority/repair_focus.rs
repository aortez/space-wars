//! Compare broad progress with progress on the remaining failed approaches.
use super::*;

mod run;

const FOCUS_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-repair-focus-v1-plan.json"
));

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusFitness<const FOCUSED: bool> {
    protected_completed: u32,
    remaining_completed: u32,
    remaining_speed_points: u32,
    remaining_position_points: u32,
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

impl<const FOCUSED: bool> FocusFitness<FOCUSED> {
    fn precision(&mut self, score: ship_arrival::ArrivalFitness) {
        self.protected_completed += score.completed;
        self.precision_completed += score.completed;
        self.retained_completed += score.completed;
        self.precision_position_points += score.position_points;
        self.precision_velocity_points += score.velocity_points;
        self.spare_ticks += score.spare_ticks;
    }

    fn approach(&mut self, result: &Episode, focal: bool, remaining: bool) {
        let mut quality = Fitness::default();
        quality.record_approach(result);
        if remaining {
            self.remaining_completed += quality.approach_completed;
            self.remaining_speed_points += quality.approach_speed_points;
            self.remaining_position_points += quality.approach_position_points;
        } else {
            self.protected_completed += quality.approach_completed;
        }
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

impl<const FOCUSED: bool> Ord for FocusFitness<FOCUSED> {
    fn cmp(&self, other: &Self) -> Ordering {
        let key = |f: &Self| {
            let broad = [
                f.precision_position_points,
                f.precision_velocity_points,
                f.approach_speed_points,
                f.approach_position_points,
                f.spare_ticks,
            ];
            let remaining = [f.remaining_speed_points, f.remaining_position_points];
            let mut values = vec![
                f.protected_completed,
                f.remaining_completed,
                f.focal_completed,
                f.retained_completed,
                f.precision_completed,
                f.retained_match_completed,
            ];
            if FOCUSED {
                values.extend(remaining);
                values.extend(broad);
            } else {
                values.extend(broad);
                values.extend(remaining);
            }
            values
        };
        key(self).cmp(&key(other))
    }
}

impl<const FOCUSED: bool> PartialOrd for FocusFitness<FOCUSED> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct FocusTask<'a, const FOCUSED: bool> {
    base: TrainingTask<'a>,
    focal: BTreeSet<String>,
    remaining: BTreeSet<String>,
}

impl<const FOCUSED: bool> EvaluationTask for FocusTask<'_, FOCUSED> {
    type Fitness = FocusFitness<FOCUSED>;

    fn manifest(&self) -> TaskManifest {
        self.base.manifest()
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError> {
        let mut score = FocusFitness::default();
        for result in self.base.pool.map(&self.base.retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false)
        }) {
            score.precision(result?.fitness);
        }
        for result in self.base.pool.map(self.base.sources, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, None)
        }) {
            score.approach(
                &result,
                self.focal.contains(&result.case),
                self.remaining.contains(&result.case),
            );
        }
        Ok(score)
    }
}

fn parsed(value: Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn task<'a, const FOCUSED: bool>(
    sources: &'a [Source],
    plan: &Value,
    arm: &str,
) -> FocusTask<'a, FOCUSED> {
    FocusTask {
        base: TrainingTask {
            sources,
            retained: retention_cases(plan),
            pool: EpisodePool::new(worker_config()).unwrap(),
            manifest: TaskManifest {
                id: format!("recovery-guidance-repair-focus-v1-{arm}"),
                contract: spacewars_ai::guidance::contract(),
                configuration: json!({"arm":arm,"fitness_order":plan["fitness_orders"][if FOCUSED {"focused"} else {"broad"}],"plan_utf8":std::str::from_utf8(FOCUS_PLAN).unwrap()}),
            },
        },
        focal: serde_json::from_value(plan["focal_cases"].clone()).unwrap(),
        remaining: serde_json::from_value(plan["remaining_cases"].clone()).unwrap(),
    }
}

fn eligible<const FOCUSED: bool>(f: FocusFitness<FOCUSED>) -> bool {
    f.protected_completed == 616
        && f.remaining_completed == 5
        && f.focal_completed == 2
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
fn repair_focus_changes_progress_priority_under_the_same_completion_guards() {
    fn check<const FOCUSED: bool>(arm: &str) {
        let plan: Value = serde_json::from_slice(FOCUS_PLAN).unwrap();
        let order = plan["fitness_orders"][arm].as_array().unwrap();
        for (index, field) in order.iter().enumerate() {
            let mut better = json!(FocusFitness::<FOCUSED>::default());
            better[field.as_str().unwrap()] = json!(1);
            let mut worse = json!(FocusFitness::<FOCUSED>::default());
            for lower in &order[index + 1..] {
                worse[lower.as_str().unwrap()] = json!(100_000);
            }
            assert!(
                serde_json::from_value::<FocusFitness<FOCUSED>>(better).unwrap()
                    > serde_json::from_value::<FocusFitness<FOCUSED>>(worse).unwrap()
            );
        }
        let seed = FocusFitness::<FOCUSED> {
            protected_completed: 616,
            focal_completed: 2,
            retained_completed: 614,
            precision_completed: 157,
            retained_match_completed: 457,
            ..FocusFitness::default()
        };
        assert!(!eligible(seed));
        assert!(
            seed > FocusFitness {
                protected_completed: 615,
                remaining_completed: 5,
                ..seed
            }
        );
        assert!(eligible(FocusFitness {
            remaining_completed: 5,
            retained_completed: 619,
            retained_match_completed: 462,
            ..seed
        }));
    }
    check::<false>("broad");
    check::<true>("focused");
    let a = json!({"protected_completed":616,"remaining_completed":0,"focal_completed":2,
        "retained_completed":614,"precision_completed":157,"retained_match_completed":457,
        "precision_position_points":100,"precision_velocity_points":0,"approach_speed_points":0,
        "approach_position_points":0,"spare_ticks":0,"remaining_speed_points":10,"remaining_position_points":0});
    let mut b = a.clone();
    b["precision_position_points"] = json!(99);
    b["remaining_speed_points"] = json!(11);
    assert!(
        serde_json::from_value::<FocusFitness<false>>(a.clone()).unwrap()
            > serde_json::from_value::<FocusFitness<false>>(b.clone()).unwrap()
    );
    assert!(
        serde_json::from_value::<FocusFitness<true>>(a).unwrap()
            < serde_json::from_value::<FocusFitness<true>>(b).unwrap()
    );
    let plan: Value = serde_json::from_slice(FOCUS_PLAN).unwrap();
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["preservation_cases"].clone()).unwrap();
    let remaining: BTreeSet<String> =
        serde_json::from_value(plan["remaining_cases"].clone()).unwrap();
    let included: BTreeSet<String> = plan["retained_match_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(plan["focal_cases"].as_array().unwrap())
        .map(|n| n.as_str().unwrap().into())
        .collect();
    assert_eq!(
        (
            protected.len(),
            remaining.len(),
            retention_cases(&plan).len()
        ),
        (459, 5, 157)
    );
    assert!(protected.is_disjoint(&remaining));
    assert_eq!(
        protected
            .union(&remaining)
            .cloned()
            .collect::<BTreeSet<_>>(),
        included
    );
    assert_eq!(
        (
            configuration(&plan).population,
            configuration(&plan).generations
        ),
        (16, 48)
    );
}

#[test]
#[ignore = "full-retention continuation; frozen plan and explicit mode required"]
fn learned_guidance_repair_focus() {
    let mode = std::env::var("SPACEWARS_FOCUS_MODE").unwrap();
    assert!(["preflight", "train", "evaluate"].contains(&mode.as_str()));
    let root = std::env::var_os("SPACEWARS_FOCUS_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), FOCUS_PLAN);
    let plan: Value = serde_json::from_slice(FOCUS_PLAN).unwrap();
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
        assert_eq!(receipt["plan_sha256"], hash(FOCUS_PLAN));
        assert_eq!(receipt["sources"], parsed(json!(initial)));
        if mode == "train" {
            run::train(
                &task::<false>(&training, &plan, "broad"),
                &model(root, &plan, "broad"),
                &plan,
                "broad",
                root,
            );
            run::train(
                &task::<true>(&training, &plan, "focused"),
                &model(root, &plan, "focused"),
                &plan,
                "focused",
                root,
            );
        } else {
            run::evaluate(&training, &excluded, &plan, root);
        }
    }
}
