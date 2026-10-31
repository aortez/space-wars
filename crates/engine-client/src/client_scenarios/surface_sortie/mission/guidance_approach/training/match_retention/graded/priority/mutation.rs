//! Matched mutation scales with expanded retention and independent validation.
use super::*;

mod run;

const MUTATION_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-mutation-v1-plan.json"
));

struct MutationTask<'a, 'b, 'c> {
    guided: Guided<'a, 'b, 'c>,
    name: &'static str,
}

fn arm_configuration(plan: &Value, name: &str) -> EvolutionConfig {
    let mut config = plan["evolution"].clone();
    config["mutation_scale"] = plan["arms"][name]["mutation_scale"].clone();
    serde_json::from_value(config).unwrap()
}

impl EvaluationTask for MutationTask<'_, '_, '_> {
    type Fitness = OrderedFitness<true>;

    fn manifest(&self) -> TaskManifest {
        let mut manifest = self.guided.search.manifest();
        let plan: Value = serde_json::from_slice(MUTATION_PLAN).unwrap();
        manifest.id.push_str(self.name);
        manifest.configuration["arm"] = json!(self.name);
        manifest.configuration["fitness_order"] = plan["fitness_order"].clone();
        manifest
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError> {
        self.guided.evaluate(model).map(OrderedFitness)
    }
}

