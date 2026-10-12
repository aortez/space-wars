use super::*;
use crate::driving_lab::physics_snapshot;

fn step(state: &mut ScorchedState) {
    ScorchedScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
}

fn captured_crater() -> ScorchedState {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            shape: GrainShape::Round,
            suspension: true,
            demo: true,
            ..Default::default()
        },
        314,
    );
    for _ in 0..645 {
        step(&mut state);
    }
    assert_eq!(state.observation_hash(), 0x2eea1b9a60b97b0a);
    assert!(state.firing_footing_target(1).is_some());
    state
}

#[test]
fn captured_cpu_reaches_better_actual_footing_then_stops_and_fires_with_replay() {
    let mut reference = captured_crater();
    let mut state = reference.clone();
    assert_eq!(physics_snapshot(&state), physics_snapshot(&reference));
    assert_eq!(state.observation_hash(), reference.observation_hash());
    state.config.cpu_footing[1] = true;
    let mut replay = state.clone();
    let shots = state.tanks[1].shots;
    let mut completed = false;
    let mut shot_from_hold = false;
    for _ in 0..320 {
        let before = state.physics.motion(state.tanks[1].body).unwrap();
        let better = state.better_firing_footing(1, 31.3);
        let was_holding = state.driving[1]
            .footing
            .as_ref()
            .is_some_and(|d| d.hold_until_shot.is_some());
        let previous_shots = state.tanks[1].shots;
        step(&mut state);
        step(&mut replay);
        step(&mut reference);
        let driver = state.driving[1].footing.as_ref().unwrap();
        if !completed && driver.completed == 1 {
            assert!(
                better
                    && before.linear_velocity.length() < 0.15
                    && before.angular_velocity.abs() < 0.15
            );
            completed = true;
        }
        if state.tanks[1].shots > previous_shots {
            assert!(before.linear_velocity.length() < 0.6 && before.angular_velocity.abs() < 0.35);
            shot_from_hold |= was_holding;
        }
        if state.tick == 780 {
            let old = reference.physics.motion(reference.tanks[1].body).unwrap();
            let current = state.physics.motion(state.tanks[1].body).unwrap();
            assert!(current.angle.to_degrees().abs() < 18.0);
            assert!(old.angle.to_degrees().abs() > 23.0);
            assert!(current.position.x > old.position.x + 1.0);
        }
        if state.tick.is_multiple_of(60) {
            assert_eq!(state.observation_hash(), replay.observation_hash());
            state.audit().unwrap();
            reference.audit().unwrap();
        }
    }
    assert!(completed && shot_from_hold && state.tanks[1].shots > shots);
    assert_eq!(physics_snapshot(&state), physics_snapshot(&replay));
}

#[test]
fn a_new_crater_stops_the_footing_goal_without_replacing_its_history() {
    let mut state = captured_crater();
    state.config.cpu_footing[1] = true;
    step(&mut state);
    let driver = state.driving[1].footing.as_ref().unwrap();
    let before = serde_json::to_value(driver.goal.as_ref().unwrap()).unwrap();
    let position = state.physics.motion(state.tanks[1].body).unwrap().position;
    state.explode(position + Vec2::new(3.0, -2.0));
    assert!(state.check_drive_direction(1, 1.0).reason.is_some());
    step(&mut state);
    assert_eq!(state.driving[1].diagnostics.track_speed, 0.0);
    let driver = state.driving[1].footing.as_ref().unwrap();
    let after = serde_json::to_value(driver.goal.as_ref().unwrap()).unwrap();
    for key in ["destination_x", "retries", "retry_at", "exhausted"] {
        assert_eq!(after[key], before[key]);
    }
    assert_eq!((driver.started, driver.completed), (1, 0));
    state.audit().unwrap();
}

#[test]
fn disabling_cpu_movement_preserves_the_existing_footing_goal() {
    let mut state = captured_crater();
    state.config.cpu_footing[1] = true;
    step(&mut state);
    state.command(Command::CpuDrive);
    let held = serde_json::to_value(&state.driving[1].footing).unwrap();
    for _ in 0..180 {
        step(&mut state);
    }
    assert_eq!(
        serde_json::to_value(&state.driving[1].footing).unwrap(),
        held
    );
    state.command(Command::CpuDrive);
    step(&mut state);
    let resumed = serde_json::to_value(&state.driving[1].footing).unwrap();
    for key in ["destination_x", "retries", "retry_at", "exhausted"] {
        assert_eq!(resumed["goal"][key], held["goal"][key]);
    }
    assert_eq!(resumed["started"], held["started"]);
    state.audit().unwrap();
}
