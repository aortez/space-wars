use super::*;
use crate::ship_arrival::{ArrivalOutcome, DWELL_TICKS};

#[test]
fn partitions_are_physically_distinct_and_selection_manifest_contains_only_training() {
    let training = cases(Partition::Training);
    assert_eq!(training.len(), 4);
    assert_eq!(cases(Partition::StationaryValidation).len(), 4);
    assert_eq!(cases(Partition::MovingTransfer).len(), 3);
    let all = [
        Partition::Training,
        Partition::StationaryValidation,
        Partition::MovingTransfer,
    ]
    .into_iter()
    .flat_map(cases)
    .collect::<Vec<_>>();
    for (index, case) in all.iter().enumerate() {
        for other in &all[index + 1..] {
            assert_ne!(case.name, other.name);
            let mut renamed = other.clone();
            renamed.name = case.name.clone();
            assert_ne!(case, &renamed, "same physical case in two partitions");
        }
    }
    let manifest = TrainingTask.manifest();
    assert_ne!(manifest.id, ArrivalTask.manifest().id);
    assert_eq!(manifest.configuration["cases"], serde_json::json!(training));
    assert!(training.iter().all(|c| c.target_velocity == Vec2::ZERO));
    assert!(
        cases(Partition::MovingTransfer)
            .iter()
            .all(|c| c.target_velocity != Vec2::ZERO)
    );
    assert_eq!(ship_arrival::cases().len(), 8);
}

#[test]
fn selection_fitness_matches_training_partition_without_validation_contribution() {
    // A brake-only model survives the deadline and gives nonzero shaping, so
    // accidentally including validation cases would change this comparison.
    let mut genome = template();
    genome.layers[1].biases[2] = 1.0;
    let model = DenseModel::new(genome).unwrap();
    let selected = TrainingTask.evaluate(&model).unwrap();
    let report = evaluate(Policy::Model(model)).unwrap();
    assert_eq!(selected, report.partitions[0].fitness);
    assert!(selected.position_points > 0);
    assert!(report.partitions[1].fitness.position_points > 0);
    assert!(report.partitions[2].fitness.position_points > 0);
}

#[test]
fn reference_establishes_reachability_in_each_declared_partition() {
    let report = evaluate(Policy::Reference).unwrap();
    for partition in report.partitions {
        for episode in partition.episodes {
            eprintln!(
                "{} {:?} ticks={} distance={} speed={}",
                episode.case.name,
                episode.outcome,
                episode.elapsed_ticks,
                episode.measure.distance,
                episode.measure.relative_speed
            );
            assert_eq!(
                episode.outcome,
                ArrivalOutcome::Arrived,
                "{}",
                episode.case.name
            );
            assert_eq!(episode.measure.dwell, DWELL_TICKS);
            assert_eq!(episode.saturated_ticks, [0; 9]);
            assert_eq!(episode.released.unwrap().applied, [0.0; 3]);
        }
    }
}

#[test]
fn pinned_recipe_rejects_changed_budget_and_architecture_without_training() {
    let template = template();
    assert_eq!(template.parameter_count(), 107);
    assert_eq!(
        DenseModel::new(template.clone())
            .unwrap()
            .infer(&[0.0; 9])
            .unwrap(),
        vec![0.0; 3]
    );
    assert!(config(4).is_err());
    let run =
        EvolutionRun::new(&TrainingTask, template.clone(), vec![], config(1).unwrap()).unwrap();
    validate_run(&run).unwrap();
    let mut different = config(1).unwrap();
    different.generations -= 1;
    let changed = EvolutionRun::new(&TrainingTask, template, vec![], different).unwrap();
    assert!(validate_run(&changed).is_err());
    let changed = EvolutionRun::new(
        &TrainingTask,
        ship_arrival::neutral_genome(),
        vec![],
        config(1).unwrap(),
    )
    .unwrap();
    assert!(validate_run(&changed).is_err());
}

#[test]
fn saved_learned_genome_replays_arrivals_without_random_training() {
    let model = DenseModel::from_json(include_bytes!(
        "../../fixtures/ship-arrival-learning-v1/seed-3.json"
    ))
    .unwrap();
    assert_eq!(
        model.id(),
        "6c0ef53fb27e190243c84903855544a2cb9b97f4f32c92499dd37e541bbc41fb"
    );
    assert!(model.genome().same_architecture(&template()));
    let report = evaluate(Policy::Model(model)).unwrap();
    for partition in report.partitions {
        for episode in partition.episodes {
            assert_eq!(
                episode.outcome,
                ArrivalOutcome::Arrived,
                "{}",
                episode.case.name
            );
            assert_eq!(episode.measure.dwell, DWELL_TICKS);
            assert_eq!(episode.saturated_ticks, [0; 9]);
            assert_eq!(episode.released.unwrap().applied, [0.0; 3]);
        }
    }
}