fn parsed(value: Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn cohort(plan: &Value, name: &str) -> (Vec<Source>, Value) {
    let mut config = plan.clone();
    config["validation_seeds"] = plan["cohorts"][name]["validation_seeds"].clone();
    config["validation_worlds"] = plan["cohorts"][name]["validation_worlds"].clone();
    let (cases, audit) = focused::discover(&config);
    assert_eq!(
        cases.len() as u64,
        config["validation_worlds"].as_u64().unwrap() * 4
    );
    (cases, audit)
}

#[test]
fn mutation_comparison_changes_only_scale_and_requires_all_known_successes() {
    let plan: Value = serde_json::from_slice(MUTATION_PLAN).unwrap();
    let mut standard = json!(arm_configuration(&plan, "standard"));
    let mut small = json!(arm_configuration(&plan, "small"));
    assert_eq!(standard["mutation_scale"], json!(0.15_f32));
    assert_eq!(small["mutation_scale"], json!(0.05_f32));
    standard.as_object_mut().unwrap().remove("mutation_scale");
    small.as_object_mut().unwrap().remove("mutation_scale");
    assert_eq!(standard, small);
    let f = GradedFitness {
        precision_completed: 157,
        protected_completed: 460,
        ..GradedFitness::default()
    };
    assert!(eligible(f, &plan));
    assert!(!eligible(
        GradedFitness {
            protected_completed: 459,
            additional_completed: 14,
            ..f
        },
        &plan
    ));
    assert!(!eligible(
        GradedFitness {
            precision_completed: 156,
            additional_completed: 14,
            ..f
        },
        &plan
    ));
    let rows = vec![
        json!({"generation":0,"genome":"seed","training_fitness":f,"arrived":100}),
        json!({"generation":12,"genome":"lost","training_fitness":GradedFitness{protected_completed:459,..f},"arrived":128}),
    ];
    assert_eq!(choose(&rows, &plan), 0);
}

#[test]
#[ignore = "matched mutation experiment; explicit mode and frozen inputs required"]
fn learned_guidance_mutation() {
    let plan: Value = serde_json::from_slice(MUTATION_PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_MUTATION_MODE").unwrap();
    assert!(
        [
            "preflight",
            "train-standard",
            "resume-standard",
            "train-small",
            "resume-small",
            "evaluate"
        ]
        .contains(&mode.as_str())
    );
    let root = std::env::var_os("SPACEWARS_MUTATION_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), MUTATION_PLAN);
    let bytes = fs::read(root.join("inputs.json")).unwrap();
    assert_eq!(hash(&bytes), plan["inputs_sha256"]);
    let inputs: Value = serde_json::from_slice(&bytes).unwrap();
    let models: Vec<_> = plan["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let bytes = fs::read(root.join(row["file"].as_str().unwrap())).unwrap();
            assert_eq!(hash(&bytes), row["sha256"]);
            let model = DenseModel::from_json(&bytes).unwrap();
            assert_eq!(model.id(), row["genome"].as_str().unwrap());
            model
        })
        .collect();
    let seed = &models[0];
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let additional: BTreeSet<String> =
        serde_json::from_value(plan["additional_cases"].clone()).unwrap();
    assert!(protected.is_disjoint(&additional));
    let all = restore(&inputs, "sources");
    let (training, diagnostic): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|(c, _)| protected.contains(&c.name) || additional.contains(&c.name));
    assert_eq!((training.len(), diagnostic.len()), (474, 8));
    let probes: Vec<BrakeProbe> = inputs["brake_probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| BrakeProbe {
            case: v["case"].as_str().unwrap().into(),
            inputs: serde_json::from_value(v["inputs"].clone()).unwrap(),
        })
        .collect();
    assert_eq!(probes.len(), 8);
    let task = TrainingTask {
        sources: &training,
        retained: retention_cases(&plan),
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: TaskManifest {
            id: plan["experiment"].as_str().unwrap().into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"plan_utf8":std::str::from_utf8(MUTATION_PLAN).unwrap(),"inputs_sha256":hash(&bytes)}),
        },
    };
    let search = Search {
        task: &task,
        protected,
    };
    let make = |name| MutationTask {
        guided: Guided {
            search: &search,
            probes: &probes,
            arm: Arm::Graded,
        },
        name,
    };
    let standard = make("standard");
    let small = make("small");
    let preflight_path = root.join("preflight/preflight.json");
    if mode == "preflight" {
        let out = root.join("preflight");
        fs::create_dir(&out).unwrap();
        let fitness = standard.evaluate(seed).unwrap();
        assert!(eligible(fitness.0, &plan));
        assert_eq!(fitness.0.additional_completed, 0);
        assert_eq!(fitness, small.evaluate(seed).unwrap());
        let mut checks = Vec::new();
        for (group, cases) in [
            ("training", training.as_slice()),
            ("diagnostic", diagnostic.as_slice()),
        ] {
            let traces = out.join(group);
            fs::create_dir(&traces).unwrap();
            let episodes = task.pool.map(cases, |(case, source)| {
                let e = episode(&source.lock().unwrap(), case, "model", seed, Some(&traces));
                let expected = inputs["expected"]["progress-first-selected"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["case"] == case.name)
                    .unwrap();
                assert_eq!(&parsed(json!(e)), expected, "seed changed on known case");
                e
            });
            checks.push(json!({"partition":group,"episodes":episodes}));
        }
        let hard: Vec<_> = training
            .iter()
            .filter(|(c, _)| additional.contains(&c.name))
            .collect();
        let native = task.pool.map(&hard, |(case, source)| {
            episode(&source.lock().unwrap(), case, "mission", seed, None)
        });
        assert!(native.iter().all(|e| e.outcome == Outcome::Arrived));
        let (validation, audit) = cohort(&plan, "validation");
        save(
            preflight_path,
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(MUTATION_PLAN),
            "seed_fitness":fitness,"training":records(&training),"diagnostic":records(&diagnostic),"validation":records(&validation),
            "validation_discovery":audit,"seed_parity":checks,"additional_native":native,"retention_cases":task.retained,"brake_probes":probes}),
        );
        return;
    }
    let receipt: Value = serde_json::from_slice(&fs::read(preflight_path).unwrap()).unwrap();
    assert_eq!(
        receipt["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(receipt["plan_sha256"], hash(MUTATION_PLAN));
    assert_eq!(parsed(json!(records(&training))), receipt["training"]);
    assert_eq!(parsed(json!(records(&diagnostic))), receipt["diagnostic"]);
    let validation = restore(&receipt, "validation");
    if mode == "evaluate" {
        run::evaluate(
            &standard,
            &small,
            &models,
            &plan,
            &validation,
            &diagnostic,
            root,
        );
    } else {
        let arm = if mode.ends_with("standard") {
            &standard
        } else {
            &small
        };
        run::train(
            arm,
            seed,
            &plan,
            &validation,
            root,
            mode.starts_with("resume"),
        );
    }
}
