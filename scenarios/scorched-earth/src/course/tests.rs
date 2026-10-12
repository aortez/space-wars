use super::*;
use crate::driving_lab::{physics_snapshot, snapshot, step_trial};

#[test]
fn course_controls_reset_preserve_options_and_park_the_other_tank() {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            suspension: true,
            demo: true,
            ..Default::default()
        },
        42,
    );
    for course in DrivingCourse::ALL {
        state.command(Command::Course);
        assert_eq!(state.config.course, Some(course));
        assert!(state.config.suspension && state.config.demo && state.config.cpu_driving);
        let initial = state.observation_hash();
        let parked = snapshot(&state, 1, -1.0).position[0];
        for _ in 0..240 {
            state.command(Command::Fire);
            step_trial(&mut state, 0, None);
        }
        assert_eq!((state.shots, state.impacts), (0, 0));
        assert_eq!(state.driving[1].diagnostics.powered_ticks, 0);
        assert!((snapshot(&state, 1, -1.0).position[0] - parked).abs() < 0.1);
        state.command(Command::Select);
        assert_eq!(state.selected, 0);
        state.audit().unwrap();
        state.command(Command::Reset);
        assert_eq!(state.observation_hash(), initial);
    }
    state.command(Command::Course);
    assert_eq!(state.config.course, None);
    state.command(Command::Course);
    state.command(Command::Roof);
    assert!(state.config.roof && state.config.course.is_none());
    state.command(Command::Barrage);
    assert_eq!(state.config.course, Some(DrivingCourse::Flat));
    state.command(Command::Barrage);
    assert_eq!(state.config.course, Some(DrivingCourse::Step));
}

#[test]
fn all_courses_conserve_and_clone_continuations_replay_both_gears_and_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for suspension in [false, true] {
            for course in DrivingCourse::ALL {
                let mut state = ScorchedState::new(
                    ScorchedConfig {
                        shape,
                        suspension,
                        course: Some(course),
                        demo: true,
                        ..Default::default()
                    },
                    42,
                );
                if course == DrivingCourse::Rubble {
                    assert!(state.loose_cells() > 20, "actual released dirt");
                }
                for _ in 0..120 {
                    step_trial(&mut state, 0, Some(0.0));
                }
                let before = state.observation_hash();
                let native = physics_snapshot(&state);
                let _ = snapshot(&state, 0, 1.0);
                assert_eq!(state.observation_hash(), before);
                assert_eq!(
                    physics_snapshot(&state),
                    native,
                    "inspection must be read-only"
                );
                let mut replay = state.clone();
                assert_eq!(
                    physics_snapshot(&replay),
                    native,
                    "fork preserves contacts and motor state"
                );
                for _ in 0..720 {
                    step_trial(&mut state, 0, Some(1.0));
                    step_trial(&mut replay, 0, Some(1.0));
                }
                assert_eq!(state.observation_hash(), replay.observation_hash());
                state.audit().unwrap();
                if course == DrivingCourse::Flat {
                    assert!(snapshot(&state, 0, 1.0).position[0] > course.finish_x());
                }
            }
        }
    }
}

#[test]
fn unchanged_cpu_trial_matches_normal_steps_without_resetting_combat() {
    for suspension in [false, true] {
        let mut ordinary = ScorchedState::new(
            ScorchedConfig {
                suspension,
                demo: true,
                ..Default::default()
            },
            42,
        );
        let mut trial = ordinary.clone();
        for _ in 0..360 {
            ScorchedScenario::step(&mut ordinary, &[], Duration::from_secs_f32(DT));
            step_trial(&mut trial, 0, None);
            assert_eq!(ordinary.observation_hash(), trial.observation_hash());
        }
        assert!(ordinary.shots > 0);
        let frozen_controller = serde_json::to_value(&trial.driving[0]).unwrap();
        let mut held = trial.clone();
        step_trial(&mut held, 0, Some(1.0));
        let controller = serde_json::to_value(&held.driving[0]).unwrap();
        for field in [
            "next_plan",
            "retry_at",
            "cpu_direction",
            "stalled_plans",
            "planned_x",
        ] {
            assert_eq!(controller[field], frozen_controller[field], "{field}");
        }
    }
}

