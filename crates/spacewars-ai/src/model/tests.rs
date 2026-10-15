use super::*;

fn genome() -> DenseGenome {
    DenseGenome {
        version: MODEL_VERSION,
        contract: ModelContract {
            organism: "test_dot".into(),
            role: "movement_v1".into(),
            observation: "xy_v1".into(),
            action: "force_v1".into(),
            inputs: 2,
            outputs: 1,
            cadence_ticks: 1,
        },
        memory: ModelMemory::None,
        layers: vec![
            DenseLayer {
                outputs: 2,
                activation: Activation::Linear,
                weights: vec![2.0, -1.0, 0.5, 3.0],
                biases: vec![0.25, -0.5],
            },
            DenseLayer {
                outputs: 1,
                activation: Activation::Tanh,
                weights: vec![0.5, -0.25],
                biases: vec![0.125],
            },
        ],
    }
}

#[test]
fn row_major_layers_biases_and_activation_match_manual_arithmetic() {
    let model = DenseModel::new(genome()).unwrap();
    // [1, 0.5] -> [1.75, 1.5] -> tanh(0.625).
    assert_eq!(model.infer(&[1.0, 0.5]).unwrap(), vec![0.625_f32.tanh()]);
    model.infer(&[-1.0, 1.0]).unwrap();
    assert_eq!(
        model.clone().infer(&[1.0, 0.5]).unwrap(),
        vec![0.625_f32.tanh()]
    );
}

#[test]
fn artifact_round_trip_preserves_parameters_identity_and_output() {
    let mut genome = genome();
    genome.layers[0].weights[0] = f32::from_bits(0x3dcccccd);
    genome.layers[0].biases[0] = -0.0;
    let model = DenseModel::new(genome).unwrap();
    let loaded =
        DenseModel::from_json(&serde_json::to_vec_pretty(model.genome()).unwrap()).unwrap();
    assert_eq!(model.id(), loaded.id());
    let bits = |m: &DenseModel| {
        m.genome()
            .parameters()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    };
    assert_eq!(bits(&model), bits(&loaded));
    assert_eq!(
        model.infer(&[0.7, -0.3]).unwrap(),
        loaded.infer(&[0.7, -0.3]).unwrap()
    );

    let mut other = model.genome().clone();
    other.contract.organism = "another_dot".into();
    let other = DenseModel::new(other).unwrap();
    assert_ne!(model.id(), other.id());
    assert!(other.require_contract(&model.genome().contract).is_err());
    let mut expected = model.genome().contract.clone();
    expected.cadence_ticks = 2;
    assert!(model.require_contract(&expected).is_err());
}

#[test]
fn execution_compatibility_does_not_imply_matching_parameter_layout() {
    let original = genome();
    let mut alternate = original.clone();
    alternate.layers = vec![DenseLayer {
        outputs: 1,
        activation: Activation::Linear,
        weights: vec![1.0, 2.0],
        biases: vec![0.0],
    }];
    DenseModel::new(alternate.clone())
        .unwrap()
        .require_contract(&original.contract)
        .unwrap();
    assert!(!original.same_architecture(&alternate));
}

#[test]
fn invalid_parameters_versions_shapes_and_contracts_are_rejected() {
    let invalid = [
        {
            let mut g = genome();
            g.version += 1;
            g
        },
        {
            let mut g = genome();
            g.layers[0].weights.pop();
            g
        },
        {
            let mut g = genome();
            g.layers[1].biases.clear();
            g
        },
        {
            let mut g = genome();
            g.layers[0].weights[0] = f32::NAN;
            g
        },
        {
            let mut g = genome();
            g.layers[0].biases[0] = f32::INFINITY;
            g
        },
        {
            let mut g = genome();
            g.contract.outputs = 2;
            g
        },
        {
            let mut g = genome();
            g.contract.inputs = 0;
            g
        },
        {
            let mut g = genome();
            g.contract.cadence_ticks = 0;
            g
        },
        {
            let mut g = genome();
            g.contract.action = "ambiguous action".into();
            g
        },
        {
            let mut g = genome();
            g.layers.clear();
            g
        },
    ];
    for g in invalid {
        assert!(DenseModel::new(g).is_err());
    }
    let mut large = genome();
    large.contract.inputs = 256;
    large.contract.outputs = 256;
    large.layers = vec![DenseLayer {
        outputs: 256,
        activation: Activation::Linear,
        weights: vec![0.0; 256 * 256],
        biases: vec![0.0; 256],
    }];
    assert!(
        DenseModel::new(large)
            .unwrap_err()
            .0
            .contains("parameter limit")
    );
}

#[test]
fn unsupported_json_and_oversized_artifacts_are_rejected() {
    let mut value = serde_json::to_value(genome()).unwrap();
    value["memory"] = "recurrent".into();
    assert!(DenseModel::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    value["memory"] = "none".into();
    value["extra"] = true.into();
    assert!(DenseModel::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(
        DenseModel::from_json(&vec![b' '; MAX_MODEL_BYTES + 1])
            .unwrap_err()
            .0
            .contains("size limit")
    );
}

#[test]
fn non_finite_inputs_and_overflow_do_not_become_usable_outputs() {
    let model = DenseModel::new(genome()).unwrap();
    assert!(model.infer(&[1.0]).is_err());
    assert!(model.infer(&[0.0, f32::INFINITY]).is_err());
    let mut g = genome();
    g.layers[0].weights[0] = f32::MAX;
    assert!(
        DenseModel::new(g)
            .unwrap()
            .infer(&[2.0, 0.0])
            .unwrap_err()
            .0
            .contains("non-finite")
    );
}
