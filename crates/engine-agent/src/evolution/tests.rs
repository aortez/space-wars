use super::*;
use crate::line_diagnostic::{KnownPolicy, LineFitness, LineTask, known_genome};

fn config() -> EvolutionConfig {
    EvolutionConfig {
        population: 6,
        generations: 3,
        elites: 1,
        mutation_probability: 1.0,
        ..EvolutionConfig::default()
    }
}

fn new_run() -> EvolutionRun<LineFitness> {
    EvolutionRun::new(
        &LineTask::default(),
        known_genome(KnownPolicy::Oracle),
        vec![],
        config(),
    )
    .unwrap()
}

#[test]
fn historical_inspection_preserves_old_protocols_without_allowing_resume() {
    let mut run = new_run();
    finish(&mut run);
    run.runtime.push_str("/old-build");
    run.task.id.push_str("/old-recipe");
    let bytes = run.encode().unwrap();
    let historical = EvolutionRun::<LineFitness>::inspect(&bytes).unwrap();
    assert_eq!(historical, run);
    assert!(EvolutionRun::<LineFitness>::decode(&bytes, &LineTask::default()).is_err());
    run.history[0].best_id = "invalid".into();
    assert!(EvolutionRun::<LineFitness>::inspect(&run.encode().unwrap()).is_err());
}

fn finish(run: &mut EvolutionRun<LineFitness>) {
    while run.advance(&LineTask::default()).unwrap() {}
}

#[test]
fn seed_and_recipe_reproduce_the_entire_run() {
    let mut a = new_run();
    let mut b = new_run();
    finish(&mut a);
    finish(&mut b);
    assert_eq!(a, b);
    assert_eq!(a.history().len(), 3);
    assert!(
        a.history()
            .windows(2)
            .all(|pair| pair[0].best_fitness <= pair[1].best_fitness)
    );
    let different = EvolutionRun::new(
        &LineTask::default(),
        known_genome(KnownPolicy::Oracle),
        vec![],
        EvolutionConfig {
            seed: 2,
            ..config()
        },
    )
    .unwrap();
    assert_ne!(different.population(), new_run().population());
}

#[test]
fn known_winner_survives_selection_and_offspring_have_bounded_mutations() {
    let oracle = known_genome(KnownPolicy::Oracle);
    let id = DenseModel::new(oracle.clone()).unwrap().id().to_owned();
    let mut seeds = vec![known_genome(KnownPolicy::Idle); config().population];
    seeds[0] = oracle.clone();
    let mut run = EvolutionRun::new(&LineTask::default(), oracle.clone(), seeds, config()).unwrap();
    for _ in 0..config().population {
        assert!(run.advance(&LineTask::default()).unwrap());
    }
    assert_eq!(run.generation(), 1);
    assert_eq!(run.history()[0].best_id, id);
    assert_eq!(run.history()[0].best_fitness.completed, 81);
    assert_eq!(run.population()[0].genome, oracle);
    assert!(!run.population()[0].mutated);
    for candidate in run.population() {
        assert!(
            run.history()[0]
                .population_ids
                .contains(candidate.source_genome.as_ref().unwrap())
        );
        assert!(
            candidate
                .genome
                .parameters()
                .all(|p| p.abs() <= run.config().parameter_limit)
        );
    }
    assert!(run.population()[1..].iter().all(|c| c.mutated));
    assert!(run.fitness().iter().all(Option::is_none));
}

#[test]
fn checkpoints_at_initial_partial_boundary_and_complete_states_resume_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    let mut control = new_run();
    finish(&mut control);
    for advances in [0, 1, 5, 6, 7, 17, 18] {
        let mut run = new_run();
        for _ in 0..advances {
            assert!(run.advance(&LineTask::default()).unwrap());
        }
        run.save(&path).unwrap();
        let mut restored = EvolutionRun::load(&path, &LineTask::default()).unwrap();
        assert_eq!(run, restored);
        finish(&mut restored);
        assert_eq!(
            control, restored,
            "checkpoint after {advances} candidate batches"
        );
        restored.save(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), {
            control.save(&path).unwrap();
            std::fs::read(&path).unwrap()
        });
    }
}

#[test]
fn corruption_and_unsupported_checkpoint_versions_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    let run = new_run();
    run.save(&path).unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    json["sha256"] = "incorrect".into();
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(
        EvolutionRun::load(&path, &LineTask::default())
            .unwrap_err()
            .0
            .contains("checksum")
    );
    run.save(&path).unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    json["version"] = 2.into();
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(EvolutionRun::load(&path, &LineTask::default()).is_err());
}

