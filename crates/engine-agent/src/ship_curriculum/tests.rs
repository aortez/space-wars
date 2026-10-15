use super::*;

#[test]
fn recipe_and_evaluation_schedule_match_the_pretraining_plan() {
    let frozen: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/data/ship-arrival-mixed-v1-plan.json"
    ))
    .unwrap();
    let current = manifest().unwrap();
    assert_eq!(frozen["id"], current.id);
    assert_eq!(frozen["seed_id"], current.seed_id);
    assert_eq!(frozen["initialization"], current.initialization);
    assert_eq!(frozen["operators"], current.operators);
    assert_eq!(frozen["finalist_selection"], current.finalist_selection);
    assert_eq!(frozen["gate"], current.gate);
    let training: TaskManifest = serde_json::from_value(frozen["training"].clone()).unwrap();
    assert_eq!(training, current.training);
    // Typed decoding restores the declared f32 precision of physical/model
    // quantities instead of comparing JSON formatting of float decimals.
    let configs: Vec<EvolutionConfig> = serde_json::from_value(frozen["runs"].clone()).unwrap();
    assert_eq!(configs, current.runs);
    let genome: spacewars_ai::model::DenseGenome =
        serde_json::from_value(frozen["seed_genome"].clone()).unwrap();
    assert_eq!(genome, current.seed_genome);
    let expected = frozen["partitions"].as_array().unwrap();
    assert_eq!(expected.len(), current.partitions.len());
    for (saved, schedule) in expected.iter().zip(current.partitions) {
        assert_eq!(
            saved["partition"],
            serde_json::to_value(schedule.partition).unwrap()
        );
        let cases: Vec<ArrivalCase> = serde_json::from_value(saved["cases"].clone()).unwrap();
        assert_eq!(cases, schedule.cases);
    }
    // Runtime identity is executable-specific, so rebuilt tests do not compare
    // their binary hash with the preserved experiment executable.
}

#[test]
fn schedule_preserves_declared_overlap_and_fresh_cases_are_distinct() {
    let training = cases(Partition::Training);
    assert_eq!(training.len(), 12);
    assert_eq!(
        training[..4],
        ship_training::cases(ship_training::Partition::Training)
    );
    assert!(
        training[4..8]
            .iter()
            .all(|c| c.target_velocity != Vec2::ZERO)
    );
    assert!(
        training[8..]
            .iter()
            .all(|c| c.start.altitude <= 75.0 && c.target_offset.y < 0.0)
    );
    assert_eq!(cases(Partition::Original).len(), 11);
    let development = cases(Partition::Development);
    assert_eq!(development.len(), 96);
    assert_eq!(
        training.iter().filter(|c| development.contains(c)).count(),
        6
    );
    let fresh = [
        Partition::FreshStationary,
        Partition::FreshMoving,
        Partition::FreshInward,
    ]
    .into_iter()
    .flat_map(|p| {
        let cases = cases(p);
        assert_eq!(cases.len(), 16);
        cases
    })
    .collect::<Vec<_>>();
    let mut seen = [training, cases(Partition::Original), development].concat();
    for case in fresh {
        for other in &seen {
            assert_ne!(case.name, other.name);
            let mut renamed = other.clone();
            renamed.name = case.name.clone();
            assert_ne!(case, renamed, "overlapping physical case: {}", case.name);
        }
        seen.push(case);
    }
    assert_eq!(
        TrainingTask.manifest().configuration["cases"],
        serde_json::json!(cases(Partition::Training))
    );
    assert_ne!(
        TrainingTask.manifest().id,
        ship_training::TrainingTask.manifest().id
    );
}

