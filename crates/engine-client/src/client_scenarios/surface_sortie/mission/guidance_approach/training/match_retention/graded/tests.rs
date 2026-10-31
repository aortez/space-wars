use super::*;

#[test]
fn graded_braking_keeps_every_completion_count_ahead_of_progress() {
    let retained = GradedFitness {
        precision_completed: 157,
        protected_completed: 90,
        additional_completed: 1,
        ..GradedFitness::default()
    };
    let maximum_progress = GradedFitness {
        brake_progress_points: u32::MAX,
        approach_speed_points: u32::MAX,
        approach_position_points: u32::MAX,
        precision_position_points: u32::MAX,
        precision_velocity_points: u32::MAX,
        spare_ticks: u32::MAX,
        ..retained
    };
    assert!(
        retained
            > GradedFitness {
                precision_completed: 156,
                ..maximum_progress
            }
    );
    assert!(
        retained
            > GradedFitness {
                protected_completed: 89,
                ..maximum_progress
            }
    );
    assert!(
        retained
            > GradedFitness {
                additional_completed: 0,
                ..maximum_progress
            }
    );
    assert!(
        GradedFitness {
            brake_progress_points: 1,
            ..retained
        } > GradedFitness {
            brake_progress_points: 0,
            ..maximum_progress
        }
    );
}

#[test]
fn graded_braking_rewards_subthreshold_progress_but_caps_positive_outputs() {
    assert_eq!(brake_points(-8.0), 0);
    assert_eq!(brake_points(-2.0), 0);
    for pair in [-2.0, -1.5, -1.0, -0.5, -0.125, 0.0, 0.0625, 0.125].windows(2) {
        assert!(brake_points(pair[0]) < brake_points(pair[1]));
    }
    assert_eq!(brake_points(0.125), 1000);
    assert_eq!(brake_points(8.0), 1000);
    assert!(!Movement::decode(&[0.0, 0.0, 0.0]).unwrap().brake);
    assert!(brake_points(0.0) < brake_points(0.125));
}

#[test]
fn graded_probe_matches_first_physical_decision_without_changing_base_fitness() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let cases: Vec<Case> = [2, 0]
        .map(|i| serde_json::from_value(plan["training_cases"][i].clone()).unwrap())
        .to_vec();
    let training = sources(cases.clone());
    let before = records(&training);
    let model = DenseModel::from_json(GENOME).unwrap();
    let protected = BTreeSet::from([cases[0].name.clone()]);
    let probes = brake_probes(&training, &protected);
    assert_eq!(probes.len(), 1);
    assert_eq!(probes[0].case, cases[1].name);
    let out = tempfile::tempdir().unwrap();
    episode(
        &training[1].1.lock().unwrap(),
        &cases[1],
        "model",
        &model,
        Some(out.path()),
    );
    let trace =
        fs::read_to_string(out.path().join(format!("{}-model.jsonl", cases[1].name))).unwrap();
    let first: Value = serde_json::from_str(trace.lines().next().unwrap()).unwrap();
    let inputs: [f32; 9] =
        serde_json::from_value(first["decision"]["encoded"]["inputs"].clone()).unwrap();
    let outputs: [f32; 3] =
        serde_json::from_value(first["decision"]["raw_outputs"].clone()).unwrap();
    assert_eq!(probes[0].inputs, inputs);
    assert_eq!(probe_outputs(&probes, &model).unwrap(), [outputs[2]]);
    assert_eq!(records(&training), before);
    let task = TrainingTask {
        sources: &training,
        retained: ship_curriculum::cases(Partition::Training)[..4].to_vec(),
        pool: EpisodePool::new(WorkerConfig::Fixed { workers: 2 }).unwrap(),
        manifest: TaskManifest {
            id: "graded-fixture".into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({}),
        },
    };
    let search = Search {
        task: &task,
        protected,
    };
    let control = Guided {
        search: &search,
        probes: &probes,
        arm: Arm::Control,
    };
    let graded = Guided {
        arm: Arm::Graded,
        ..control
    };
    let unchanged = search.evaluate_progress(&model, true).unwrap();
    assert_eq!(
        control.evaluate(&model).unwrap(),
        GradedFitness::from_retained(unchanged, 0)
    );
    assert_eq!(
        graded.evaluate(&model).unwrap(),
        GradedFitness::from_retained(unchanged, brake_points(outputs[2]))
    );
    assert_ne!(control.manifest(), graded.manifest());
    assert!(brake_probes(&training, &cases.iter().map(|c| c.name.clone()).collect()).is_empty());
}

#[test]
fn graded_recipe_keeps_inspected_membership_and_reserves_unseen_worlds() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let prior: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/data/learned-guidance-brake-bias-v1-plan.json"
    )))
    .unwrap();
    let focused: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/data/learned-guidance-focused-progress-v1-plan.json"
    )))
    .unwrap();
    assert_eq!(plan["training_cases"], prior["approach_cases"]);
    assert_eq!(plan["protected_cases"], prior["protected_cases"]);
    assert_eq!(plan["additional_cases"], prior["additional_cases"]);
    assert_eq!(configuration(&plan), configuration(&focused));
    assert_eq!(retention_cases(&plan), retention_cases(&prior));
    let cases = plan["training_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 98);
    let identities: BTreeSet<_> = cases
        .iter()
        .map(|c| ["seed", "shape", "seat", "source_tick", "planet"].map(|k| c[k].to_string()))
        .collect();
    assert_eq!(identities.len(), 98);
    let inspected: BTreeSet<_> = cases.iter().map(|c| c["seed"].as_u64().unwrap()).collect();
    let reserved: BTreeSet<_> = plan["validation_seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_u64().unwrap())
        .collect();
    assert_eq!(reserved.len(), 12);
    assert!(reserved.is_disjoint(&inspected));
    assert_eq!(plan["protected_cases"].as_array().unwrap().len(), 90);
    assert_eq!(plan["additional_cases"].as_array().unwrap().len(), 8);
    assert_eq!(plan["brake_progress"]["floor"], -2.0);
    assert_eq!(plan["brake_progress"]["target"], 0.125);
    assert_eq!(plan["brake_progress"]["points_per_case"], 1000);
    assert_eq!(retention_cases(&plan).len(), 157);
    assert_eq!(
        configuration(&plan).population * configuration(&plan).generations * (98 + 157) * 2,
        195840
    );
}
