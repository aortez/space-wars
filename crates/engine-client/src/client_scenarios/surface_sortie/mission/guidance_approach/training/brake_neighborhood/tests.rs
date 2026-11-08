use super::*;

#[test]
fn brake_coordinate_changes_only_one_parameter_and_preserves_other_outputs() {
    let model = DenseModel::from_json(GENOME).unwrap();
    for coordinate in 0..9 {
        assert_eq!(shifted(&model, coordinate, 0.0).id(), model.id());
        for delta in [-1.0, -0.03125, 0.03125, 1.0] {
            let changed = shifted(&model, coordinate, delta);
            assert_eq!(
                model
                    .genome()
                    .parameters()
                    .zip(changed.genome().parameters())
                    .filter(|(a, b)| a != b)
                    .count(),
                1
            );
            for input in [-1.0, -0.25, 0.0, 0.25, 1.0] {
                let a = model.infer(&[input; 9]).unwrap();
                let b = changed.infer(&[input; 9]).unwrap();
                assert_eq!(a[..2], b[..2]);
                assert!((b[2] - a[2]).abs() <= delta.abs() + 0.00001);
                if coordinate == 8 {
                    assert!((b[2] - a[2] - delta).abs() < 0.00001);
                }
            }
        }
    }
}

#[test]
fn neighborhood_recipe_preserves_every_observed_start_and_declares_budget() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let cases: Vec<Case> = serde_json::from_value(plan["approach_cases"].clone()).unwrap();
    let identities: BTreeSet<_> = cases
        .iter()
        .map(|c| (c.seed, c.shape.label(), c.seat, c.source_tick, c.planet))
        .collect();
    assert_eq!(cases.len(), 114);
    assert_eq!(identities.len(), 114);
    let names: BTreeSet<_> = cases.iter().map(|c| c.name.clone()).collect();
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let additional: BTreeSet<String> =
        serde_json::from_value(plan["additional_cases"].clone()).unwrap();
    assert_eq!((protected.len(), additional.len()), (108, 6));
    assert!(protected.is_disjoint(&additional));
    assert_eq!(
        protected
            .union(&additional)
            .cloned()
            .collect::<BTreeSet<_>>(),
        names
    );
    assert_eq!(retention_cases(&plan).len(), 157);
    let offsets: Vec<f32> = serde_json::from_value(plan["offsets"].clone()).unwrap();
    assert_eq!(offsets.len(), 12);
    assert!(offsets.windows(2).all(|p| p[0] < p[1]));
    assert!(
        offsets
            .iter()
            .all(|d| d.is_finite() && *d != 0.0 && offsets.contains(&(-d)))
    );
    assert_eq!(2 + 9 * offsets.len() + 15, 125);
    assert_eq!(plan["sweep_episode_count"], 125 * (114 + 157));
}

#[test]
fn neighborhood_trace_selection_uses_declared_priorities_and_stable_ties() {
    let reports = vec![
        json!({"label":"baseline","family":"baseline","score":[157,108,0,1,1]}),
        json!({"label":"brake-on","family":"constant","score":[12,104,6,1,1]}),
        json!({"label":"kept","family":"coordinate","score":[157,108,0,2,1]}),
        json!({"label":"gain","family":"coordinate","score":[157,106,2,1,1]}),
        json!({"label":"more","family":"coordinate","score":[156,108,6,1,1]}),
        json!({"label":"tie","family":"coordinate","score":[157,106,2,1,1]}),
        json!({"label":"neighbor","family":"population","score":[157,108,0,1,1]}),
    ];
    assert_eq!(trace_indices(&reports), [0, 1, 2, 3, 6]);
}
