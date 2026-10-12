use super::*;
use crate::driving_lab::{physics_snapshot, snapshot, step_support_trial, step_trial};

fn settled(course: DrivingCourse, shape: GrainShape, suspension: bool) -> ScorchedState {
    settled_with_tuning(course, shape, suspension, WheelTuning::default())
}

fn settled_with_tuning(
    course: DrivingCourse,
    shape: GrainShape,
    suspension: bool,
    wheel_tuning: WheelTuning,
) -> ScorchedState {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            course: Some(course),
            shape,
            suspension,
            wheel_tuning,
            demo: true,
            ..Default::default()
        },
        42,
    );
    for _ in 0..120 {
        step_trial(&mut state, 0, Some(0.0));
    }
    state
}

#[test]
fn goal_driving_reaches_the_flag_slows_for_steps_and_stays_stopped_with_replay() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for suspension in [false, true] {
            for course in [
                DrivingCourse::Flat,
                DrivingCourse::Step,
                DrivingCourse::Slope,
                DrivingCourse::Rubble,
            ] {
                let mut state = settled(course, shape, suspension);
                let mut goal = TravelGoal::new(course.finish_x(), 2.5);
                let mut replay = state.clone();
                let mut replay_goal = goal.clone();
                let mut arrived = 0;
                let mut rough = 0;
                for tick in 0..1200 {
                    let decision = goal.decide(&state, 0);
                    let repeated = replay_goal.decide(&replay, 0);
                    assert_eq!(
                        serde_json::to_value(&decision).unwrap(),
                        serde_json::to_value(repeated).unwrap()
                    );
                    arrived += usize::from(decision.mode == "arrived");
                    if decision.mode == "rough ground" {
                        rough += 1;
                        assert!(decision.desired_speed <= 1.5);
                    }
                    step_trial(&mut state, 0, Some(decision.axis));
                    step_trial(&mut replay, 0, Some(decision.axis));
                    if tick % 60 == 0 {
                        assert_eq!(state.observation_hash(), replay.observation_hash());
                        state.audit().unwrap();
                    }
                }
                let last = snapshot(&state, 0, 1.0);
                assert!(
                    arrived >= 120,
                    "{shape:?} {suspension} {course:?}: {last:?}"
                );
                assert!((last.position[0] - course.finish_x()).abs() <= ARRIVAL_DISTANCE);
                assert!(last.velocity[0].abs() < 0.15 && last.pitch_degrees.abs() < 10.0);
                assert_eq!(rough > 0, course != DrivingCourse::Flat);
                assert_eq!(
                    serde_json::to_value(goal).unwrap(),
                    serde_json::to_value(replay_goal).unwrap()
                );
            }
        }
    }
}

#[test]
fn a_goal_behind_the_tank_and_a_lower_cruise_speed_work_without_reversing_the_world() {
    for (destination, speed) in [(-39.0, 1.2), (-28.2, 0.1)] {
        let mut state = settled(DrivingCourse::Flat, GrainShape::Round, true);
        let mut goal = TravelGoal::new(destination, speed);
        let mut cruising_speeds = Vec::new();
        for tick in 0..1000 {
            let decision = goal.decide(&state, 0);
            if tick > 120 && decision.mode == "cruise" {
                cruising_speeds.push(decision.actual_speed);
            }
            step_trial(&mut state, 0, Some(decision.axis));
        }
        assert!(cruising_speeds.len() > 120);
        let mean = cruising_speeds.iter().sum::<f32>() / cruising_speeds.len() as f32;
        assert!(
            (mean - speed).abs() < speed * 0.15,
            "requested {speed}, measured {mean}"
        );
        let last = snapshot(&state, 0, -1.0);
        assert!(
            (last.position[0] - destination).abs() <= ARRIVAL_DISTANCE,
            "{last:?}"
        );
        assert!(last.velocity[0].abs() < 0.15);
        assert_eq!(goal.retries, 0);
        state.audit().unwrap();
    }
}

