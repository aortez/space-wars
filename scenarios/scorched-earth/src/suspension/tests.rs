use super::*;
use crate::driving::tests::{fixture, step};
mod hills;

fn wheeled_fixture(
    shape: GrainShape,
    strength: BankStrength,
    floor: impl Fn(f32) -> f32,
) -> ScorchedState {
    let mut state = fixture(shape, strength, floor);
    state.config.suspension = true;
    for i in 0..2 {
        let body = state.tanks[i].body;
        let position = state.physics.motion(body).unwrap().position + Vec2::Y * 0.6;
        assert!(state.physics.remove_entity(body.entity));
        let gear = insert(
            &mut state.physics,
            body.entity,
            position,
            state.config.wheel_tuning,
        );
        state.tanks[i].body = gear.bodies()[0];
        state.running_gear[i] = Some(gear);
    }
    for _ in 0..120 {
        step(&mut state, Controls::default());
    }
    state
}

#[test]
fn wheel_comparisons_preserve_mass_cruising_speed_brakes_and_replay() {
    let mut baseline_mass = None;
    for tuning in [
        WheelTuning::default(),
        WheelTuning {
            motor_torque: 135.0,
            ..Default::default()
        },
        WheelTuning {
            radius: 0.8,
            ..Default::default()
        },
    ] {
        let mut state = ScorchedState::new(
            ScorchedConfig {
                course: Some(DrivingCourse::Flat),
                suspension: true,
                wheel_tuning: tuning,
                ..Default::default()
            },
            42,
        );
        let gear = state.running_gear[0].as_ref().unwrap();
        let masses = gear
            .bodies()
            .map(|body| state.physics.body_mass(body).unwrap());
        let baseline = *baseline_mass.get_or_insert(masses);
        assert!(
            masses
                .into_iter()
                .zip(baseline)
                .all(|(m, b)| (m - b).abs() < 0.0001)
        );
        for _ in 0..120 {
            step(&mut state, Controls::default());
        }
        let mut replay = state.clone();
        for tick in 0..300 {
            let controls = Controls {
                drive: if tick < 240 { 1.0 } else { 0.0 },
                ..Default::default()
            };
            step(&mut state, controls);
            step(&mut replay, controls);
            assert_eq!(state.observation_hash(), replay.observation_hash());
            if tick == 239 {
                let speed = state
                    .physics
                    .motion(state.tanks[0].body)
                    .unwrap()
                    .linear_velocity
                    .x;
                assert!((speed - 2.5).abs() < 0.2, "{tuning:?}: {speed}");
            }
        }
        let speed = state
            .physics
            .motion(state.tanks[0].body)
            .unwrap()
            .linear_velocity
            .length();
        assert!(speed < 0.1, "{tuning:?}: {speed}");
        state.audit().unwrap();
    }
}

#[test]
fn sprung_wheels_drive_brake_reverse_and_replay_in_both_dirt_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for strength in [BankStrength::Off, BankStrength::Weak, BankStrength::Strong] {
            let mut state = wheeled_fixture(shape, strength, |_| -5.0);
            let body = state.tanks[0].body;
            let start = state.physics.motion(body).unwrap();
            let controls = Controls {
                drive: 1.0,
                ..Default::default()
            };
            for _ in 0..180 {
                step(&mut state, controls);
            }
            let moved = state.physics.motion(body).unwrap();
            assert!(
                moved.position.x > start.position.x + 4.0,
                "{shape:?} {strength:?}: {start:?} -> {moved:?}"
            );
            let mut replay = state.clone();
            for _ in 0..60 {
                step(&mut state, Controls::default());
                step(&mut replay, Controls::default());
                assert_eq!(state.observation_hash(), replay.observation_hash());
            }
            let stopped = state.physics.motion(body).unwrap();
            assert!(stopped.linear_velocity.length() < 0.1, "{stopped:?}");
            assert!(stopped.position.x - moved.position.x < 1.5);
            for _ in 0..150 {
                step(
                    &mut state,
                    Controls {
                        drive: -1.0,
                        ..Default::default()
                    },
                );
            }
            assert!(state.physics.motion(body).unwrap().position.x < stopped.position.x - 3.0);
            state.explode(state.physics.motion(body).unwrap().position + Vec2::new(2.0, -2.0));
            let mut replay = state.clone();
            for _ in 0..120 {
                step(&mut state, controls);
                step(&mut replay, controls);
            }
            assert_eq!(state.observation_hash(), replay.observation_hash());
            state.audit().unwrap();
        }
    }
}