#[test]
fn all_new_cases_have_valid_initial_conditions_and_marker_paths() {
    // Check only fixture validity; fresh scores remain uninspected until after
    // all declared finalists have been selected by training fitness.
    for partition in [
        Partition::Training,
        Partition::FreshStationary,
        Partition::FreshMoving,
        Partition::FreshInward,
    ] {
        for case in cases(partition) {
            let name = case.name.clone();
            ship_arrival::run_episode(case, Policy::Idle, false)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}

#[test]
fn stationary_retention_cannot_be_traded_for_more_total_arrivals() {
    let retained = CurriculumFitness {
        stationary_completed: 4,
        total: ArrivalFitness {
            completed: 4,
            ..ArrivalFitness::default()
        },
    };
    let traded = CurriculumFitness {
        stationary_completed: 3,
        total: ArrivalFitness {
            completed: 11,
            position_points: 12000,
            velocity_points: 12000,
            spare_ticks: 14400,
        },
    };
    assert!(retained > traded);
    let improved = CurriculumFitness {
        total: ArrivalFitness {
            completed: 5,
            ..retained.total
        },
        ..retained
    };
    assert!(improved > retained);
}

#[test]
fn training_fitness_contains_only_declared_cases_and_retention_subset() {
    let model = ship_qualification::candidate().unwrap();
    let fitness = TrainingTask.evaluate(&model).unwrap();
    let mut total = ArrivalFitness::default();
    for case in cases(Partition::Training) {
        let score = ship_arrival::run_episode(case, Policy::Model(model.clone()), false)
            .unwrap()
            .fitness;
        total.completed += score.completed;
        total.position_points += score.position_points;
        total.velocity_points += score.velocity_points;
        total.spare_ticks += score.spare_ticks;
    }
    assert_eq!(fitness.stationary_completed, 4);
    assert_eq!(fitness.total, total);
}

#[test]
fn continuation_pins_seed_artifact_budget_and_initial_population_layout() {
    let run = new_run(1).unwrap();
    validate_run(&run).unwrap();
    let seeded = &run.population()[0];
    assert_eq!(
        seeded.source_genome.as_deref(),
        Some(ship_qualification::CANDIDATE)
    );
    assert!(!seeded.mutated);
    assert_eq!(
        DenseModel::new(seeded.genome.clone()).unwrap().id(),
        ship_qualification::CANDIDATE
    );
    assert_eq!(run.population().len(), 32);
    assert!(
        run.population()[1..]
            .iter()
            .all(|c| c.source_genome.is_none())
    );
    assert_eq!(run.config().generations, 32);
    assert_eq!(run.population(), new_run(1).unwrap().population());
    assert_ne!(run.population()[1..], new_run(2).unwrap().population()[1..]);
    assert!(new_run(4).is_err());
    let without_seed = EvolutionRun::new(
        &TrainingTask,
        ship_training::template(),
        vec![],
        config(1).unwrap(),
    )
    .unwrap();
    assert!(validate_run(&without_seed).is_err());
    let mut changed = config(1).unwrap();
    changed.generations += 1;
    let changed = EvolutionRun::new(
        &TrainingTask,
        ship_training::template(),
        run.seeds().to_vec(),
        changed,
    )
    .unwrap();
    assert!(validate_run(&changed).is_err());
}

#[test]
fn retained_mixed_genomes_replay_the_complete_acceptance_gate_without_training() {
    for (json, expected_id) in [
        (
            include_bytes!("../../fixtures/ship-arrival-mixed-v1/seed-1.json").as_slice(),
            "693447c6df8315017a67f8dba8809568cf08f3ec66131302bfd026fa49f343fb",
        ),
        (
            include_bytes!("../../fixtures/ship-arrival-mixed-v1/seed-2.json").as_slice(),
            "96124d0afeb69f7d5e61bc8c8b46e6b94260c0b1962197e1bde81e05ae3d9f9b",
        ),
        (
            include_bytes!("../../fixtures/ship-arrival-mixed-v1/seed-3.json").as_slice(),
            "7f878c6e0632673753a782d0ff251ad8635a317796b1d036a6862e6ac49c5b55",
        ),
    ] {
        let model = DenseModel::from_json(json).unwrap();
        assert_eq!(model.id(), expected_id);
        let report = evaluate(Policy::Model(model)).unwrap();
        assert!(report.passed, "{expected_id}");
        assert_eq!(report.training_fitness.stationary_completed, 4);
        assert_eq!(report.training_fitness.total.completed, 12);
        for partition in report.partitions {
            for episode in partition.episodes {
                assert_eq!(
                    episode.outcome,
                    ArrivalOutcome::Arrived,
                    "{} {expected_id}",
                    episode.case.name
                );
                assert_eq!(episode.saturated_ticks, [0; 9]);
            }
        }
    }
}
