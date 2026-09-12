use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, EventLifecycle,
    SegmentRepresentation,
};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use std::time::Duration;

fn ready(aspect: f32, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Duck),
        ],
        Duration::ZERO,
    );
    // Keep the first wall-tag/hurdle course as a frozen geometry/controller
    // regression baseline; platform tests exercise the new default separately.
    let event_seed = state.duck_state().unwrap().navigation.unwrap().course_seed;
    state.active_event = Some(crate::events::ActiveEvent::Duck(Box::new(DuckEvent::new(
        Layout::new(aspect),
        event_seed,
    ))));
    state
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
    }
}

#[test]
fn duck_stays_in_the_course_and_tags_both_walls_before_the_exit_is_due() {
    let mut state = ready(800.0 / 480.0, 42);
    ticks(&mut state, OPENING_TICKS);
    let entrance_on_left = state.duck_state().unwrap().left_to_right;
    let mut touched = [false; 2];
    for tick in 0..20 * 60 {
        let duck = state.duck_state().expect("duck must still be playing tag");
        assert_eq!(duck.outcome, None, "premature exit at tick {tick}");
        assert_eq!(duck.exit_open_milli, 0, "exit opened at tick {tick}");
        let x = duck.position_milli.unwrap()[0] as f32 / 1000.0;
        // Ignore the initial warm-up near the entrance: both ends must be
        // visited after a complete outward crossing.
        if x * (if entrance_on_left { 1.0 } else { -1.0 }) > 320.0 {
            touched[1] = true;
        }
        if touched[1] && x * (if entrance_on_left { 1.0 } else { -1.0 }) < -320.0 {
            touched[0] = true;
        }
        ticks(&mut state, 1);
    }
    assert_eq!(touched, [true; 2]);
}

#[test]
fn seeded_courses_jump_all_obstacles_exit_and_release_every_body() {
    let mut directions = [false; 2];
    let mut cases = 0;
    let mut exit_range = [u64::MAX, 0];
    for aspect in [
        0.25,
        0.6,
        0.75,
        1024.0 / 768.0,
        800.0 / 480.0,
        1280.0 / 720.0,
        4.0,
    ] {
        for seed in 0..32 {
            let mut state = ready(aspect, seed);
            let mut phases = Vec::new();
            let mut airborne = false;
            let mut outcome = None;
            for tick in 0..DUCK_TICKS {
                let stats = state.duck_state().unwrap();
                let navigation = stats.navigation.unwrap();
                assert_eq!(
                    navigation.exit_visible,
                    tick >= OPENING_TICKS + EXIT_DELAY_TICKS
                );
                if !navigation.exit_visible {
                    assert_eq!(stats.exit_open_milli, 0);
                }
                directions[usize::from(stats.left_to_right)] = true;
                if phases.last().copied() != state.event_phase() {
                    phases.push(state.event_phase().unwrap());
                }
                assert!(state.body_count() <= 5 && state.collider_count() <= 5);
                assert!(stats.entrance_open_milli <= 1000 && stats.exit_open_milli <= 1000);
                if state.event_phase() == Some(EventPhase::Opening) {
                    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                    assert_eq!(stats.position_milli, None);
                    assert_eq!(stats.jumps, 0);
                    assert_eq!(stats.exit_open_milli, 0);
                    assert_eq!(
                        stats.entrance_open_milli,
                        (state.phase_tick() as f32 / OPENING_TICKS as f32 * 1000.0).round() as u32
                    );
                }
                if matches!(
                    state.event_phase(),
                    Some(EventPhase::Running | EventPhase::Exiting)
                ) {
                    assert_eq!(state.body_count(), 5);
                    airborne |= stats.jumps > 0 && !stats.grounded;
                }
                if state.event_phase() == Some(EventPhase::Exiting) {
                    assert_eq!(stats.entrance_open_milli, 0);
                    if state.phase_tick() >= 24 {
                        assert_eq!(stats.exit_open_milli, 1000);
                    }
                }
                if let Some(result) = stats.outcome {
                    assert_eq!(
                        result,
                        ClockDuckOutcome::Exited,
                        "aspect={aspect} seed={seed} tick={tick} {stats:?}"
                    );
                    assert_eq!(stats.cleared_obstacles, 3);
                    assert_eq!(navigation.calibrated_jumps, 2);
                    assert_eq!(navigation.speed_samples, 9);
                    assert!(
                        navigation.wall_tags.iter().all(|tags| *tags >= 1),
                        "{navigation:?}"
                    );
                    assert_eq!(
                        stats.jumps,
                        2 + 3 * (navigation.wall_tags.iter().sum::<u32>() + 1)
                    );
                    assert_eq!(state.body_count(), 0);
                    assert_eq!(stats.position_milli, None);
                    if outcome.is_none() {
                        exit_range[0] = exit_range[0].min(tick);
                        exit_range[1] = exit_range[1].max(tick);
                    }
                    outcome = Some(result);
                }
                ticks(&mut state, 1);
                if let Some(after) = state.duck_state() {
                    if after.jumps != stats.jumps {
                        assert!(stats.grounded, "a jump must start from a physical support");
                        assert_eq!(after.jumps, stats.jumps + 1);
                    }
                    if tick + 1 == OPENING_TICKS {
                        assert_eq!(state.event_phase(), Some(EventPhase::Running));
                        assert_eq!(after.entrance_open_milli, 1000);
                        assert!(after.position_milli.is_some());
                    }
                }
            }
            assert!(airborne);
            assert_eq!(outcome, Some(ClockDuckOutcome::Exited));
            assert_eq!(
                phases,
                [
                    EventPhase::Opening,
                    EventPhase::Running,
                    EventPhase::Exiting,
                    EventPhase::Resetting
                ]
            );
            assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
            assert_eq!(state.duck_state(), None);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            cases += 1;
        }
    }
    assert_eq!(directions, [true, true]);
    eprintln!(
        "duck matrix: {cases} seeded courses, all tagged both walls and exited; exit ticks {}..{}; no falls/timeouts",
        exit_range[0], exit_range[1]
    );
}