#[test]
fn goal_refuses_the_trench_and_rocking_does_not_extend_a_stalled_attempt_forever() {
    for course in [DrivingCourse::Trench, DrivingCourse::Slope] {
        let mut state =
            settled_with_tuning(course, GrainShape::Hexagon, true, WheelTuning::REFERENCE);
        let mut goal = TravelGoal::new(course.finish_x(), 2.5);
        let mut blocked = 0;
        for _ in 0..3600 {
            let decision = goal.decide(&state, 0);
            blocked += usize::from(decision.mode == "blocked ground");
            step_trial(&mut state, 0, Some(decision.axis));
        }
        if course == DrivingCourse::Trench {
            assert!(blocked > 1800);
            assert_eq!(goal.retries, 0);
            let last = snapshot(&state, 0, 1.0);
            assert!(last.position[0] < -20.0 && last.supported, "{last:?}");
        } else {
            assert!(goal.exhausted, "{goal:?}");
            assert_eq!(goal.retries, RETRY_BUDGET);
            // Terrain changes do not replenish the same goal's retry budget.
            state.explode(Vec2::new(10.0, -5.0));
            for _ in 0..120 {
                let decision = goal.decide(&state, 0);
                assert_eq!(decision.mode, "stalled");
                assert_eq!(decision.axis, 0.0);
                step_trial(&mut state, 0, Some(decision.axis));
            }
        }
        state.audit().unwrap();
    }
}

fn retained_corner(course: DrivingCourse, shape: GrainShape) -> (ScorchedState, TravelGoal) {
    let mut state = settled_with_tuning(
        course,
        shape,
        true,
        WheelTuning {
            motor_torque: 120.0,
            ..WheelTuning::REFERENCE
        },
    );
    let mut goal = TravelGoal::new(course.finish_x(), 2.5);
    for _ in 0..900 {
        if !SupportCheck::Reference.supported(&state, 0)
            && SupportCheck::Current.supported(&state, 0)
        {
            return (state, goal);
        }
        let decision = goal.decide_reference(&state, 0, SupportCheck::Reference);
        step_support_trial(&mut state, 0, Some(decision.axis), SupportCheck::Reference);
    }
    panic!("retained support failure not reached");
}

#[test]
fn a_paid_retreat_recovers_captured_stalls_without_resetting_world_or_goal_history() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for course in [DrivingCourse::Slope, DrivingCourse::Rubble] {
            let (mut state, mut goal) = retained_corner(course, shape);
            assert_eq!(
                state.tick,
                if course == DrivingCourse::Slope {
                    552
                } else {
                    516
                }
            );
            let mut reference = state.clone();
            let mut old_goal = goal.clone();
            let mut replay = state.clone();
            let mut replay_goal = goal.clone();
            assert_eq!(physics_snapshot(&state), physics_snapshot(&reference));
            let mut retreated = 0;
            let mut arrived = 0;
            let mut old_arrived = 0;
            for tick in 0..2700 {
                let best = goal.best_distance;
                let retries = goal.retries;
                let decision = goal.decide(&state, 0);
                let repeated = replay_goal.decide(&replay, 0);
                let old = old_goal.decide_reference(&reference, 0, SupportCheck::Current);
                retreated += usize::from(decision.mode == "retreat");
                arrived += usize::from(decision.mode == "arrived");
                old_arrived += usize::from(old.mode == "arrived");
                assert!(
                    goal.best_distance <= best
                        && goal.retries >= retries
                        && goal.retries <= RETRY_BUDGET
                );
                assert_eq!(
                    serde_json::to_value(&decision).unwrap(),
                    serde_json::to_value(repeated).unwrap()
                );
                step_support_trial(&mut state, 0, Some(decision.axis), SupportCheck::Current);
                step_support_trial(&mut replay, 0, Some(decision.axis), SupportCheck::Current);
                step_support_trial(&mut reference, 0, Some(old.axis), SupportCheck::Current);
                if tick % 60 == 0 {
                    assert_eq!(state.observation_hash(), replay.observation_hash());
                    state.audit().unwrap();
                }
            }
            assert!(
                retreated > 120 && arrived > 120,
                "{shape:?} {course:?}: {goal:?}"
            );
            assert_eq!(goal.retries, 1);
            assert_eq!(old_arrived, 0);
            assert!(old_goal.exhausted);
            assert_eq!(
                serde_json::to_value(goal).unwrap(),
                serde_json::to_value(replay_goal).unwrap()
            );
            assert!(
                (snapshot(&state, 0, 1.0).position[0] - course.finish_x()).abs()
                    <= ARRIVAL_DISTANCE
            );
            reference.audit().unwrap();
        }
    }
}

