use super::*;

#[test]
fn extracted_readout_roundtrips_and_cannot_change_other_outputs() {
    let baseline = DenseModel::from_json(GENOME).unwrap();
    let head = Arm::Brake.seed(&baseline);
    assert_eq!(head.genome().parameter_count(), 9);
    assert_eq!(Arm::Brake.expand(&head, &baseline).id(), baseline.id());
    let mut changed = head.genome().clone();
    changed.layers[0].weights = vec![0.25, -0.5, 1.0, 0.125, -0.75, 0.0, 0.33, -0.2];
    changed.layers[0].biases[0] = 0.8;
    let expanded = Arm::Brake.expand(&DenseModel::new(changed).unwrap(), &baseline);
    assert_eq!(expanded.genome().layers[0], baseline.genome().layers[0]);
    assert_eq!(
        expanded.genome().layers[1].weights[..16],
        baseline.genome().layers[1].weights[..16]
    );
    assert_eq!(
        expanded.genome().layers[1].biases[..2],
        baseline.genome().layers[1].biases[..2]
    );
    for sample in 0..129 {
        let inputs = std::array::from_fn::<_, 9, _>(|i| {
            ((sample * (i + 3) + i * 7) % 101) as f32 / 50.0 - 1.0
        });
        assert_eq!(
            expanded.infer(&inputs).unwrap()[..2],
            baseline.infer(&inputs).unwrap()[..2]
        );
    }
}

#[test]
fn both_search_spaces_evaluate_the_same_starting_controller() {
    let baseline = DenseModel::from_json(GENOME).unwrap();
    let task = TrainingTask {
        sources: &[],
        retained: ship_curriculum::cases(Partition::Training)[..4].to_vec(),
        pool: EpisodePool::new(WorkerConfig::Fixed { workers: 2 }).unwrap(),
        manifest: TaskManifest {
            id: "brake-readout-fixture".into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({}),
        },
    };
    let full = Search {
        task: &task,
        baseline: &baseline,
        arm: Arm::All,
    };
    let brake = Search {
        task: &task,
        baseline: &baseline,
        arm: Arm::Brake,
    };
    assert_eq!(
        full.evaluate(&baseline).unwrap(),
        brake.evaluate(&Arm::Brake.seed(&baseline)).unwrap()
    );
    assert_ne!(full.manifest(), brake.manifest());
}

#[test]
fn readout_comparison_keeps_curriculum_and_reserves_unseen_worlds() {
    let previous: Value = serde_json::from_slice(LEARNING_PLAN_V3).unwrap();
    let plan: Value = serde_json::from_slice(RECIPE).unwrap();
    for field in ["training_cases", "evolution", "fitness_order"] {
        assert_eq!(plan[field], previous[field]);
    }
    assert_eq!(retention_cases(&plan), retention_cases(&previous));
    assert_eq!(retention_cases(&plan).len(), 157);
    assert_eq!(plan["development_cases"].as_array().unwrap().len(), 54);
    let inspected: std::collections::BTreeSet<_> = plan["training_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(plan["development_cases"].as_array().unwrap())
        .map(|c| c["seed"].as_u64().unwrap())
        .collect();
    for seed in plan["validation_seeds"].as_array().unwrap() {
        assert!(!inspected.contains(&seed.as_u64().unwrap()));
    }
    assert_eq!(
        configuration(&plan).population * configuration(&plan).generations * 172 * 2,
        132096
    );
}
