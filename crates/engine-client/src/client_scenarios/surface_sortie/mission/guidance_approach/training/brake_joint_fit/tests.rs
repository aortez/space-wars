use super::*;

#[test]
fn joint_fit_hidden_export_round_trips_native_outputs() {
    let model = DenseModel::from_json(GENOME).unwrap();
    for i in 0..81 {
        let inputs = std::array::from_fn(|j| (((i * 17 + j * 29) % 101) as f32 - 50.0) / 50.0);
        let outputs: [f32; 3] = model.infer(&inputs).unwrap().try_into().unwrap();
        let row = record(
            &model,
            "match",
            "fixture",
            i as u64,
            inputs,
            outputs,
            outputs[2] > 0.0,
        );
        let decoded: Row = serde_json::from_slice(&serde_json::to_vec(&row).unwrap()).unwrap();
        assert_eq!(decoded.inputs, inputs);
        assert_eq!(decoded.hidden, row.hidden);
        assert_eq!(decoded.outputs, outputs);
    }
}

#[test]
fn joint_fit_rejects_non_brake_changes_and_detects_threshold_flips() {
    let model = DenseModel::from_json(GENOME).unwrap();
    let mut genome = model.genome().clone();
    genome.layers[1].weights[16..24].fill(0.0);
    genome.layers[1].biases[2] = 0.0;
    let baseline = DenseModel::new(genome.clone()).unwrap();
    genome.layers[1].biases[2] = f32::EPSILON;
    let candidate = DenseModel::new(genome.clone()).unwrap();
    assert_brake_only(&baseline, &candidate, 8.0);
    let outputs = baseline.infer(&[0.0; 9]).unwrap().try_into().unwrap();
    let rows = vec![
        record(
            &baseline,
            "precision",
            "preserve",
            1,
            [0.0; 9],
            outputs,
            false,
        ),
        record(&baseline, "match", "target", 2, [0.0; 9], outputs, false),
    ];
    let checked = static_check(
        &candidate,
        &rows,
        &BTreeSet::new(),
        &BTreeSet::from(["target".into()]),
    );
    assert_eq!(checked["flip_count"], 1);
    assert_eq!(checked["requested_on"], 1);
    assert_eq!(checked["passes"], false);
    genome.layers[1].weights[0] += 1.0;
    assert!(
        std::panic::catch_unwind(|| assert_brake_only(
            &baseline,
            &DenseModel::new(genome).unwrap(),
            8.0
        ))
        .is_err()
    );
}

#[test]
fn joint_fit_recipe_retains_the_full_inspected_cohort() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let previous: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/data/learned-guidance-brake-neighborhood-v1-plan.json"
    )))
    .unwrap();
    for field in [
        "approach_cases",
        "protected_cases",
        "additional_cases",
        "retention_partitions",
    ] {
        assert_eq!(plan[field], previous[field]);
    }
    assert_eq!(retention_cases(&plan).len(), 157);
    assert_eq!(
        plan["physical_validation"]["maximum_evaluation_episodes"],
        271 * 6
    );
    assert_eq!(plan["physical_validation"]["export_episodes"], 271 * 2);
    assert_eq!(plan["parameter_limit"], 8.0);
    assert_eq!(plan["training_or_production_change"], false);
}
