use super::*;

#[test]
fn reference_reaches_every_fixed_stationary_and_moving_target() {
    for case in cases() {
        let episode = run_episode(case, Policy::Reference, false).unwrap();
        eprintln!(
            "{} {:?} ticks={} distance={} speed={} clipped={:?}",
            episode.case.name,
            episode.outcome,
            episode.elapsed_ticks,
            episode.measure.distance,
            episode.measure.relative_speed,
            episode.saturated_ticks
        );
        assert_eq!(
            episode.outcome,
            ArrivalOutcome::Arrived,
            "{}",
            episode.case.name
        );
        assert_eq!(episode.fitness.completed, 1);
        assert_eq!(episode.measure.dwell, DWELL_TICKS);
        assert_eq!(episode.saturated_ticks, [0; 9]);
        assert_eq!(episode.setup_ticks, 1);
        assert_eq!(episode.released.as_ref().unwrap().applied, [0.0; 3]);
    }
}

#[test]
fn dwell_requires_both_position_and_velocity_and_resets_on_a_flyby() {
    let (_, mut o, target, _) = setup(&cases()[0]).unwrap();
    o.ship.position = target.position;
    o.ship.velocity = target.velocity;
    assert_eq!(Measure::read(&o, target, 10).dwell, 11);
    o.ship.velocity = target.velocity + Vec2::X * 10.0;
    assert_eq!(Measure::read(&o, target, 10).dwell, 0);
    o.ship.velocity = target.velocity;
    o.ship.position += Vec2::Y * 3.0;
    assert_eq!(Measure::read(&o, target, 10).dwell, 0);
}

#[test]
fn setup_errors_are_not_scored_failures_and_unsafe_transitions_cannot_count_as_arrival() {
    let mut low = cases()[0].clone();
    low.start.altitude = 15.0;
    assert!(run_episode(low, Policy::Reference, false).is_err());
    let mut invalid = cases()[0].clone();
    invalid.start.altitude = f32::NAN;
    assert!(run_episode(invalid, Policy::Reference, false).is_err());
    let (_, initial, _, _) = setup(&cases()[0]).unwrap();
    for reason in [
        ArrivalOutcome::Contact,
        ArrivalOutcome::Assisted,
        ArrivalOutcome::LostControl,
        ArrivalOutcome::OutsideRegion,
    ] {
        let mut o = initial.clone();
        match reason {
            ArrivalOutcome::Contact => o.contact = true,
            ArrivalOutcome::Assisted => o.landing.assist_strength = 0.01,
            ArrivalOutcome::LostControl => o.location = PilotLocation::OnFoot,
            ArrivalOutcome::OutsideRegion => {
                o.ship.position = o.planet_motion.position + Vec2::Y * 350.0
            }
            _ => unreachable!(),
        }
        assert_eq!(failure(&o, &initial), Some(reason));
        assert_eq!(
            score(
                reason,
                Measure {
                    distance: 0.0,
                    relative_speed: 0.0,
                    dwell: 30
                },
                30,
                1200
            ),
            ArrivalFitness::default()
        );
    }
}

#[test]
fn trace_retention_and_restarts_preserve_real_physics_and_control_tapes() {
    let case = cases()[1].clone();
    let mut trace = run_episode(case.clone(), Policy::Reference, true).unwrap();
    assert_eq!(
        trace,
        run_episode(case.clone(), Policy::Reference, true).unwrap()
    );
    assert!(
        trace
            .trace
            .iter()
            .all(|t| t.before.tick + 1 == t.after.as_ref().unwrap().tick && t.error.is_none())
    );
    trace.trace.clear();
    assert_eq!(trace, run_episode(case, Policy::Reference, false).unwrap());
}

#[test]
fn saved_neutral_model_matches_idle_and_wrong_organisms_are_rejected() {
    let model = DenseModel::from_json(&serde_json::to_vec(&neutral_genome()).unwrap()).unwrap();
    for case in [cases()[0].clone(), cases()[5].clone()] {
        let mut learned = run_episode(case.clone(), Policy::Model(model.clone()), true).unwrap();
        let idle = run_episode(case, Policy::Idle, true).unwrap();
        assert_ne!(learned.brain_id, idle.brain_id);
        assert_eq!(learned.final_observation, idle.final_observation);
        assert_eq!(learned.outcome, idle.outcome);
        assert_ne!(learned.outcome, ArrivalOutcome::Arrived);
        assert_eq!(learned.fitness.completed, 0);
        assert_eq!(learned.trace.len(), idle.trace.len());
        for (learned, idle) in learned.trace.iter_mut().zip(&idle.trace) {
            learned.raw_outputs = None;
            assert_eq!(learned, idle);
        }
    }
    let diagnostic = DenseModel::new(crate::line_diagnostic::known_genome(
        crate::line_diagnostic::KnownPolicy::Oracle,
    ))
    .unwrap();
    assert!(
        run_episode(cases()[0].clone(), Policy::Model(diagnostic), false)
            .unwrap_err()
            .0
            .contains("incompatible")
    );
}

#[test]
fn invalid_model_output_releases_controls_and_receives_zero_fitness() {
    let mut genome = neutral_genome();
    genome.layers[0].weights[1] = f32::MAX;
    genome.layers[0].biases[0] = f32::MAX;
    let model = DenseModel::new(genome).unwrap();
    let episode = run_episode(cases()[0].clone(), Policy::Model(model), true).unwrap();
    assert_eq!(episode.outcome, ArrivalOutcome::InvalidOutput);
    assert_eq!(episode.fitness, ArrivalFitness::default());
    assert_eq!(episode.elapsed_ticks, 0);
    assert_eq!(episode.trace[0].request.tick, episode.initial.tick);
    assert!(episode.trace[0].encoded.inputs[1] > 0.0);
    assert_eq!(episode.trace[0].movement, None);
    assert_eq!(episode.released.unwrap().applied, [0.0; 3]);
    assert!(
        episode.trace[0]
            .error
            .as_ref()
            .unwrap()
            .contains("non-finite")
    );
}

#[test]
fn shared_evolution_runner_resumes_material_evaluations_with_the_same_results() {
    use crate::evolution::{EvolutionConfig, EvolutionRun};
    let config = EvolutionConfig {
        population: 2,
        generations: 2,
        elites: 1,
        tournament_size: 2,
        initial_scale: 0.0,
        ..EvolutionConfig::default()
    };
    let mut control = EvolutionRun::new(&ArrivalTask, neutral_genome(), vec![], config).unwrap();
    let mut partial = control.clone();
    partial.advance(&ArrivalTask).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ship-run.json");
    partial.save(&path).unwrap();
    let mut resumed = EvolutionRun::load(&path, &ArrivalTask).unwrap();
    while control.advance(&ArrivalTask).unwrap() {}
    while resumed.advance(&ArrivalTask).unwrap() {}
    assert_eq!(control, resumed);
    assert_eq!(control.history().len(), 2);
}
