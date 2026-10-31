//! Matched reward ordering with independent validation checkpoint selection.
use super::*;
use std::cmp::Ordering;

mod isolation;
mod joint;
mod mutation;
mod repair;
mod repair_focus;
mod run;

const PRIORITY_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-priority-v1-plan.json"
));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct OrderedFitness<const PROGRESS: bool>(GradedFitness);

impl<const PROGRESS: bool> Ord for OrderedFitness<PROGRESS> {
    fn cmp(&self, other: &Self) -> Ordering {
        let key = |f: GradedFitness| {
            let middle = if PROGRESS {
                [
                    f.approach_speed_points,
                    f.approach_position_points,
                    f.brake_progress_points,
                ]
            } else {
                [
                    f.brake_progress_points,
                    f.approach_speed_points,
                    f.approach_position_points,
                ]
            };
            [
                f.precision_completed,
                f.protected_completed,
                f.additional_completed,
                middle[0],
                middle[1],
                middle[2],
                f.precision_position_points,
                f.precision_velocity_points,
                f.spare_ticks,
            ]
        };
        key(self.0).cmp(&key(other.0))
    }
}

impl<const PROGRESS: bool> PartialOrd for OrderedFitness<PROGRESS> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct OrderedTask<'a, 'b, 'c, const PROGRESS: bool>(Guided<'a, 'b, 'c>);

fn arm_name<const PROGRESS: bool>() -> &'static str {
    if PROGRESS {
        "progress-first"
    } else {
        "brake-first"
    }
}

impl<const PROGRESS: bool> EvaluationTask for OrderedTask<'_, '_, '_, PROGRESS> {
    type Fitness = OrderedFitness<PROGRESS>;

    fn manifest(&self) -> TaskManifest {
        let mut manifest = self.0.search.manifest();
        let plan: Value = serde_json::from_slice(PRIORITY_PLAN).unwrap();
        manifest.id.push_str(arm_name::<PROGRESS>());
        manifest.configuration["arm"] = json!(arm_name::<PROGRESS>());
        manifest.configuration["fitness_order"] = plan["arms"][arm_name::<PROGRESS>()].clone();
        manifest
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError> {
        self.0.evaluate(model).map(OrderedFitness)
    }
}

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

fn discover(
    plan: &Value,
    cohort: &str,
    parent: &DenseModel,
    pool: &EpisodePool,
) -> (Vec<Source>, Value) {
    let config = &plan["cohorts"][cohort];
    let count = config["validation_worlds"].as_u64().unwrap() as usize * 4;
    let mut selected = Vec::new();
    let mut audit = Vec::new();
    for seed in config["validation_seeds"].as_array().unwrap() {
        let mut one = plan.clone();
        one["validation_seeds"] = json!([seed]);
        let (cases, missing) = validation(&one);
        let accepted = missing.is_empty()
            && focused::complete_world(
                &cases
                    .iter()
                    .map(|(case, _)| case.clone())
                    .collect::<Vec<_>>(),
                seed.as_u64().unwrap(),
            );
        let native = pool.map(&cases, |(case, source)| {
            episode(&source.lock().unwrap(), case, "mission", parent, None)
        });
        audit.push(json!({"seed":seed,"accepted":accepted,"available":records(&cases),"missing":missing,"native":native}));
        if accepted {
            selected.extend(cases);
        }
        if selected.len() == count {
            break;
        }
    }
    assert_eq!(selected.len(), count, "insufficient reserve: {cohort}");
    (selected, json!(audit))
}

fn eligible(fitness: GradedFitness, plan: &Value) -> bool {
    let rule = &plan["selection"];
    u64::from(fitness.precision_completed) == rule["precision_completed"].as_u64().unwrap()
        && u64::from(fitness.protected_completed) == rule["protected_completed"].as_u64().unwrap()
        && u64::from(fitness.additional_completed)
            >= rule["minimum_additional_completed"].as_u64().unwrap()
}