#[test]
fn suspension_moves_over_a_step_and_wheels_cannot_drive_in_air() {
    let mut state = wheeled_fixture(GrainShape::Round, BankStrength::Off, |x| {
        if (-16.0..-12.0).contains(&x) {
            -4.5
        } else {
            -5.0
        }
    });
    let gear = state.running_gear[0].clone().unwrap();
    let before = gear.snapshot(&state.physics).unwrap();
    let mut min_length = f32::INFINITY;
    let mut max_length = 0.0_f32;
    for _ in 0..210 {
        step(
            &mut state,
            Controls {
                drive: 1.0,
                ..Default::default()
            },
        );
        let snapshot = gear.snapshot(&state.physics).unwrap();
        for length in snapshot.suspension_lengths {
            min_length = min_length.min(length);
            max_length = max_length.max(length);
        }
    }
    let after = gear.snapshot(&state.physics).unwrap();
    assert!(after.chassis.position.x > -15.0, "{after:?}");
    assert!(
        max_length - min_length > 0.04,
        "travel {min_length}..{max_length}, {before:?}"
    );
    let spec = spec(state.config.wheel_tuning);
    assert!(min_length > spec.suspension_rest_length - spec.suspension_travel - 0.04);
    assert!(max_length < spec.suspension_rest_length + spec.suspension_travel + 0.04);
    for body in gear.bodies() {
        let motion = state.physics.motion(body).unwrap();
        state
            .physics
            .set_pose(body, motion.position + Vec2::Y * 30.0, motion.angle, true);
        state.physics.set_velocity(body, Vec2::ZERO, 0.0, true);
    }
    // Refresh the old contact manifolds at the new airborne pose.
    step(&mut state, Controls::default());
    let x = state
        .physics
        .motion(state.tanks[0].body)
        .unwrap()
        .position
        .x;
    for _ in 0..30 {
        step(
            &mut state,
            Controls {
                drive: 1.0,
                ..Default::default()
            },
        );
    }
    assert!(
        (state
            .physics
            .motion(state.tanks[0].body)
            .unwrap()
            .position
            .x
            - x)
            .abs()
            < 0.02
    );
    state.audit().unwrap();
}

#[test]
fn suspended_cpu_drives_and_fires_and_comparison_rebuilds_cleanly() {
    let mut state = wheeled_fixture(GrainShape::Hexagon, BankStrength::Weak, |_| -5.0);
    state.config.cpu_driving = true;
    state.config.demo = true;
    for tank in &mut state.tanks {
        tank.next_shot = state.tick + 240;
    }
    let before = state
        .tanks
        .map(|tank| state.physics.motion(tank.body).unwrap().position.x);
    let mut firing_speeds = Vec::new();
    for _ in 0..300 {
        let shots = state.tanks.map(|t| t.shots);
        step(&mut state, Controls::default());
        for (i, previous) in shots.into_iter().enumerate() {
            if state.tanks[i].shots > previous {
                firing_speeds.push(
                    state
                        .physics
                        .motion(state.tanks[i].body)
                        .unwrap()
                        .linear_velocity
                        .length(),
                );
            }
        }
    }
    let after = state
        .tanks
        .map(|tank| state.physics.motion(tank.body).unwrap().position.x);
    assert!(
        after[0] > before[0] + 2.0 && after[1] < before[1] - 2.0,
        "{before:?} -> {after:?}"
    );
    assert_eq!(firing_speeds.len(), 2, "{firing_speeds:?}");
    assert!(firing_speeds.iter().all(|s| *s < 0.7));
    state.command(Command::Suspension);
    assert!(state.running_gear.iter().all(Option::is_none));
    assert_eq!(state.body_count(), 3);
    state.command(Command::Suspension);
    assert!(state.running_gear.iter().all(Option::is_some));
    assert_eq!(state.body_count(), 7);
    state.command(Command::Roof);
    assert!(state.running_gear.iter().all(Option::is_none));
    assert!(
        state.config.suspension,
        "lab keeps the chosen comparison for manual play"
    );
    state.command(Command::Demo);
    assert!(state.running_gear.iter().all(Option::is_some));
    state.audit().unwrap();
}