#[test]
fn warmup_measures_real_jump_and_running_motion_instead_of_assuming_tuning() {
    for aspect in [0.25, 800.0 / 480.0, 4.0] {
        for scale in [0.85, 1.0, 1.15] {
            let mut event = DuckEvent::new(Layout::new(aspect), 42);
            event.movement.run_speed *= scale;
            event.movement.jump_height *= scale;
            event.movement.gravity *= 1.1;
            let expected_speed = event.movement.run_speed;
            let expected_height = event.movement.jump_height;
            let expected_flight =
                2.0 * (2.0 * expected_height / event.movement.gravity).sqrt() / DT;
            for _ in 0..300 {
                event.step();
                let navigation = event.diagnostics().navigation.unwrap();
                if navigation.calibrated_jumps < 2 {
                    if let Some(position) = event.position() {
                        assert!((position.x - event.radius * 6.0).abs() < event.radius * 0.05);
                    }
                    assert_eq!(navigation.run_speed_milli, None);
                }
                if navigation.speed_samples == 9 {
                    let height = navigation.jump_height_milli.unwrap() as f32 / 1000.0;
                    let speed = navigation.run_speed_milli.unwrap() as f32 / 1000.0;
                    assert!(
                        (height / expected_height - 1.0).abs() < 0.06,
                        "{navigation:?}"
                    );
                    assert!(
                        (speed / expected_speed - 1.0).abs() < 0.02,
                        "{navigation:?}"
                    );
                    assert!(
                        (navigation.flight_ticks.unwrap() as f32 - expected_flight).abs() < 4.0
                    );
                    break;
                }
            }
            assert_eq!(event.controller.heights.count, 2);
            assert_eq!(event.controller.speeds.count, 9);
        }
    }
}

#[test]
fn sample_window_rejects_invalid_values_and_does_not_retain_speed_spikes() {
    let mut samples = controller::Samples::default();
    assert_eq!(samples.median(), None);
    for value in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
        samples.push(value);
    }
    assert_eq!(samples.count, 0);
    for value in [
        100.0, 101.0, 99.0, 100.0, 10000.0, 100.0, 101.0, 99.0, 100.0,
    ] {
        samples.push(value);
    }
    assert_eq!(samples.median(), Some(100.0));
    for _ in 0..20 {
        samples.push(80.0);
    }
    assert_eq!(samples.count, 9);
    assert_eq!(samples.median(), Some(80.0));
}

#[test]
fn a_disturbed_warmup_is_discarded_and_retried() {
    let mut event = DuckEvent::new(Layout::new(800.0 / 480.0), 42);
    while event.jumps == 0 {
        assert!(event.tick < 100);
        event.step();
    }
    // Disturb the real body during its first flight. Even if it returns to
    // the same floor, this is not an uncontaminated vertical calibration.
    event.world.as_mut().unwrap().apply_velocity_delta(
        DUCK_BODY,
        Vec2::new(event.radius * 20.0, 0.0),
        true,
    );
    while event.controller.heights.count < 2 {
        assert!(event.tick < 300, "warm-up never recovered");
        event.step();
    }
    assert_eq!(event.jumps, 3);
}