fn choose(history: &[Value], plan: &Value) -> usize {
    history
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            eligible(
                serde_json::from_value(row["training_fitness"].clone()).unwrap(),
                plan,
            )
        })
        .min_by_key(|(_, row)| {
            (
                std::cmp::Reverse(row["arrived"].as_u64().unwrap()),
                std::cmp::Reverse(
                    row["training_fitness"]["additional_completed"]
                        .as_u64()
                        .unwrap(),
                ),
                row["generation"].as_u64().unwrap(),
                row["genome"].as_str().unwrap(),
            )
        })
        .expect("parent must remain eligible")
        .0
}

#[test]
fn priority_changes_only_physical_progress_and_brake_precedence() {
    let a = GradedFitness {
        precision_completed: 157,
        protected_completed: 128,
        additional_completed: 30,
        brake_progress_points: 7000,
        approach_speed_points: 20000,
        approach_position_points: 25000,
        ..GradedFitness::default()
    };
    let b = GradedFitness {
        brake_progress_points: 6000,
        approach_speed_points: 20001,
        ..a
    };
    assert!(OrderedFitness::<false>(a) > OrderedFitness::<false>(b));
    assert!(OrderedFitness::<true>(a) < OrderedFitness::<true>(b));
    for field in 0..9 {
        let mut v = serde_json::to_value(a).unwrap();
        let plan: Value = serde_json::from_slice(PRIORITY_PLAN).unwrap();
        let name = plan["arms"]["brake-first"][field].as_str().unwrap();
        v[name] = json!(v[name].as_u64().unwrap() + 1);
        let better: GradedFitness = serde_json::from_value(v).unwrap();
        assert!(OrderedFitness::<false>(better) > OrderedFitness::<false>(a));
        assert!(OrderedFitness::<true>(better) > OrderedFitness::<true>(a));
        if field < 3 {
            let worse = GradedFitness {
                brake_progress_points: 0,
                approach_speed_points: 0,
                approach_position_points: 0,
                ..better
            };
            assert!(OrderedFitness::<true>(worse) > OrderedFitness::<true>(a));
            assert!(OrderedFitness::<false>(worse) > OrderedFitness::<false>(a));
        }
    }
    assert_eq!(
        serde_json::to_value(OrderedFitness::<true>(a)).unwrap(),
        json!(a)
    );
}

#[test]
fn priority_validation_selection_protects_retention_and_uses_declared_ties() {
    let plan: Value = serde_json::from_slice(PRIORITY_PLAN).unwrap();
    let make = |generation, arrived, known, additional, genome| {
        json!({"generation":generation,"arrived":arrived,
        "genome":genome,"training_fitness":GradedFitness { precision_completed:157, protected_completed:known,
        additional_completed:additional, ..GradedFitness::default() }})
    };
    let parent = make(0, 24, 128, 30, "z");
    let mut rows = vec![
        parent.clone(),
        make(12, 32, 127, 32, "a"),
        make(24, 24, 128, 30, "a"),
    ];
    assert_eq!(choose(&rows, &plan), 0);
    rows.push(make(36, 25, 128, 30, "b"));
    assert_eq!(choose(&rows, &plan), 3);
    rows.push(make(48, 25, 128, 32, "c"));
    assert_eq!(choose(&rows, &plan), 4);
    rows.push(make(48, 25, 128, 32, "a"));
    assert_eq!(choose(&rows, &plan), 5);
    rows.push(make(60, 32, 128, 29, "a"));
    assert_eq!(choose(&rows, &plan), 5);
    let mut lost_precision = parent;
    lost_precision["training_fitness"]["precision_completed"] = json!(156);
    assert!(!eligible(
        serde_json::from_value(lost_precision["training_fitness"].clone()).unwrap(),
        &plan
    ));
}