#[test]
fn wheelbase_support_admits_cell_corners_and_both_drivers_clear_rough_courses() {
    use crate::driving_lab::{snapshot, step_trial, wheel_contacts};
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for course in [DrivingCourse::Slope, DrivingCourse::Rubble] {
            for axis in [None, Some(1.0)] {
                let mut state = ScorchedState::new(
                    ScorchedConfig {
                        shape,
                        course: Some(course),
                        suspension: true,
                        demo: true,
                        ..Default::default()
                    },
                    42,
                );
                for _ in 0..120 {
                    step_trial(&mut state, 0, Some(0.0));
                }
                let mut crossed_corner = false;
                let mut finished = false;
                for _ in 0..900 {
                    step_trial(&mut state, 0, axis);
                    let drive = snapshot(&state, 0, 1.0);
                    assert!(drive.supported, "{shape:?} {course:?} {axis:?}: {drive:?}");
                    assert!(drive.pitch_degrees.abs() < 30.0, "{drive:?}");
                    crossed_corner |= !state.has_reference_track_support(0)
                        && wheel_contacts(&state, 0)
                            .unwrap()
                            .iter()
                            .flatten()
                            .any(|c| c.accepted_support && !c.reference_accepted_support);
                    if drive.position[0] >= course.finish_x() {
                        finished = true;
                        break;
                    }
                }
                assert!(finished, "{shape:?} {course:?} {axis:?}");
                assert!(crossed_corner, "must exercise the repaired support gate");
                state.audit().unwrap();
            }
        }
    }
}

#[test]
fn frozen_support_failure_keeps_native_state_and_goal_history() {
    use crate::driving_lab::{SupportCheck, TravelGoal, physics_snapshot, step_support_trial};
    let mut state = ScorchedState::new(
        ScorchedConfig {
            course: Some(DrivingCourse::Slope),
            suspension: true,
            demo: true,
            wheel_tuning: WheelTuning::REFERENCE,
            ..Default::default()
        },
        42,
    );
    for _ in 0..120 {
        step_support_trial(&mut state, 0, Some(0.0), SupportCheck::Reference);
    }
    let mut goal = TravelGoal::new(DrivingCourse::Slope.finish_x(), 2.5);
    for _ in 0..900 {
        if !state.has_reference_track_support(0) && state.has_track_support(0) {
            break;
        }
        let decision = goal.decide_reference(&state, 0, SupportCheck::Reference);
        step_support_trial(&mut state, 0, Some(decision.axis), SupportCheck::Reference);
    }
    assert_eq!(
        state.tick, 603,
        "retain the original goal-controller failure"
    );
    let mut candidate = state.clone();
    let mut candidate_goal = goal.clone();
    let native = physics_snapshot(&state);
    assert_eq!(physics_snapshot(&candidate), native);
    assert_eq!(
        serde_json::to_value(&goal).unwrap(),
        serde_json::to_value(&candidate_goal).unwrap()
    );
    let old = goal.decide_reference(&state, 0, SupportCheck::Reference);
    let new = candidate_goal.decide_reference(&candidate, 0, SupportCheck::Current);
    assert_eq!(old.mode, "no support");
    assert!(new.axis > 0.0);
    assert_eq!(old.retries, new.retries);
    step_support_trial(&mut state, 0, Some(old.axis), SupportCheck::Reference);
    step_support_trial(&mut candidate, 0, Some(new.axis), SupportCheck::Current);
    assert_eq!(state.driving[0].diagnostics.track_speed, 0.0);
    assert!(candidate.driving[0].diagnostics.track_speed > 0.0);
    // Sampling and controller inspection must not alter the captured world.
    assert_ne!(physics_snapshot(&candidate), native);
    state.audit().unwrap();
    candidate.audit().unwrap();
}

