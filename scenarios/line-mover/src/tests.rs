use super::*;

fn step(state: &mut LineState, action: LineAction) -> StepResult {
    LineMoverScenario::step(state, &[action.encode()], DT)
}

#[test]
fn arrival_needs_consecutive_post_step_dwell_and_wins_deadline_tie() {
    let mut state = LineMoverScenario::init(
        LineConfig {
            max_ticks: 4,
            ..LineConfig::default()
        },
        7,
    );
    for (action, position, dwell, terminated) in [
        (LineAction::Right, -1, 0, false),
        (LineAction::Right, 0, 0, false),
        (LineAction::Right, 1, 1, false),
        (LineAction::Stay, 1, 2, true),
    ] {
        assert_eq!(step(&mut state, action).terminated, terminated);
        assert_eq!(
            (state.snapshot().position, state.snapshot().dwell),
            (position, dwell)
        );
    }
    assert_eq!(state.snapshot().outcome, Some(LineOutcome::Arrived));
    let terminal = state.snapshot();
    assert!(step(&mut state, LineAction::Left).terminated);
    assert_eq!(state.snapshot(), terminal);
}

#[test]
fn initially_at_target_requires_dwell_and_leaving_resets_it() {
    let mut state = LineMoverScenario::init(
        LineConfig {
            start: 1,
            ..LineConfig::default()
        },
        0,
    );
    assert!(!step(&mut state, LineAction::Stay).terminated);
    assert_eq!(state.snapshot().dwell, 1);
    step(&mut state, LineAction::Left);
    assert_eq!(state.snapshot().dwell, 0);
    assert!(!step(&mut state, LineAction::Right).terminated);
    assert!(step(&mut state, LineAction::Stay).terminated);
    assert_eq!(state.snapshot().tick, 4);
}

#[test]
fn boundaries_clamp_and_idle_times_out() {
    for (start, target, action) in [(-4, 4, LineAction::Left), (4, -4, LineAction::Right)] {
        let mut state = LineMoverScenario::init(
            LineConfig {
                start,
                target,
                max_ticks: 2,
                ..LineConfig::default()
            },
            0,
        );
        assert!(!step(&mut state, action).terminated);
        assert_eq!(state.snapshot().position, start);
        assert!(LineMoverScenario::step(&mut state, &[], DT).terminated);
        assert_eq!(state.snapshot().outcome, Some(LineOutcome::Timeout));
    }
}

#[test]
fn observation_and_rendering_are_read_only_and_distance_has_fixed_units() {
    let mut state = LineMoverScenario::init(LineConfig::default(), 0);
    let before = state.snapshot();
    for _ in 0..3 {
        assert_eq!(
            serde_json::from_slice::<[f32; 1]>(&LineMoverScenario::observe(&state).payload)
                .unwrap(),
            [0.375]
        );
        let _ = LineMoverScenario::render_frame(&state);
    }
    assert!(
        !LineMoverScenario::step(&mut state, &[LineAction::Right.encode()], Duration::ZERO)
            .terminated
    );
    assert_eq!(state.snapshot(), before);
    step(&mut state, LineAction::Right);
    assert_eq!(state.inputs(), [0.25]);
    let reset = LineMoverScenario::init(LineConfig::default(), u64::MAX);
    assert_eq!(reset.snapshot(), before);
}

#[test]
fn malformed_actions_end_the_episode_without_moving() {
    for actions in [
        vec![Action::scenario(ACTION_KIND, vec![3])],
        vec![Action::scenario(ACTION_KIND, vec![1, 2])],
        vec![Action::scenario(ACTION_KIND + 1, vec![1])],
        vec![LineAction::Left.encode(), LineAction::Right.encode()],
    ] {
        let mut state = LineMoverScenario::init(LineConfig::default(), 0);
        assert!(LineMoverScenario::step(&mut state, &actions, DT).terminated);
        assert_eq!(
            state.snapshot(),
            LineSnapshot {
                tick: 1,
                position: -2,
                target: 1,
                dwell: 0,
                outcome: Some(LineOutcome::InvalidAction)
            }
        );
    }
}

#[test]
fn invalid_configs_are_rejected_before_world_creation() {
    for config in [
        LineConfig {
            start: i32::MIN,
            ..LineConfig::default()
        },
        LineConfig {
            target: 5,
            ..LineConfig::default()
        },
        LineConfig {
            dwell_ticks: 0,
            ..LineConfig::default()
        },
        LineConfig {
            max_ticks: 1,
            ..LineConfig::default()
        },
        LineConfig {
            max_ticks: 257,
            ..LineConfig::default()
        },
    ] {
        assert!(LineMoverScenario::try_init(config).is_err());
    }
}