#[test]
#[ignore = "matched 240-generation arms; explicit mode and fresh output required"]
fn learned_guidance_priority() {
    let plan: Value = serde_json::from_slice(PRIORITY_PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_PRIORITY_MODE").unwrap();
    assert!(
        [
            "preflight",
            "train-brake-first",
            "resume-brake-first",
            "train-progress-first",
            "resume-progress-first",
            "evaluate"
        ]
        .contains(&mode.as_str())
    );
    let root = std::env::var_os("SPACEWARS_PRIORITY_ROOT").unwrap();
    let root = Path::new(&root);
    let prior = Path::new(plan["prior_directory"].as_str().unwrap());
    for (file, digest) in plan["prior_files"].as_object().unwrap() {
        assert_eq!(hash(&fs::read(prior.join(file)).unwrap()), *digest);
    }
    let parent_bytes = fs::read(plan["parent_file"].as_str().unwrap()).unwrap();
    assert_eq!(hash(&parent_bytes), plan["parent_file_sha256"]);
    let parent = DenseModel::from_json(&parent_bytes).unwrap();
    assert_eq!(parent.id(), plan["parent_genome"].as_str().unwrap());
    let previous: Value =
        serde_json::from_slice(&fs::read(prior.join("preflight/preflight.json")).unwrap()).unwrap();
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["known_protected_cases"].clone()).unwrap();
    let known = restore(&previous, "known");
    let (mut training, host): (Vec<_>, Vec<_>) = known
        .into_iter()
        .partition(|(case, _)| protected.contains(&case.name));
    assert_eq!(training.len(), 128);
    assert_eq!(host.len(), 2);
    training.extend(restore(&previous, "expansion"));
    let probes: Vec<BrakeProbe> = previous["brake_probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| BrakeProbe {
            case: v["case"].as_str().unwrap().into(),
            inputs: serde_json::from_value(v["inputs"].clone()).unwrap(),
        })
        .collect();
    assert_eq!(probes.len(), 8);
    let pool = EpisodePool::new(worker_config()).unwrap();
    let preflight_path = root.join("preflight/preflight.json");
    if mode == "preflight" {
        let out = root.join("preflight");
        fs::create_dir(&out).unwrap();
        let mut receipt = json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PRIORITY_PLAN),
            "training":records(&training),"host":records(&host),"retention_cases":retention_cases(&plan),"brake_probes":probes});
        for cohort in ["validation", "qualification"] {
            let (cases, audit) = discover(&plan, cohort, &parent, &pool);
            receipt[cohort] = json!(records(&cases));
            receipt[format!("{cohort}_discovery")] = audit;
        }
        let cases: Vec<_> = training
            .iter()
            .filter(|(c, _)| c.seed == 720006 && c.seat == 0)
            .collect();
        let mut diagnosis = Vec::new();
        for driver in ["mission", "reference", "model"] {
            let traces = out.join(driver);
            fs::create_dir(&traces).unwrap();
            for (case, source) in &cases {
                diagnosis.push(episode(
                    &source.lock().unwrap(),
                    case,
                    driver,
                    &parent,
                    Some(&traces),
                ));
            }
        }
        receipt["diagnosis"] = json!(diagnosis);
        save(preflight_path, &receipt);
        return;
    }
    let bytes = fs::read(preflight_path).unwrap();
    let receipt: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        receipt["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(receipt["plan_sha256"], hash(PRIORITY_PLAN));
    assert_eq!(
        serde_json::from_slice::<Value>(&serde_json::to_vec(&records(&training)).unwrap()).unwrap(),
        receipt["training"]
    );
    let validation = restore(&receipt, "validation");
    let task = TrainingTask {
        sources: &training,
        retained: retention_cases(&plan),
        pool,
        manifest: TaskManifest {
            id: plan["experiment"].as_str().unwrap().into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"plan_utf8":std::str::from_utf8(PRIORITY_PLAN).unwrap(),"preflight_sha256":hash(&bytes)}),
        },
    };
    let search = Search {
        task: &task,
        protected,
    };
    let make = || Guided {
        search: &search,
        probes: &probes,
        arm: Arm::Graded,
    };
    let control = OrderedTask::<false>(make());
    let progress = OrderedTask::<true>(make());
    if mode == "evaluate" {
        run::evaluate(
            &control,
            &progress,
            &parent,
            &plan,
            &receipt,
            &validation,
            &host,
            root,
        );
    } else if mode.ends_with("brake-first") {
        run::train(
            &control,
            &parent,
            &plan,
            &validation,
            root,
            mode.starts_with("resume"),
        );
    } else {
        run::train(
            &progress,
            &parent,
            &plan,
            &validation,
            root,
            mode.starts_with("resume"),
        );
    }
}