#[test]
fn wheeled_tank_brakes_on_a_cell_slope_without_rolling_away() {
    use crate::driving_lab::{snapshot, step_trial};
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = ScorchedState::new(
            ScorchedConfig {
                shape,
                course: Some(DrivingCourse::Slope),
                suspension: true,
                ..Default::default()
            },
            42,
        );
        for _ in 0..120 {
            step_trial(&mut state, 0, Some(0.0));
        }
        for _ in 0..600 {
            step_trial(&mut state, 0, Some(1.0));
            if snapshot(&state, 0, 1.0).position[0] >= -16.0 {
                break;
            }
        }
        let moving = snapshot(&state, 0, 1.0);
        assert!(
            moving.position[0] >= -16.0 && moving.pitch_degrees > 15.0,
            "{moving:?}"
        );
        for _ in 0..180 {
            step_trial(&mut state, 0, Some(0.0));
        }
        let stopped = snapshot(&state, 0, 1.0);
        assert!(
            stopped.supported && stopped.velocity[0].abs() < 0.05,
            "{stopped:?}"
        );
        assert!(
            (stopped.position[0] - moving.position[0]).abs() < 1.5,
            "{moving:?} -> {stopped:?}"
        );
        let mut replay = state.clone();
        for _ in 0..180 {
            step_trial(&mut state, 0, Some(0.0));
            step_trial(&mut replay, 0, Some(0.0));
            assert_eq!(state.observation_hash(), replay.observation_hash());
        }
        assert!((snapshot(&state, 0, 1.0).position[0] - stopped.position[0]).abs() < 0.05);
        state.audit().unwrap();
    }
}

#[test]
fn wheels_do_not_drive_on_a_steep_grade_or_upside_down() {
    for slope in [-50.0_f32, 50.0] {
        let mut state = wheeled_fixture(GrainShape::Round, BankStrength::Off, |x| {
            -5.0 + (x + 20.0).clamp(-6.0, 6.0) * slope.to_radians().tan()
        });
        // Place the complete assembly on the incline. The long fixture warmup
        // otherwise lets an unsupported tank slide all the way to the flat toe.
        let entity = state.tanks[0].body.entity;
        state.physics.remove_entity(entity);
        let normal = Vec2::Y.rotate_radians(slope.to_radians());
        let gear = RoverAssembly::insert_at(
            &mut state.physics,
            entity,
            RoverSpawnPose {
                wheel_center: Vec2::new(-20.0, -5.0) + normal * 0.8,
                up_angle: std::f32::consts::FRAC_PI_2 + slope.to_radians(),
            },
            spec(state.config.wheel_tuning),
        )
        .unwrap();
        state.tanks[0].body = gear.bodies()[0];
        state.running_gear[0] = Some(gear);
        for _ in 0..30 {
            step(&mut state, Controls::default());
        }
        let angle = state
            .physics
            .motion(state.tanks[0].body)
            .unwrap()
            .angle
            .to_degrees();
        assert!(
            angle.abs() > 40.0,
            "steep fixture must rest on the incline: {angle}"
        );
        assert!(!state.has_track_support(0));
        assert!(
            crate::driving_lab::wheel_contacts(&state, 0)
                .unwrap()
                .iter()
                .flatten()
                .any(|c| c.separation <= 0.03),
            "steep fixture must have real wheel contacts"
        );
        let mut neutral = state.clone();
        for _ in 0..30 {
            step(
                &mut state,
                Controls {
                    drive: 1.0,
                    ..Default::default()
                },
            );
            step(&mut neutral, Controls::default());
            assert_eq!(
                state.physics.snapshot_bytes().unwrap(),
                neutral.physics.snapshot_bytes().unwrap()
            );
        }
    }
    let mut inverted = wheeled_fixture(GrainShape::Round, BankStrength::Off, |_| -5.0);
    let gear = inverted.running_gear[0].clone().unwrap();
    let center = inverted
        .physics
        .motion(inverted.tanks[0].body)
        .unwrap()
        .position;
    for body in gear.bodies() {
        let motion = inverted.physics.motion(body).unwrap();
        inverted.physics.set_pose(
            body,
            center - (motion.position - center) + Vec2::Y,
            motion.angle + std::f32::consts::PI,
            true,
        );
        inverted.physics.set_velocity(body, Vec2::ZERO, 0.0, true);
    }
    for _ in 0..120 {
        step(&mut inverted, Controls::default());
    }
    assert!(!inverted.has_track_support(0));
    let mut neutral = inverted.clone();
    for _ in 0..60 {
        step(
            &mut inverted,
            Controls {
                drive: 1.0,
                ..Default::default()
            },
        );
        step(&mut neutral, Controls::default());
        assert_eq!(
            inverted.physics.snapshot_bytes().unwrap(),
            neutral.physics.snapshot_bytes().unwrap()
        );
    }
}
