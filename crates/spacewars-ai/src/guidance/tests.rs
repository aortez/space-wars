use super::*;
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, pilot::MaterialFlightStart};
use std::time::Duration;

fn observation() -> FreeFlightObservation {
    let mut state = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(
            PlayerId::PLAYER_1,
            MaterialFlightStart {
                bearing: 0.0,
                altitude: 90.0,
                radial_speed: 0.0,
                lateral_speed: 0.0,
                heading_offset: 0.0,
            },
        )],
    );
    SurfaceSortieScenario::step(
        &mut state,
        &Movement::default().encode(PlayerId::PLAYER_1),
        Duration::from_nanos(16_666_667),
    );
    state.free_flight_observation(0).unwrap()
}

fn binding(o: &FreeFlightObservation) -> Binding {
    Binding {
        owner: o.owner,
        spaceling: o.spaceling,
        vehicle: o.vehicle,
        planet: o.planet,
        epoch: 7,
    }
}

#[test]
fn local_features_preserve_position_velocity_gravity_and_braking_frame_distinctions() {
    let mut o = observation();
    o.ship.position = Vec2::new(10.0, 20.0);
    o.ship.velocity = Vec2::new(3.0, 4.0);
    o.ship.angle = std::f32::consts::FRAC_PI_2;
    o.ship.spin = 0.9;
    o.gravity = Vec2::new(0.0, -9.0);
    o.frame_velocity = Vec2::new(1.0, 1.0);
    let target = Target {
        id: 1,
        position: Vec2::new(30.0, 50.0),
        velocity: Vec2::new(5.0, 7.0),
    };
    let features = Features::read(&o, target);
    let encoded = features.encode().unwrap();
    let expected = [
        0.3,
        -0.2,
        3.0 / 70.0,
        -2.0 / 70.0,
        3.0 / 70.0,
        -2.0 / 70.0,
        -0.2,
        0.0,
        0.5,
    ];
    for (actual, expected) in encoded.inputs.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-6);
    }
    assert_eq!(encoded.saturated, [false; 9]);
    o.frame_velocity = Vec2::new(-10.0, 8.0);
    let changed = Features::read(&o, target);
    assert_eq!(changed.target_velocity, features.target_velocity);
    assert_ne!(changed.frame_velocity, features.frame_velocity);
    let mut far = features;
    far.target_offset = Vec2::new(200.0, -100.0);
    let encoded = far.encode().unwrap();
    assert_eq!(&encoded.inputs[..2], &[1.0, -1.0]);
    assert_eq!(&encoded.saturated[..2], &[true, false]);
    far.spin = f32::NAN;
    assert!(far.encode().is_err());
}

#[test]
fn decoder_is_neutral_at_zero_allows_both_buttons_and_emits_only_owned_controls() {
    assert_eq!(Movement::decode(&[0.0; 3]).unwrap(), Movement::default());
    let movement = Movement::decode(&[0.5, 1.0, 1.0]).unwrap();
    assert!((movement.turn - 0.5_f32.tanh()).abs() <= f32::EPSILON);
    assert!(movement.thrust && movement.brake);
    let actions = movement.encode(PlayerId::PLAYER_1);
    let controls = SurfaceSortieAction::decode(&actions[0]).unwrap().1;
    assert!(controls.primary_held && controls.brake_held && !controls.interact_held);
    assert!(!SurfaceWingAction::decode(&actions[1]).unwrap().1.closed);
    assert_eq!(
        SurfaceWeaponAction::decode(&actions[2]).unwrap().1,
        SurfaceWeaponAction::default()
    );
    assert_eq!(
        SurfaceMiningAction::decode(&actions[3]).unwrap().1,
        SurfaceMiningAction::default()
    );
    assert!(Movement::decode(&[f32::INFINITY, 0.0, 0.0]).is_err());
    assert!(Movement::decode(&[0.0; 2]).is_err());
}

#[test]
fn duplicate_calls_pause_reset_target_changes_and_epoch_checks_preserve_memory_contract() {
    let o = observation();
    let binding = binding(&o);
    let target = Target {
        id: 1,
        position: o.ship.position + Vec2::Y * 20.0,
        velocity: Vec2::ZERO,
    };
    let request = Request {
        binding,
        tick: o.tick,
        target_id: 1,
        features: Features::read(&o, target),
    };
    let mut a = Controller::new(Policy::Reference, binding).unwrap();
    let first = a.decide(request).unwrap();
    let memory = a.thrust_remainder;
    assert_eq!(a.decide(request).unwrap(), first);
    assert_eq!(a.thrust_remainder, memory);
    let mut resumed = a.clone();
    let next = Request {
        tick: request.tick + 1,
        ..request
    };
    assert_eq!(a.decide(next).unwrap(), resumed.decide(next).unwrap());
    let mut reset = Controller::new(Policy::Reference, binding).unwrap();
    assert_eq!(reset.decide(request).unwrap(), first);
    let new_target = Request {
        tick: next.tick + 1,
        target_id: 2,
        ..next
    };
    let mut fresh = Controller::new(Policy::Reference, binding).unwrap();
    assert_eq!(
        a.decide(new_target).unwrap(),
        fresh.decide(new_target).unwrap()
    );
    assert!(a.decide(next).is_err());
    assert!(
        a.decide(Request {
            binding: Binding {
                epoch: 8,
                ..binding
            },
            tick: new_target.tick + 1,
            ..new_target
        })
        .is_err()
    );
    let mut conflicting = new_target;
    conflicting.features.spin += 0.1;
    assert!(a.decide(conflicting).is_err());
}

#[test]
fn ordinary_controls_turn_toward_the_right_and_release_all_movement() {
    let mut state = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(
            PlayerId::PLAYER_1,
            MaterialFlightStart {
                bearing: 0.0,
                altitude: 90.0,
                radial_speed: 0.0,
                lateral_speed: 0.0,
                heading_offset: 0.0,
            },
        )],
    );
    let dt = Duration::from_nanos(16_666_667);
    SurfaceSortieScenario::step(
        &mut state,
        &Movement::default().encode(PlayerId::PLAYER_1),
        dt,
    );
    let before = state.free_flight_observation(0).unwrap().ship.angle;
    for _ in 0..20 {
        SurfaceSortieScenario::step(
            &mut state,
            &Movement {
                turn: 0.5,
                ..Movement::default()
            }
            .encode(PlayerId::PLAYER_1),
            dt,
        );
    }
    let after = state.free_flight_observation(0).unwrap().ship;
    assert!(after.spin < 0.0);
    assert!(
        (after.angle - before + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI
            < 0.0
    );
    SurfaceSortieScenario::step(
        &mut state,
        &Movement {
            turn: 0.5,
            thrust: true,
            brake: true,
        }
        .encode(PlayerId::PLAYER_1),
        dt,
    );
    assert_eq!(
        state.free_flight_observation(0).unwrap().applied,
        [0.5, 1.0, 1.0]
    );
    SurfaceSortieScenario::step(
        &mut state,
        &Movement::default().encode(PlayerId::PLAYER_1),
        dt,
    );
    assert_eq!(state.free_flight_observation(0).unwrap().applied, [0.0; 3]);
}
