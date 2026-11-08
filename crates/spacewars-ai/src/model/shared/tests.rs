use super::*;

fn contract(inputs: usize) -> ModelContract {
    ModelContract {
        organism: "test_dot".into(),
        role: "movement_v1".into(),
        observation: "xy_sets_v1".into(),
        action: "force_v1".into(),
        inputs,
        outputs: 2,
        cadence_ticks: 1,
    }
}

/// Deterministic values in roughly ±1 with no exact zeros.
fn values(count: usize, salt: u32) -> Vec<f32> {
    (0..count as u32)
        .map(|i| ((i * 7919 + salt * 104_729) % 2001) as f32 / 1000.0 - 1.0005)
        .collect()
}

fn dense() -> DenseGenome {
    DenseGenome {
        version: MODEL_VERSION,
        contract: ModelContract {
            observation: "xy_v1".into(),
            ..contract(3)
        },
        memory: ModelMemory::None,
        layers: vec![
            DenseLayer {
                outputs: 4,
                activation: Activation::Tanh,
                weights: values(12, 1),
                biases: values(4, 2),
            },
            DenseLayer {
                outputs: 2,
                activation: Activation::Linear,
                weights: values(8, 3),
                biases: values(2, 4),
            },
        ],
    }
}

/// Five two-value items with a presence flag, then three three-value items.
fn encoders() -> Vec<SetEncoder> {
    vec![
        SetEncoder {
            items: 5,
            item_width: 2,
            presence: Some(0),
            pool: Pool::Max,
            layer: DenseLayer {
                outputs: 3,
                activation: Activation::Tanh,
                weights: values(6, 5),
                biases: values(3, 6),
            },
        },
        SetEncoder {
            items: 3,
            item_width: 3,
            presence: None,
            pool: Pool::Max,
            layer: DenseLayer {
                outputs: 2,
                activation: Activation::Tanh,
                weights: values(6, 7),
                biases: values(2, 8),
            },
        },
    ]
}

fn shared() -> SharedGenome {
    SharedGenome::from_dense(&dense(), contract(3 + 10 + 9), encoders()).unwrap()
}

#[test]
fn widened_dense_genome_reproduces_its_parent_bitwise() {
    let parent = DenseModel::new(dense()).unwrap();
    let child = SharedModel::new(shared()).unwrap();
    assert_eq!(child.genome().trunk[1], dense().layers[1]);
    assert_eq!(child.genome().trunk[0].weights.len(), 4 * (3 + 3 + 2));
    for salt in 0..64 {
        let direct = values(3, 100 + salt);
        let mut sets = values(19, 200 + salt);
        // Vary which items are present, including none.
        for item in 0..5 {
            sets[item * 2] = if (salt >> item) & 1 == 1 { 1.0 } else { 0.0 };
        }
        let mut inputs = direct.clone();
        inputs.extend(sets);
        let expected = parent.infer(&direct).unwrap();
        let actual = child.infer(&inputs).unwrap();
        assert_eq!(
            expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            "salt {salt}"
        );
    }
    let mut wrong = dense();
    wrong.layers[0].weights.pop();
    assert!(SharedGenome::from_dense(&wrong, contract(22), encoders()).is_err());
    assert!(SharedGenome::from_dense(&dense(), contract(21), encoders()).is_err());
}

#[test]
fn encoders_max_pool_present_items_and_report_empty_sets_as_zero() {
    // An item [presence, x] encodes to x; the trunk adds direct and pooled.
    let genome = SharedGenome {
        version: SHARED_MODEL_VERSION,
        contract: ModelContract {
            outputs: 1,
            ..contract(1 + 6)
        },
        memory: ModelMemory::None,
        direct_inputs: 1,
        encoders: vec![SetEncoder {
            items: 3,
            item_width: 2,
            presence: Some(0),
            pool: Pool::Max,
            layer: DenseLayer {
                outputs: 1,
                activation: Activation::Linear,
                weights: vec![0.0, 1.0],
                biases: vec![0.0],
            },
        }],
        trunk: vec![DenseLayer {
            outputs: 1,
            activation: Activation::Linear,
            weights: vec![1.0, 2.0],
            biases: vec![0.25],
        }],
    };
    let model = SharedModel::new(genome).unwrap();
    let infer = |inputs: &[f32]| model.infer(inputs).unwrap()[0];
    // The absent item's large value is ignored; 0.5 beats -0.25.
    assert_eq!(infer(&[1.0, 1.0, 0.5, 0.0, 9.0, 1.0, -0.25]), 2.25);
    assert_eq!(infer(&[1.0, 1.0, -0.5, 0.0, 9.0, 1.0, -0.25]), 0.75);
    assert_eq!(infer(&[1.0, 0.0, 0.5, 0.0, 9.0, -1.0, -0.25]), 1.25);
    // A negative pooled maximum is kept, not floored at zero.
    assert_eq!(infer(&[0.0, 1.0, -0.5, 1.0, -0.75, 0.0, 3.0]), -0.75);
    assert!(model.infer(&[1.0; 6]).is_err());
    assert!(
        model
            .infer(&[1.0, 1.0, f32::NAN, 0.0, 0.0, 0.0, 0.0])
            .is_err()
    );
}