#[test]
fn small_pool_retains_the_whole_rubble_source() {
    let state = ScorchedState::new(
        ScorchedConfig {
            course: Some(DrivingCourse::Rubble),
            max_grains: 1,
            ..Default::default()
        },
        42,
    );
    assert_eq!(state.loose_cells(), 0);
    assert_eq!(
        state.terrain[0].terrain.hash(),
        DrivingCourse::Rubble.terrain().hash()
    );
    state.audit().unwrap();
}

#[test]
fn playable_goal_courses_stop_at_the_flag_and_replay_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for course in DrivingCourse::ALL {
            let mut state = ScorchedState::new(
                ScorchedConfig {
                    shape,
                    suspension: true,
                    course: Some(course),
                    course_goal_driving: true,
                    demo: true,
                    ..Default::default()
                },
                42,
            );
            let mut replay = state.clone();
            for tick in 0..1500 {
                ScorchedScenario::step(&mut state, &[], Duration::from_secs_f32(DT));
                ScorchedScenario::step(&mut replay, &[], Duration::from_secs_f32(DT));
                if tick % 60 == 0 {
                    assert_eq!(state.observation_hash(), replay.observation_hash());
                    state.audit().unwrap();
                }
            }
            let drive = snapshot(&state, 0, 1.0);
            assert!(
                drive.supported && drive.velocity[0].abs() < 0.15,
                "{shape:?} {course:?}: {drive:?}"
            );
            if course == DrivingCourse::Trench {
                assert!(drive.position[0] < -20.0);
                assert_eq!(drive.driving.mode, "blocked ground");
            } else {
                assert!(
                    (drive.position[0] - course.finish_x()).abs() <= 0.2,
                    "{shape:?} {course:?}: {drive:?}"
                );
                assert_eq!(drive.driving.mode, "arrived");
            }
            assert_eq!(
                (state.shots, state.driving[1].diagnostics.powered_ticks),
                (0, 0)
            );
        }
    }
}

#[test]
fn driver_switches_preserve_goal_history_and_reset_explicitly_starts_over() {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            suspension: true,
            course: Some(DrivingCourse::Slope),
            course_goal_driving: true,
            demo: true,
            wheel_tuning: WheelTuning::REFERENCE,
            ..Default::default()
        },
        42,
    );
    for _ in 0..2700 {
        ScorchedScenario::step(&mut state, &[], Duration::from_secs_f32(DT));
    }
    let retained = serde_json::to_value(&state.course_goal).unwrap();
    assert_eq!(retained["retries"], 2);
    state.command(Command::Demo);
    for _ in 0..60 {
        ScorchedScenario::step(&mut state, &[], Duration::from_secs_f32(DT));
    }
    assert_eq!(serde_json::to_value(&state.course_goal).unwrap(), retained);
    state.command(Command::CpuDrive);
    assert!(!state.config.cpu_driving);
    state.command(Command::CpuDrive);
    assert!(state.config.cpu_driving && !state.config.course_goal_driving);
    state.command(Command::CpuDrive);
    assert!(state.config.cpu_driving && state.config.course_goal_driving);
    assert_eq!(serde_json::to_value(&state.course_goal).unwrap(), retained);
    state.command(Command::Demo);
    ScorchedScenario::step(&mut state, &[], Duration::from_secs_f32(DT));
    assert_eq!(state.course_decision.as_ref().unwrap().mode, "stalled");
    state.command(Command::Reset);
    let reset = serde_json::to_value(&state.course_goal).unwrap();
    assert_eq!(reset["retries"], 0);
    assert_eq!(reset["exhausted"], false);
    state.command(Command::Course);
    assert_eq!(state.config.course, Some(DrivingCourse::Trench));
    assert!(state.course_decision.is_none());
}
