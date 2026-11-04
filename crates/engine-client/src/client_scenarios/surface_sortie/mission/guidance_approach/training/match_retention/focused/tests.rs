use super::*;

#[test]
#[ignore = "replays a match case recorded on the experiment machine; another C math library flies the match elsewhere"]
fn progress_scope_changes_only_progress_and_excludes_protected_cases() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let cases: Vec<Case> = [2, 4]
        .map(|i| serde_json::from_value(plan["training_cases"][i].clone()).unwrap())
        .to_vec();
    let training = sources(cases.clone());
    let model = DenseModel::from_json(GENOME).unwrap();
    let task = TrainingTask {
        sources: &training,
        retained: ship_curriculum::cases(Partition::Training)[..4].to_vec(),
        pool: EpisodePool::new(WorkerConfig::Fixed { workers: 2 }).unwrap(),
        manifest: TaskManifest {
            id: "focused-progress-fixture".into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({}),
        },
    };
    let search = Search {
        task: &task,
        protected: BTreeSet::from([cases[0].name.clone()]),
    };
    let control = Scoped {
        search: &search,
        scope: Scope::All,
    };
    let focused = Scoped {
        search: &search,
        scope: Scope::Remaining,
    };
    let old = search.evaluate(&model).unwrap();
    assert_eq!(control.evaluate(&model).unwrap(), old);
    let mut expected = old;
    let mut quality = Fitness::default();
    quality.record_approach(&episode(
        &training[1].1.lock().unwrap(),
        &cases[1],
        "model",
        &model,
        None,
    ));
    expected.approach_speed_points = quality.approach_speed_points;
    expected.approach_position_points = quality.approach_position_points;
    assert!(expected.approach_speed_points < old.approach_speed_points);
    assert_eq!(focused.evaluate(&model).unwrap(), expected);
    assert_ne!(control.manifest(), focused.manifest());
    let all_protected = Search {
        task: &task,
        protected: cases.into_iter().map(|c| c.name).collect(),
    };
    let score = all_protected.evaluate_progress(&model, true).unwrap();
    assert_eq!(score.approach_speed_points, 0);
    assert_eq!(score.approach_position_points, 0);
}

#[test]
fn complete_world_requires_each_seat_and_shape_once() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let template: Case = serde_json::from_value(plan["training_cases"][0].clone()).unwrap();
    let mut cases = Vec::new();
    for shape in [DirtMode::WeakAngular, DirtMode::WeakRound] {
        for seat in 0..2 {
            cases.push(Case {
                shape,
                seat,
                ..template.clone()
            });
        }
    }
    assert!(complete_world(&cases, template.seed));
    assert!(!complete_world(&cases[..3], template.seed));
    assert!(!complete_world(&cases, template.seed + 1));
    cases[3] = cases[0].clone();
    assert!(!complete_world(&cases, template.seed));
}

#[test]
fn focused_recipe_fixes_membership_and_reserves_new_worlds() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let previous: Value = serde_json::from_slice(RECIPE).unwrap();
    assert_eq!(configuration(&plan), configuration(&previous));
    assert_eq!(retention_cases(&plan), retention_cases(&previous));
    let cases = plan["training_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 82);
    let names: BTreeSet<_> = cases.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 82);
    let protected: BTreeSet<_> = plan["protected_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    let remaining: BTreeSet<_> = plan["additional_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(protected.len(), 74);
    assert_eq!(remaining.len(), 8);
    assert!(protected.is_disjoint(&remaining));
    assert_eq!(
        protected
            .union(&remaining)
            .copied()
            .collect::<BTreeSet<_>>(),
        names
    );
    let seeds: BTreeSet<_> = cases
        .iter()
        .map(|c| c["seed"].as_u64().unwrap())
        .chain(
            previous["validation_seeds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_u64().unwrap()),
        )
        .collect();
    let reserved: BTreeSet<_> = plan["validation_seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_u64().unwrap())
        .collect();
    assert_eq!(reserved.len(), 12);
    assert!(seeds.is_disjoint(&reserved));
    assert_eq!(
        configuration(&plan).population * configuration(&plan).generations * (82 + 157) * 2,
        183552
    );
}