#[test]
fn retreat_rechecks_new_holes_and_keeps_its_spent_retry() {
    let (mut state, mut goal) = retained_corner(DrivingCourse::Slope, GrainShape::Hexagon);
    for _ in 0..1200 {
        let decision = goal.decide(&state, 0);
        if decision.mode == "retreat" {
            break;
        }
        step_trial(&mut state, 0, Some(decision.axis));
    }
    assert_eq!(goal.retries, 1);
    assert!(goal.retreat.is_some());
    let position = state.physics.motion(state.tanks[0].body).unwrap().position;
    state.explode(position + Vec2::new(-3.5, -2.0));
    assert!(state.check_drive_direction(0, -1.0).reason.is_some());
    let decision = goal.decide(&state, 0);
    assert_eq!(decision.axis, 0.0);
    assert_eq!(decision.mode, "retreat blocked");
    assert!(goal.retreat.is_none());
    assert_eq!(
        goal.retries, 1,
        "changed terrain cannot replenish the allowance"
    );
    state.audit().unwrap();
}

#[test]
fn reaching_goal_x_in_midair_is_not_arrival() {
    let mut state = settled(DrivingCourse::Flat, GrainShape::Round, true);
    let gear = state.running_gear[0].clone().unwrap();
    let x = state
        .physics
        .motion(state.tanks[0].body)
        .unwrap()
        .position
        .x;
    for body in gear.bodies() {
        let motion = state.physics.motion(body).unwrap();
        state
            .physics
            .set_pose(body, motion.position + Vec2::Y * 30.0, motion.angle, true);
        state.physics.set_velocity(body, Vec2::ZERO, 0.0, true);
    }
    step_trial(&mut state, 0, Some(0.0));
    state
        .physics
        .set_velocity(state.tanks[0].body, Vec2::ZERO, 0.0, true);
    let decision = TravelGoal::new(x, 2.5).decide(&state, 0);
    assert_eq!(decision.mode, "no support");
    assert_eq!(decision.axis, 0.0);
}

#[test]
fn retreat_times_out_if_its_requested_drive_cannot_move_the_tank() {
    let (mut state, mut goal) = retained_corner(DrivingCourse::Slope, GrainShape::Round);
    for _ in 0..1200 {
        let decision = goal.decide(&state, 0);
        if decision.mode == "retreat" {
            break;
        }
        step_trial(&mut state, 0, Some(decision.axis));
    }
    assert!(goal.retreat.is_some());
    let started = state.tick;
    for _ in 0..=RETREAT_TICKS {
        let decision = goal.decide(&state, 0);
        if decision.mode == "retreat timeout" {
            assert_eq!(decision.axis, 0.0);
            assert_eq!(goal.retries, 1);
            assert!(goal.retreat.is_none());
            assert!(state.tick - started <= RETREAT_TICKS);
            return;
        }
        // A nonresponding actuator: gravity and native contacts still run.
        step_trial(&mut state, 0, Some(0.0));
    }
    panic!("an unsuccessful retreat must have a deadline");
}
