use super::*;

#[test]
fn match_retention_orders_successes_before_progress() {
    let complete = RetainedFitness {
        precision_completed: 157,
        protected_completed: 61,
        ..RetainedFitness::default()
    };
    let loses_match = RetainedFitness {
        protected_completed: 60,
        additional_completed: 12,
        approach_speed_points: u32::MAX,
        approach_position_points: u32::MAX,
        precision_position_points: u32::MAX,
        precision_velocity_points: u32::MAX,
        spare_ticks: u32::MAX,
        ..complete
    };
    assert!(complete > loses_match);
    assert!(
        complete
            > RetainedFitness {
                precision_completed: 156,
                protected_completed: 61,
                ..loses_match
            }
    );
    assert!(
        RetainedFitness {
            additional_completed: 1,
            ..complete
        } > complete
    );
}

#[test]
fn match_retention_scores_real_episodes_without_changing_their_quality() {
    let plan: Value = serde_json::from_slice(RECIPE).unwrap();
    let case: Case = serde_json::from_value(plan["training_cases"][4].clone()).unwrap();
    let training = sources(vec![case.clone()]);
    let model = DenseModel::from_json(GENOME).unwrap();
    let task = TrainingTask {
        sources: &training,
        retained: ship_curriculum::cases(Partition::Training)[..4].to_vec(),
        pool: EpisodePool::new(WorkerConfig::Fixed { workers: 2 }).unwrap(),
        manifest: TaskManifest {
            id: "match-retention-fixture".into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({}),
        },
    };
    let ordinary = task.evaluate(&model).unwrap();
    assert_eq!(ordinary.approach_completed, 1, "fixture must arrive");
    let protected = Search {
        task: &task,
        protected: BTreeSet::from([case.name]),
    };
    assert_eq!(
        protected.evaluate(&model).unwrap(),
        RetainedFitness::from_quality(ordinary, 1)
    );
    let additional = Search {
        task: &task,
        protected: BTreeSet::new(),
    };
    assert_eq!(
        additional.evaluate(&model).unwrap(),
        RetainedFitness::from_quality(ordinary, 0)
    );
}

#[test]
fn match_retention_recipe_separates_known_starts_from_reserved_worlds() {
    let plan: Value = serde_json::from_slice(RECIPE).unwrap();
    let previous: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/data/learned-guidance-brake-readout-v1-plan.json"
    )))
    .unwrap();
    assert_eq!(configuration(&plan), configuration(&previous));
    assert_eq!(retention_cases(&plan), retention_cases(&previous));
    let cases = plan["training_cases"].as_array().unwrap();
    let identities: BTreeSet<_> = cases
        .iter()
        .map(|c| ["seed", "shape", "seat", "source_tick", "planet"].map(|k| c[k].to_string()))
        .collect();
    assert_eq!(cases.len(), 73);
    assert_eq!(identities.len(), cases.len());
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    assert_eq!(protected.len(), 61);
    assert_eq!(
        plan["protected_cases"].as_array().unwrap().len(),
        protected.len()
    );
    assert!(
        protected
            .iter()
            .all(|name| cases.iter().any(|c| c["name"] == *name))
    );
    for shape in ["WeakAngular", "WeakRound"] {
        assert!(protected.contains(&format!("validation-8191-{shape}-p2")));
    }
    let seeds: BTreeSet<_> = cases.iter().map(|c| c["seed"].as_u64().unwrap()).collect();
    let reserved: BTreeSet<_> = plan["validation_seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_u64().unwrap())
        .collect();
    assert_eq!(reserved.len(), 4);
    assert!(seeds.is_disjoint(&reserved));
    assert_eq!(
        configuration(&plan).population
            * configuration(&plan).generations
            * (retention_cases(&plan).len() + cases.len()),
        88320
    );
}
