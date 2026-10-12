use super::*;
use crate::driving_lab::{chassis_contacts, disable_combat, snapshot};

fn hills(shape: GrainShape, tuning: WheelTuning, quiet: bool) -> ScorchedState {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            shape,
            suspension: true,
            wheel_tuning: tuning,
            ..Default::default()
        },
        42,
    );
    if quiet {
        disable_combat(&mut state);
    }
    for _ in 0..120 {
        step(&mut state, Controls::default());
    }
    state
}

fn drive(state: &mut ScorchedState, axis: f32) {
    step(
        state,
        Controls {
            drive: axis,
            ..Default::default()
        },
    );
}

#[test]
fn ordinary_hill_stall_is_reproduced_and_manual_hill_starts_clear_it() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut old = hills(
            shape,
            WheelTuning {
                motor_torque: 120.0,
                ..WheelTuning::REFERENCE
            },
            true,
        );
        for _ in 0..1200 {
            drive(&mut old, 1.0);
        }
        let stalled = snapshot(&old, 0, 1.0);
        assert!((-10.1..-9.8).contains(&stalled.position[0]), "{stalled:?}");
        assert!(stalled.supported && stalled.velocity.iter().all(|v| v.abs() < 0.01));
        let wheels = stalled.wheel_speeds.unwrap();
        assert!(wheels[0].abs() < 0.01 && wheels[1].abs() > 2.0);
        assert!(
            chassis_contacts(&old, 0).is_empty(),
            "the hull is not snagged"
        );
        old.audit().unwrap();

        // Use real manual controls on ordinary hills, including a release and
        // restart on three inclines. No goal policy or pose correction helps.
        let mut state = hills(shape, WheelTuning::default(), true);
        let mut replay = state.clone();
        for target in [-10.0, -6.0, -2.0, 8.0] {
            for _ in 0..1200 {
                if snapshot(&state, 0, 1.0).position[0] >= target {
                    break;
                }
                drive(&mut state, 1.0);
                drive(&mut replay, 1.0);
            }
            let before = snapshot(&state, 0, 1.0);
            assert!(
                before.position[0] >= target && before.supported,
                "{shape:?}: {before:?}"
            );
            for _ in 0..120 {
                drive(&mut state, 0.0);
                drive(&mut replay, 0.0);
            }
            let stopped = snapshot(&state, 0, 1.0);
            // The summit is already downhill; allow the ordinary braking
            // distance from full cruising speed there, versus uphill rollback.
            let stopping_distance = if target == 8.0 { 1.5 } else { 0.5 };
            assert!(
                (stopped.position[0] - before.position[0]).abs() < stopping_distance,
                "{stopped:?}"
            );
            assert!(
                Vec2::new(stopped.velocity[0], stopped.velocity[1]).length() < 0.15,
                "{stopped:?}"
            );
            assert_eq!(state.observation_hash(), replay.observation_hash());
            state.audit().unwrap();
        }
        assert!(state.tick < 2100, "the climb must make sustained progress");
    }
}

#[test]
fn manual_hill_climb_with_live_enemy_fire_finishes_in_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = hills(shape, WheelTuning::default(), false);
        for _ in 0..1800 {
            if snapshot(&state, 0, 1.0).position[0] >= 8.0 {
                break;
            }
            drive(&mut state, 1.0);
        }
        let last = snapshot(&state, 0, 1.0);
        assert!(
            last.position[0] >= 8.0 && last.supported && last.health > 0.0,
            "{shape:?}: {last:?}"
        );
        assert!(last.shots > 0 && last.impacts > 0);
        state.audit().unwrap();
    }
}