#[test]
fn valid_checksum_does_not_bypass_runtime_recipe_or_progress_validation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    for change in 0..5 {
        let mut run = new_run();
        match change {
            0 => run.runtime.push_str("/different_build"),
            1 => run.task.configuration["world"] = "different_world".into(),
            2 => run.fitness[1] = Some(LineFitness::default()),
            3 => run.generation = 1,
            4 => run.population[0].genome.layers[0].weights[0] = 100.0,
            _ => unreachable!(),
        }
        run.save(&path).unwrap();
        assert!(
            EvolutionRun::load(&path, &LineTask::default()).is_err(),
            "change {change}"
        );
    }
}

#[test]
fn failed_evaluations_leave_a_retryable_slot_and_no_fake_score() {
    struct FailingTask;
    impl EvaluationTask for FailingTask {
        type Fitness = LineFitness;
        fn manifest(&self) -> TaskManifest {
            LineTask::default().manifest()
        }
        fn evaluate(&self, _: &DenseModel) -> Result<LineFitness, EvolutionError> {
            Err(EvolutionError("worker unavailable".into()))
        }
    }
    let mut run = new_run();
    let before = run.clone();
    assert_eq!(
        run.advance(&FailingTask).unwrap_err().0,
        "worker unavailable"
    );
    assert_eq!(run, before);
    assert!(run.advance(&LineTask::default()).unwrap());
    assert!(run.fitness()[0].is_some());
}

#[test]
fn changed_recipe_cannot_continue_existing_results() {
    struct DifferentTask;
    impl EvaluationTask for DifferentTask {
        type Fitness = LineFitness;
        fn manifest(&self) -> TaskManifest {
            let mut manifest = LineTask::default().manifest();
            manifest.id = "changed".into();
            manifest
        }
        fn evaluate(&self, _: &DenseModel) -> Result<LineFitness, EvolutionError> {
            panic!("must not evaluate")
        }
    }
    let mut run = new_run();
    assert!(
        run.advance(&DifferentTask)
            .unwrap_err()
            .0
            .contains("recipe changed")
    );
}

#[test]
fn seed_layout_and_parameter_range_must_match_the_experiment() {
    let template = known_genome(KnownPolicy::Oracle);
    let mut alternate = template.clone();
    alternate.layers[0].activation = spacewars_ai::model::Activation::Tanh;
    // Both execute the same I/O contract, but fixed-layout evolution rejects the seed.
    DenseModel::new(alternate.clone())
        .unwrap()
        .require_contract(&template.contract)
        .unwrap();
    assert!(
        EvolutionRun::new(
            &LineTask::default(),
            template.clone(),
            vec![alternate],
            config()
        )
        .is_err()
    );
    let mut out_of_range = template.clone();
    out_of_range.layers[0].biases[0] = 100.0;
    assert!(
        EvolutionRun::new(
            &LineTask::default(),
            template.clone(),
            vec![out_of_range],
            config()
        )
        .is_err()
    );
    assert!(
        EvolutionRun::new(
            &LineTask::default(),
            template.clone(),
            vec![template; 7],
            config()
        )
        .is_err()
    );
}

#[test]
fn invalid_population_and_operator_settings_are_rejected() {
    for config in [
        EvolutionConfig {
            population: 1,
            ..config()
        },
        EvolutionConfig {
            elites: 6,
            ..config()
        },
        EvolutionConfig {
            generations: 0,
            ..config()
        },
        EvolutionConfig {
            mutation_probability: f32::NAN,
            ..config()
        },
        EvolutionConfig {
            parameter_limit: 0.0,
            ..config()
        },
        EvolutionConfig {
            mutation_scale: 9.0,
            ..config()
        },
    ] {
        assert!(
            EvolutionRun::new(
                &LineTask::default(),
                known_genome(KnownPolicy::Oracle),
                vec![],
                config
            )
            .is_err()
        );
    }
}

#[test]
fn failed_storage_does_not_destroy_an_existing_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    let run = new_run();
    run.save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(run.save(&path.join("unwritable.json")).is_err());
    assert_eq!(original, std::fs::read(&path).unwrap());
    assert_eq!(
        run,
        EvolutionRun::load(&path, &LineTask::default()).unwrap()
    );
    assert!(
        read_bounded(&path, 10)
            .unwrap_err()
            .0
            .contains("size limit")
    );
}
