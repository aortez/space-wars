use super::*;

#[test]
fn known_model_has_an_exact_hand_checkable_trace() {
    let model = DenseModel::new(known_genome(KnownPolicy::Oracle)).unwrap();
    let episode = run_episode(LineController::Model(&model), LineConfig::default(), true).unwrap();
    assert_eq!(episode.outcome, DiagnosticOutcome::Arrived);
    assert_eq!(episode.final_state.tick, 4);
    assert_eq!(
        episode.fitness,
        LineFitness {
            completed: 1,
            proximity_points: 8,
            spare_ticks: 12
        }
    );
    assert_eq!(episode.trace.len(), 4);
    for (i, (before, input, action, after, dwell)) in [
        (-2, 0.375, LineAction::Right, -1, 0),
        (-1, 0.25, LineAction::Right, 0, 0),
        (0, 0.125, LineAction::Right, 1, 1),
        (1, 0.0, LineAction::Stay, 1, 2),
    ]
    .into_iter()
    .enumerate()
    {
        let tick = &episode.trace[i];
        assert_eq!(tick.before.tick, i as u32);
        assert_eq!(tick.before.position, before);
        assert_eq!(tick.inputs, [input]);
        assert_eq!(
            tick.raw_outputs.as_ref().unwrap(),
            &vec![-input, 0.0625, input]
        );
        assert_eq!(tick.action, Some(action));
        assert_eq!((tick.after.position, tick.after.dwell), (after, dwell));
        assert_eq!(tick.fitness.proximity_points, 8 - (1_i32).abs_diff(after));
        assert!(tick.error.is_none());
    }
}

#[test]
fn all_known_models_match_independent_rules_on_all_81_cases() {
    let task = LineTask::default();
    assert_eq!(task.cases().len(), 81);
    for policy in [KnownPolicy::Oracle, KnownPolicy::Idle, KnownPolicy::Wrong] {
        let model = DenseModel::new(known_genome(policy)).unwrap();
        let mut completed = 0;
        for &case in task.cases() {
            let model_episode = run_episode(LineController::Model(&model), case, true).unwrap();
            let rule_episode = run_episode(LineController::Rule(policy), case, true).unwrap();
            assert_eq!(model_episode.final_state, rule_episode.final_state);
            assert_eq!(model_episode.fitness, rule_episode.fitness);
            assert_eq!(model_episode.outcome, rule_episode.outcome);
            let actions = |e: &LineEpisode| e.trace.iter().map(|t| t.action).collect::<Vec<_>>();
            assert_eq!(actions(&model_episode), actions(&rule_episode));
            completed += model_episode.fitness.completed;
        }
        assert_eq!(
            completed,
            if policy == KnownPolicy::Oracle { 81 } else { 9 }
        );
    }
    let evaluate = |p| {
        task.evaluate(&DenseModel::new(known_genome(p)).unwrap())
            .unwrap()
    };
    let oracle = evaluate(KnownPolicy::Oracle);
    assert_eq!(
        oracle,
        LineFitness {
            completed: 81,
            proximity_points: 648,
            spare_ticks: 966
        }
    );
    assert!(oracle > evaluate(KnownPolicy::Idle));
    assert!(evaluate(KnownPolicy::Idle) > evaluate(KnownPolicy::Wrong));
}

#[test]
fn traces_and_previous_episodes_do_not_change_evaluation() {
    let model = DenseModel::new(known_genome(KnownPolicy::Oracle)).unwrap();
    let mut first =
        run_episode(LineController::Model(&model), LineConfig::default(), true).unwrap();
    run_episode(
        LineController::Model(&model),
        LineConfig {
            start: 4,
            target: -4,
            ..LineConfig::default()
        },
        true,
    )
    .unwrap();
    assert_eq!(
        first,
        run_episode(LineController::Model(&model), LineConfig::default(), true).unwrap()
    );
    first.trace.clear();
    assert_eq!(
        first,
        run_episode(LineController::Model(&model), LineConfig::default(), false).unwrap()
    );
}

#[test]
fn neutral_and_tied_outputs_have_explicit_actions() {
    assert_eq!(decode(&[0.0; 3]).unwrap(), LineAction::Stay);
    assert_eq!(decode(&[1.0, 0.0, 1.0]).unwrap(), LineAction::Left);
    assert_eq!(decode(&[0.0, 1.0, 1.0]).unwrap(), LineAction::Stay);
    assert!(decode(&[0.0; 2]).is_err());
    assert!(decode(&[0.0, f32::NAN, 1.0]).is_err());
}

#[test]
fn incompatible_artifacts_are_host_errors_and_invalid_outputs_are_episode_failures() {
    let mut wrong = known_genome(KnownPolicy::Oracle);
    wrong.contract.organism = "material_ship".into();
    let wrong = DenseModel::new(wrong).unwrap();
    assert!(
        run_episode(LineController::Model(&wrong), LineConfig::default(), true)
            .unwrap_err()
            .0
            .contains("incompatible")
    );

    let mut overflow = known_genome(KnownPolicy::Oracle);
    overflow.layers[0].weights[0] = f32::MAX;
    overflow.layers[0].biases[0] = f32::MAX;
    let overflow = DenseModel::new(overflow).unwrap();
    let result = run_episode(
        LineController::Model(&overflow),
        LineConfig::default(),
        true,
    )
    .unwrap();
    assert_eq!(result.outcome, DiagnosticOutcome::InvalidOutput);
    assert_eq!(result.fitness, LineFitness::default());
    assert_eq!(result.trace.len(), 1);
    assert_eq!(result.trace[0].before, result.trace[0].after);
    assert_eq!(result.trace[0].action, None);
    assert!(
        result.trace[0]
            .error
            .as_ref()
            .unwrap()
            .contains("non-finite")
    );
}
