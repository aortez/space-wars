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
    state
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
    }
}

#[test]
fn seeded_courses_jump_all_obstacles_exit_and_release_every_body() {
    let mut directions = [false; 2];
    for aspect in [0.25, 0.75, 800.0 / 480.0, 4.0] {
        for seed in 0..32 {
            let mut state = ready(aspect, seed);
            let mut phases = Vec::new();
            let mut airborne = false;
            let mut outcome = None;
            for tick in 0..DUCK_TICKS {
                let stats = state.duck_state().unwrap();
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
                    assert_eq!(stats.jumps, 3);
                    assert_eq!(state.body_count(), 0);
                    assert_eq!(stats.position_milli, None);
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
        }
    }
    assert_eq!(directions, [true, true]);
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
    for at in [0, 100, 430, 580] {
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
