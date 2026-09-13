use super::*;

fn ship() -> ShipState {
    ShipState::new(0, Vec2::ZERO, Color::RED, 100, 1.0 / 60.0)
}

#[test]
fn force_couples_and_translation_use_the_opposite_exhaust_directions() {
    assert_eq!(
        ThrusterOutput {
            linear: Vec2::Y,
            ..Default::default()
        }
        .nozzles(),
        [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        ThrusterOutput {
            linear: -Vec2::Y,
            ..Default::default()
        }
        .nozzles(),
        [0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        ThrusterOutput {
            linear: Vec2::X,
            ..Default::default()
        }
        .nozzles(),
        [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]
    );
    assert_eq!(
        ThrusterOutput {
            angular: 1.0,
            ..Default::default()
        }
        .nozzles(),
        [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0]
    );
    assert_eq!(
        ThrusterOutput {
            angular: -1.0,
            ..Default::default()
        }
        .nozzles(),
        [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn acceleration_is_normalized_in_the_ship_frame_not_world_axes() {
    for angle in [0.0, 0.9, std::f32::consts::PI] {
        let local = Vec2::new(-0.6, -0.8);
        let output = ThrusterOutput::from_acceleration(
            (local * 40.0).rotate_radians(angle),
            -3.0,
            angle,
            40.0,
            6.0,
            true,
        );
        assert!(output.linear.distance_to(local) < 0.00001);
        assert_eq!(output.angular, -0.5);
        assert!(output.braking);
    }
}

#[test]
fn low_speed_exhaust_is_bounded_replayable_and_does_not_change_motion() {
    let mut first = ship();
    first.velocity = Vec2::Y * 18.0;
    let mut replay = first.clone();
    let output = ThrusterOutput {
        linear: Vec2::Y,
        ..Default::default()
    };
    for _ in 0..6000 {
        advance(&mut first, output, 1.0 / 60.0);
        advance(&mut replay, output, 1.0 / 60.0);
        assert!(first.thrusters.as_ref().unwrap().trail_count() <= MAX_TRAILS);
    }
    assert_eq!(first, replay);
    assert_eq!(first.position, Vec2::ZERO);
    assert_eq!(first.velocity, Vec2::Y * 18.0);
    assert_eq!(first.rotation_radians, 0.0);
    assert!(!first.thrusters.as_ref().unwrap().trails.is_empty());
    assert!(first.exhaust_trails.is_empty());
    let paused = first.clone();
    advance(&mut first, ThrusterOutput::default(), 0.0);
    assert_eq!(first, paused);
    for _ in 0..30 {
        advance(&mut first, ThrusterOutput::default(), 1.0 / 60.0);
    }
    assert_eq!(first.thrusters.as_ref().unwrap().trail_count(), 0);
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::new(0.0, 0.0), 40.0));
    render(&mut frame, &first);
    assert!(frame.layers.is_empty());
}

#[test]
fn dead_and_changed_form_ships_do_not_keep_a_previous_engines_wake() {
    let mut ship = ship();
    let output = ThrusterOutput {
        linear: Vec2::Y,
        ..Default::default()
    };
    advance(&mut ship, output, 1.0 / 60.0);
    ship.form = ShipForm::EscapePod;
    advance(&mut ship, ThrusterOutput::default(), 1.0 / 60.0);
    assert_eq!(ship.thrusters.as_ref().unwrap().trail_count(), 0);
    advance(&mut ship, output, 1.0 / 60.0);
    ship.dead = true;
    advance(&mut ship, output, 1.0 / 60.0);
    assert!(ship.thrusters.is_none());
}

#[test]
fn suspending_an_unoccupied_ship_reuses_storage_and_boarding_gets_a_fresh_wake() {
    let mut ship = ship();
    let output = ThrusterOutput {
        linear: Vec2::Y,
        ..Default::default()
    };
    for _ in 0..30 {
        advance(&mut ship, output, 1.0 / 60.0);
    }
    let storage = ship.thrusters.as_ref().unwrap().trails.as_ptr();
    let before = (ship.position, ship.velocity, ship.omega);
    for _ in 0..60 {
        suspend(&mut ship);
        let effects = ship.thrusters.as_ref().unwrap();
        assert_eq!(effects.output, ThrusterOutput::default());
        assert_eq!(effects.trails.as_ptr(), storage);
        assert!(effects.trails.is_empty());
    }
    assert_eq!((ship.position, ship.velocity, ship.omega), before);
    advance(&mut ship, output, 1.0 / 60.0);
    assert_eq!(ship.thrusters.as_ref().unwrap().trail_count(), 1);
    assert_eq!(ship.thrusters.as_ref().unwrap().output, output);
}

#[test]
fn scripted_cases_are_deterministic_and_have_a_fixed_primitive_bound() {
    for case in fixture::Case::ALL {
        let first = fixture::capture(case, 36, 40.0);
        let replay = fixture::capture(case, 36, 40.0);
        assert_eq!(first.frame, replay.frame);
        let count: usize = first
            .frame
            .layers
            .iter()
            .filter(|layer| layer.z == EXHAUST_LAYER)
            .map(|layer| layer.primitives.len())
            .sum();
        assert!(count <= MAX_TRAILS + 7 * 4);
        assert_eq!(count == 0, case == fixture::Case::Idle);
    }
}
