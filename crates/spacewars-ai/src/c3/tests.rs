use super::*;
use engine_rapier::world::polar::policy::{OBJECT_CAPACITY, PolicyGeometry};

#[test]
fn saved_genome_reproduces_tournament_draws_and_mutation() {
    let parent = C3Genome::random(42, 0);
    let restored = C3Genome::from_json(&parent.to_json().unwrap()).unwrap();
    assert_eq!(parent.id(), restored.id());
    assert_eq!(
        parent.id(),
        "4e01d97014080916dde5f152fa47118b7a940d28d0b5983e509bd01aa7967c4e"
    );
    assert_ne!(parent.id(), C3Genome::random(42, 1).id());
    let offspring = |genome: &C3Genome| {
        let mut draws = EvolutionDraws::new(42, 4, 2, b"offspring");
        let parent_rank = (0..3).map(|_| draws.index(8)).min();
        (parent_rank, genome.mutated(&mut draws, 0.17, 0.35).unwrap())
    };
    let (rank, child) = offspring(&parent);
    let (resumed_rank, resumed) = offspring(&restored);
    assert_eq!(rank, resumed_rank);
    assert_ne!(child.id(), parent.id());
    assert_eq!(child.id(), resumed.id());
    assert_eq!(child.to_json().unwrap(), resumed.to_json().unwrap());
    assert_eq!(child.parameter_count(), parent.parameter_count());
}

#[test]
fn stored_shape_schema_cadence_and_range_are_checked() {
    let genome = C3Genome::random(1, 0);
    for field in ["shape", "value", "cadence", "senses", "required"] {
        let mut file = genome.file.clone();
        match field {
            "shape" => file.weights.get_mut("query.weight").unwrap().shape[1] -= 1,
            "value" => file.weights.get_mut("query.bias").unwrap().values[1] = PARAMETER_LIMIT + 1.,
            "cadence" => file.cadence_ticks = 1,
            "senses" => file.binding.senses.push('2'),
            _ => file.binding.required_controls.clear(),
        }
        assert!(
            C3Genome::from_json(&serde_json::to_vec(&file).unwrap()).is_err(),
            "{field}"
        );
    }
}

#[test]
fn masked_objects_do_not_affect_actions_and_inputs_are_checked() {
    let model = C3Genome::random(0, 2);
    let mut obs = observation();
    let before = model.infer(&obs).unwrap();
    obs.objects[0][3] = 1.;
    assert_eq!(before, model.infer(&obs).unwrap());
    obs.objects[0][0] = 1.;
    assert_ne!(before, model.infer(&obs).unwrap());
    obs.objects[0][0] = 0.5;
    assert!(model.infer(&obs).is_err());
    obs.objects[0][0] = 0.;
    obs.geometry.rays.pop();
    assert!(model.infer(&obs).is_err());
}

fn observation() -> CreatureObservation {
    CreatureObservation {
        self_goal: [0.; 30],
        geometry: PolicyGeometry {
            cells: vec![[0., 0.]; 1472],
            rays: vec![[0.; 4]; 128],
            nearest: [0.; 6],
            ray_sources: vec![],
            nearest_source: None,
        },
        objects: [[0.; 18]; OBJECT_CAPACITY],
        object_sources: [None; OBJECT_CAPACITY],
    }
}

#[test]
fn contributions_and_bias_reconstruct_every_inferred_action() {
    let initial = C3Genome::random(7, 2);
    let model = initial
        .mutated(&mut EvolutionDraws::new(7, 1, 3, b"offspring"), 1., 0.5)
        .unwrap();
    let mut obs = observation();
    obs.self_goal[0] = -0.75;
    obs.geometry.cells[23] = [0.25, 0.5];
    obs.objects[0][0] = 1.;
    let trace = model.inspect(&obs).unwrap();
    assert_eq!(trace.outputs, model.infer(&obs).unwrap());
    for (action, contributions) in trace.contributions.iter().enumerate() {
        assert!(contributions.iter().any(|v| *v < 0.));
        assert!(contributions.iter().any(|v| *v > 0.));
        assert_ne!(trace.biases[action], 0.);
        let latent = contributions
            .iter()
            .fold(trace.biases[action], |sum, term| sum + term);
        assert_eq!(latent.tanh(), trace.outputs[action]);
    }
}