#[test]
fn speed_measurement_excludes_airborne_blocked_and_accelerating_motion() {
    for (grounded, blocked, accelerating) in [
        (false, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let mut controller = Controller::new();
        for _ in 0..2 {
            controller.heights.push(35.0);
            controller.flight_ticks.push(51.0);
        }
        let event = DuckEvent::new(Layout::new(800.0 / 480.0), 42);
        for tick in 0..15 {
            controller.decide(
                Observation {
                    position: Vec2::new(128.0, event.layout.floor_y + event.radius),
                    velocity: Vec2::new(
                        if accelerating {
                            10.0 * tick as f32
                        } else {
                            160.0
                        },
                        0.0,
                    ),
                    grounded,
                    blocked,
                    support: None,
                },
                &event.obstacles,
                event.width,
                event.radius,
                false,
            );
        }
        assert_eq!(controller.speeds.count, 0);
    }
}

#[test]
fn movement_accelerates_and_brakes_without_instantly_reversing_or_air_jumping() {
    use controller::{Command, Gait};
    let movement = Movement::new(800.0, 8.0);
    let mut observed = Observation {
        position: Vec2::ZERO,
        velocity: Vec2::new(movement.run_speed, -10.0),
        grounded: false,
        blocked: false,
        support: None,
    };
    let turn = Command {
        gait: Gait::Run,
        direction: -1.0,
        jump: true,
    };
    let delta = movement.velocity_delta(observed, &turn);
    assert!(delta.x < 0.0 && observed.velocity.x + delta.x > 0.0);
    assert_eq!(delta.y, 0.0);
    observed.grounded = true;
    let jump = movement.velocity_delta(observed, &turn).y + observed.velocity.y;
    assert!((jump * jump / (2.0 * movement.gravity) - 8.0 * 4.5).abs() < 0.001);
    for gait in [Gait::Walk, Gait::Still] {
        let command = Command {
            gait,
            direction: 1.0,
            jump: false,
        };
        for _ in 0..60 {
            observed.velocity += movement.velocity_delta(observed, &command);
        }
        assert_eq!(
            observed.velocity.x,
            if matches!(gait, Gait::Walk) {
                movement.walk_speed
            } else {
                0.0
            }
        );
    }
}

#[test]
fn a_fall_and_a_blocked_runner_recover_within_the_catalog_bound() {
    for fall in [false, true] {
        let mut state = ready(800.0 / 480.0, 5);
        ticks(&mut state, OPENING_TICKS);
        let Some(crate::events::ActiveEvent::Duck(event)) = &mut state.active_event else {
            panic!()
        };
        let world = event.world.as_mut().unwrap();
        if fall {
            world.set_pose(
                DUCK_BODY,
                Vec2::new(event.width * 0.5, event.layout.floor_y - event.radius * 6.0),
                0.0,
                true,
            );
        } else {
            // A real unjumpable obstruction, not a mock of the completion path.
            let id = PhysicsId::new(999);
            world.insert_body(
                BodyId::new(id, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: Vec2::new(event.width * 0.15, event.layout.floor_y + 100.0),
                    ..BodySpec::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(id, ColliderRole::PRIMARY, 0),
                    5.0,
                    100.0,
                )],
            );
        }
        ticks(&mut state, DUCK_TICKS - RESET_TICKS - OPENING_TICKS);
        assert_eq!(state.event_phase(), Some(EventPhase::Resetting));
        assert_eq!(
            state.duck_state().unwrap().outcome,
            Some(if fall {
                ClockDuckOutcome::Fell
            } else {
                ClockDuckOutcome::TimedOut
            })
        );
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        ticks(&mut state, RESET_TICKS);
        assert_eq!(state.duck_state(), None);
    }
}

#[test]
fn live_time_pause_resize_and_preview_replacement_preserve_the_clock() {
    for at in [
        0,
        100,
        430,
        580,
        OPENING_TICKS + EXIT_DELAY_TICKS - 1,
        OPENING_TICKS + EXIT_DELAY_TICKS,
        DUCK_TICKS - 10,
    ] {
        let mut state = ready(800.0 / 480.0, 42);
        ticks(&mut state, at);
        let stats = state.duck_state();
        let phase_tick = state.phase_tick();
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(23, 59, 1).unwrap(),
            )],
            Duration::ZERO,
        );
        assert_eq!(state.duck_state(), stats);
        assert_eq!(state.phase_tick(), phase_tick);
        assert_eq!(state.display().digits, [Some(2), Some(3), Some(5), Some(9)]);
        assert!(
            state
                .segments()
                .iter()
                .all(|s| s.representation == SegmentRepresentation::Anchored)
        );
        state.set_aspect_ratio(0.75);
        assert_eq!(state.duck_state(), None);
        assert_eq!(state.body_count(), 0);
    }
    let mut a = ready(800.0 / 480.0, 42);
    let mut b = ready(800.0 / 480.0, 42);
    for _ in 0..DUCK_TICKS {
        assert_eq!(a.duck_state(), b.duck_state());
        assert_eq!(
            ClockScenario::render_frame(&a),
            ClockScenario::render_frame(&b)
        );
        ticks(&mut a, 1);
        ticks(&mut b, 1);
    }
    for kind in ClockEventKind::ALL {
        ClockScenario::step(
            &mut a,
            &[ClockAction::preview_event(ClockEventKind::Duck)],
            Duration::ZERO,
        );
        ticks(&mut a, 100);
        ClockScenario::step(&mut a, &[ClockAction::preview_event(kind)], Duration::ZERO);
        assert_eq!(a.event_kind(), Some(kind));
        if kind != ClockEventKind::Duck {
            assert_eq!(a.duck_state(), None);
        }
    }
}
