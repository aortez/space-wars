use super::*;

fn flying() -> SurfaceSortieState {
    let mut state = approach(std::f32::consts::FRAC_PI_2, 180.0, 0.0, 0.0, 0.0);
    state.pilots[0].flight_enabled = true;
    // The landing fixture starts with planet-relative spin. This free-flight
    // fixture starts still, otherwise automatic rate damping correctly fires.
    state.world.ships[0].omega = 0.0;
    state
}

#[test]
fn normal_flight_publishes_visible_exhaust_well_below_legacy_speed_threshold() {
    let mut state = flying();
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    );
    let ship = &state.world.ships[0];
    let effects = ship.thrusters.as_ref().unwrap();
    assert!(ship.velocity.length() < 10.0);
    assert!(effects.output.linear.y > 0.99);
    assert_eq!(effects.trail_count(), 1);
    assert!(
        ship.exhaust_trails.is_empty(),
        "no duplicate legacy emitter"
    );
    let paused = state.world.ships[0].clone();
    SurfaceSortieScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.world.ships[0], paused);
}

#[test]
fn real_left_and_right_controls_publish_opposing_turn_actuation() {
    for horizontal in [-1.0, 1.0] {
        let mut state = flying();
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal,
                ..Default::default()
            },
        );
        let effects = state.world.ships[0].thrusters.as_ref().unwrap();
        assert_eq!(effects.output.angular.signum(), -horizontal);
        assert!(effects.output.angular.abs() > 0.99);
        assert_eq!(effects.output.linear, Vec2::ZERO);
    }
}

#[test]
fn braking_feedback_opposes_motion_in_the_rotated_ship_frame() {
    let mut state = approach(std::f32::consts::FRAC_PI_2, 180.0, 0.7, 30.0, 20.0);
    state.pilots[0].flight_enabled = true;
    let ship = &state.world.ships[0];
    let local_velocity = ship.velocity.rotate_radians(-ship.rotation_radians);
    tick(
        &mut state,
        SurfaceSortieAction {
            brake_held: true,
            ..Default::default()
        },
    );
    let effects = state.world.ships[0].thrusters.as_ref().unwrap();
    assert!(effects.output.braking);
    assert!(effects.output.linear.dot(local_velocity) < -1.0);
}

#[test]
fn gravity_and_speed_governor_do_not_pretend_to_be_engine_acceleration() {
    let mut state = flying();
    tick(&mut state, SurfaceSortieAction::default());
    assert!(
        state.world.ships[0].velocity.length() > 0.0,
        "gravity still acts"
    );
    assert_eq!(
        state.world.ships[0].thrusters.as_ref().unwrap().output,
        thrusters::ThrusterOutput::default()
    );

    let mut state = flying();
    state.world.ships[0].velocity = Vec2::Y * 100.0;
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    );
    assert_eq!(state.world.ships[0].thrust, 1.0);
    let effects = state.world.ships[0].thrusters.as_ref().unwrap();
    assert_eq!(
        effects.output.linear,
        Vec2::ZERO,
        "governed engines produce no force"
    );
    assert_eq!(effects.trail_count(), 0);
}

#[test]
fn disembarked_pilots_do_not_light_the_parked_ships_stabilization_jets() {
    let mut state = parked();
    interact(&mut state);
    assert!(state.pilots[0].body.is_some());
    // A small parked-body disturbance still needs the existing rate damping.
    // It must not look like the spaceling's controls are operating the ship.
    state.world.ships[0].omega = 0.4;
    thrusters::advance(
        &mut state.world.ships[0],
        thrusters::ThrusterOutput {
            linear: Vec2::Y,
            angular: 1.0,
            braking: false,
        },
        1.0 / 60.0,
    );
    tick(&mut state, SurfaceSortieAction::default());
    let ship = &state.world.ships[0];
    assert_eq!((ship.thrust, ship.turn, ship.brake), (0.0, 0.0, 0.0));
    let effects = ship.thrusters.as_ref().unwrap();
    assert_eq!(effects.output, thrusters::ThrusterOutput::default());
    assert_eq!(effects.trail_count(), 0);
    let mut frame = RenderFrame::default();
    render_exhaust(&mut frame, ship);
    assert!(frame.layers.is_empty());
}

#[test]
fn on_foot_inputs_leave_ship_effects_off_and_reboarding_restores_them() {
    let mut state = parked();
    disembark(&mut state);
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            horizontal: 1.0,
            brake_held: true,
            ..Default::default()
        },
    );
    assert!(state.pilots[0].body.is_some());
    assert_eq!(
        state.world.ships[0].thrusters.as_ref().unwrap().output,
        thrusters::ThrusterOutput::default()
    );

    // Use a fresh parked session for a clean grounded transfer back aboard.
    let mut state = parked();
    disembark(&mut state);
    interact(&mut state);
    assert!(state.pilots[0].body.is_none());
    idle(&mut state, 1); // release-to-arm handoff
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    );
    assert!(
        state.world.ships[0]
            .thrusters
            .as_ref()
            .unwrap()
            .output
            .linear
            .y
            > 0.99
    );
    assert!(
        state.world.ships[0]
            .thrusters
            .as_ref()
            .unwrap()
            .trail_count()
            > 0
    );
}