#[test]
fn artifact_round_trip_preserves_parameters_identity_and_output() {
    let mut genome = shared();
    genome.encoders[0].layer.weights[0] = f32::from_bits(0x3dcccccd);
    genome.trunk[0].biases[0] = -0.0;
    let model = SharedModel::new(genome).unwrap();
    let loaded =
        SharedModel::from_json(&serde_json::to_vec_pretty(model.genome()).unwrap()).unwrap();
    assert_eq!(model.id(), loaded.id());
    let bits = |m: &SharedModel| {
        m.genome()
            .parameters()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    };
    assert_eq!(bits(&model), bits(&loaded));
    let inputs = values(22, 9);
    assert_eq!(
        model.infer(&inputs).unwrap(),
        loaded.infer(&inputs).unwrap()
    );

    let mut other = model.genome().clone();
    other.encoders[1].presence = Some(2);
    let other = SharedModel::new(other).unwrap();
    assert_ne!(model.id(), other.id());
    assert!(!other.genome().same_architecture(model.genome()));
    assert!(model.require_contract(&contract(22)).is_ok());
    assert!(model.require_contract(&contract(21)).is_err());
    // A V1 artifact is not a V2 genome.
    assert!(SharedModel::from_json(&serde_json::to_vec(&dense()).unwrap()).is_err());
    assert!(
        SharedModel::from_json(&vec![b' '; MAX_MODEL_BYTES + 1])
            .unwrap_err()
            .0
            .contains("size limit")
    );
}

#[test]
fn parameters_list_encoders_then_trunk() {
    let mut genome = shared();
    let count = 6 + 3 + 6 + 2 + 4 * 8 + 4 + 8 + 2;
    assert_eq!(genome.parameter_count(), count);
    assert_eq!(genome.parameters().count(), count);
    assert_eq!(
        genome.parameters().next(),
        genome.encoders[0].layer.weights.first()
    );
    assert_eq!(genome.parameters().last(), genome.trunk[1].biases.last());
    for p in genome.parameters_mut() {
        *p = 0.5;
    }
    assert!(genome.parameters().all(|p| *p == 0.5));
    assert!(genome.same_architecture(&shared()));
}

#[test]
fn invalid_versions_layouts_and_parameters_are_rejected() {
    let invalid = [
        {
            let mut g = shared();
            g.version = MODEL_VERSION;
            g
        },
        {
            let mut g = shared();
            g.encoders[0].items += 1;
            g
        },
        {
            let mut g = shared();
            g.encoders[0].presence = Some(2);
            g
        },
        {
            let mut g = shared();
            g.encoders[1].layer.weights.pop();
            g
        },
        {
            let mut g = shared();
            g.encoders[1].layer.biases.push(0.0);
            g
        },
        {
            let mut g = shared();
            g.direct_inputs -= 1;
            g
        },
        {
            let mut g = shared();
            g.trunk[0].weights.truncate(4 * 7);
            g
        },
        {
            let mut g = shared();
            g.trunk.clear();
            g
        },
        {
            let mut g = shared();
            g.contract.outputs = 3;
            g
        },
        {
            let mut g = shared();
            g.contract.role = "two words".into();
            g
        },
        {
            let mut g = shared();
            g.encoders[0].layer.biases[0] = f32::NAN;
            g
        },
        {
            let mut g = shared();
            g.encoders = vec![g.encoders[0].clone(); MAX_ENCODERS + 1];
            g
        },
    ];
    for g in invalid {
        assert!(SharedModel::new(g).is_err());
    }
}
