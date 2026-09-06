use super::*;

fn step(state: &mut SpacelingLabState, walk: f32, jump: bool) {
    SpacelingLabScenario::step(
        state,
        &[SpacelingLabAction::control(walk, jump)],
        Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ)),
    );
}

#[test]
fn typed_controls_reject_malformed_and_nonfinite_input() {
    assert_eq!(
        SpacelingLabAction::decode(&SpacelingLabAction::control(10.0, true)),
        Some(SpacelingLabAction {
            walk: 1.0,
            jump_held: true
        })
    );
    for action in [
        Action::scenario(99, vec![0; 5]),
        Action::scenario(CONTROL_V1, vec![0; 4]),
        Action::scenario(CONTROL_V1, vec![0, 0, 0, 0, 2]),
        SpacelingLabAction::control(f32::NAN, false),
        SpacelingLabAction::control(f32::INFINITY, false),
    ] {
        assert!(SpacelingLabAction::decode(&action).is_none());
    }
}

#[test]
fn last_valid_action_wins_and_zero_duration_does_not_advance() {
    let mut state = SpacelingLabScenario::init(SpacelingLabConfig::default(), 0);
    let before = SpacelingLabScenario::observe(&state).payload;
    SpacelingLabScenario::step(
        &mut state,
        &[SpacelingLabAction::control(1.0, true)],
        Duration::ZERO,
    );
    assert_eq!(before, SpacelingLabScenario::observe(&state).payload);
    SpacelingLabScenario::step(
        &mut state,
        &[
            SpacelingLabAction::control(-1.0, true),
            SpacelingLabAction::control(1.0, false),
            SpacelingLabAction::control(f32::NAN, true),
        ],
        Duration::from_secs_f64(1.0 / 60.0),
    );
    assert_eq!(
        state.control,
        SpacelingControl {
            walk: 1.0,
            jump_held: false
        }
    );
}

#[test]
fn repeated_actions_reproduce_observations_and_render_frames() {
    let mut first = SpacelingLabScenario::init(SpacelingLabConfig::default(), 42);
    let initial = SpacelingLabScenario::render_frame(&first);
    let mut second = SpacelingLabScenario::init(SpacelingLabConfig::default(), 42);
    for tick in 0..1800 {
        let walk = if tick < 900 { 1.0 } else { -1.0 };
        let jump = tick % 240 < 8;
        step(&mut first, walk, jump);
        step(&mut second, walk, jump);
        assert_eq!(
            SpacelingLabScenario::observe(&first).payload,
            SpacelingLabScenario::observe(&second).payload
        );
    }
    assert_eq!(
        SpacelingLabScenario::render_frame(&first),
        SpacelingLabScenario::render_frame(&second)
    );
    assert_eq!(
        initial,
        SpacelingLabScenario::render_frame(&SpacelingLabScenario::init(
            SpacelingLabConfig::default(),
            42
        ))
    );
    assert!(first.spaceling_snapshot().jumps > 2);
    assert!(first.landings > 2);
}

#[test]
fn spaceling_can_walk_around_both_bumps_and_whole_planet() {
    for direction in [-1.0, 1.0] {
        let mut state = SpacelingLabScenario::init(SpacelingLabConfig::default(), 0);
        for _ in 0..120 {
            step(&mut state, 0.0, false);
        }
        let mut total_angle = 0.0;
        let mut previous_angle = std::f32::consts::FRAC_PI_2;
        let mut supported = 0;
        for _ in 0..2400 {
            step(&mut state, direction, false);
            let snapshot = state.spaceling_snapshot();
            let position = snapshot.motion.position;
            let angle = position.y.atan2(position.x);
            total_angle += (angle - previous_angle + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            previous_angle = angle;
            assert!(
                position.length() > PLANET_RADIUS + 0.65,
                "inside terrain: {snapshot:?}"
            );
            assert!(
                position.length() < PLANET_RADIUS + 3.0,
                "escaped terrain: {snapshot:?}"
            );
            supported += usize::from(snapshot.grounded());
        }
        assert!(
            total_angle * -direction > std::f32::consts::TAU,
            "direction {direction} moved only {total_angle} radians"
        );
        assert!(supported > 1800, "grounded only {supported}/2400");
    }
}

#[test]
fn holding_jump_lands_once_and_diagnostics_match_physical_support() {
    let mut state = SpacelingLabScenario::init(SpacelingLabConfig::default(), 0);
    for _ in 0..120 {
        step(&mut state, 0.0, false);
    }
    let initial_landings = state.landings;
    let mut peak = 0.0_f32;
    for _ in 0..240 {
        step(&mut state, 0.0, true);
        peak = peak.max(state.spaceling_snapshot().motion.position.length());
    }
    let snapshot = state.spaceling_snapshot();
    assert!(peak > PLANET_RADIUS + 2.4);
    assert!(snapshot.grounded());
    assert_eq!(snapshot.jumps, 1);
    assert_eq!(state.landings, initial_landings + 1);
    assert!(state.last_airtime_ticks > 30);
    assert_eq!(snapshot.support.unwrap().collider.entity, PLANET_ID);
}

#[test]
fn zero_gravity_and_invalid_configuration_remain_finite() {
    let mut state = SpacelingLabScenario::init(
        SpacelingLabConfig {
            gravity_acceleration: 0.0,
            planet_angular_velocity: 0.0,
        },
        0,
    );
    for _ in 0..120 {
        step(&mut state, 0.0, true);
    }
    assert_eq!(state.gravity, Vec2::ZERO);
    assert_eq!(state.spaceling_snapshot().jumps, 0);
    assert!(!state.spaceling_snapshot().grounded());
    let invalid = SpacelingLabScenario::init(
        SpacelingLabConfig {
            gravity_acceleration: f32::NAN,
            planet_angular_velocity: f32::INFINITY,
        },
        0,
    );
    assert_eq!(invalid.config, SpacelingLabConfig::default());
}
