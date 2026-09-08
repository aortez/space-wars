use super::*;
use engine_rapier::world::{
    BodyId as PhysicsBodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
};

mod compatibility_tests;
mod flight_tests;
mod motion_tests;
mod multiplayer_tests;
mod outpost_tests;
mod recovery_tests;
mod travel_tests;

fn tick(state: &mut SurfaceSortieState, input: SurfaceSortieAction) {
    SurfaceSortieScenario::step(
        state,
        &[input.encode(PlayerId::PLAYER_1)],
        Duration::from_secs_f64(1.0 / 60.0),
    );
}

fn idle(state: &mut SurfaceSortieState, ticks: usize) {
    for _ in 0..ticks {
        tick(state, SurfaceSortieAction::default());
    }
}

fn parked() -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 7);
    idle(&mut state, 120);
    assert!(state.vehicle_settled(0), "{:?}", state.observation(0));
    state
}

fn approach(
    up_angle: f32,
    height: f32,
    tilt: f32,
    descent: f32,
    sideways: f32,
) -> SurfaceSortieState {
    approach_in(
        SurfaceMotionPreset::Stationary,
        up_angle,
        height,
        tilt,
        descent,
        sideways,
    )
}

fn approach_in(
    preset: SurfaceMotionPreset,
    up_angle: f32,
    height: f32,
    tilt: f32,
    descent: f32,
    sideways: f32,
) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init(preset, 7);
    state.pilots[0].controls_armed = true;
    let planet = state.world.planets[0];
    let up = Vec2::from_radians(up_angle);
    let center = planet.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 5.5 + height);
    let ship = &mut state.world.ships[0];
    ship.position = center - SHIP_PIVOT;
    ship.rotation_radians = rotation_for_direction(up) + tilt;
    ship.direction = Vec2::Y.rotate_radians(ship.rotation_radians);
    ship.velocity =
        planet_surface_velocity(&planet, center) - up * descent + Vec2::new(up.y, -up.x) * sideways;
    ship.omega = planet.wrapper_omega / (1.0 - ship.turn_power / ship.delta_time);
    state
}

#[test]
fn minimap_retains_world_context_and_uses_the_actual_camera_aspect() {
    let mut state = parked();
    let initial_map = SurfaceSortieScenario::minimap_frame(&state, 0, 16.0 / 9.0);
    // Move far enough that the planet is outside the main camera.
    state.world.ships[0].position += Vec2::new(200.0, 170.0);
    idle(&mut state, 1);
    let camera = SurfaceSortieScenario::render_frame(&state).camera;
    assert!(
        Vec2::new(camera.center.x, camera.center.y).distance_to(state.world.planets[0].position)
            > camera.height
    );
    for aspect in [16.0 / 9.0, 9.0 / 16.0] {
        let map = SurfaceSortieScenario::minimap_frame(&state, 0, aspect);
        assert_eq!(map.camera, initial_map.camera);
        let layer = map
            .ordered_layers()
            .into_iter()
            .find(|layer| layer.z == 0)
            .unwrap();
        let RenderPrimitive::Polygon(rect) = &layer.primitives[0] else {
            panic!("camera footprint")
        };
        let width = rect.points[1].x - rect.points[0].x;
        let height = rect.points[2].y - rect.points[1].y;
        assert!((width / height - aspect).abs() < 1.0e-5);
        assert!((height - camera.height).abs() < 1.0e-4);
    }
}

#[test]
fn gentle_flown_landings_work_around_the_planet_and_access_follows_the_ship() {
    for index in 0..8 {
        let mut state = approach(
            index as f32 * std::f32::consts::TAU / 8.0,
            18.0,
            5.0_f32.to_radians(),
            5.0,
            3.0,
        );
        let original_health = state.world.ships[0].life;
        let mut landed_at = None;
        for tick_index in 0..900 {
            idle(&mut state, 1);
            assert!(!state.world.physics.ship_is_constrained(0));
            assert!(state.world.spaceport_contacts.is_empty());
            if state.vehicle_settled(0) {
                landed_at = Some(tick_index);
                break;
            }
        }
        assert!(
            landed_at.is_some(),
            "bearing {index}: {:?}",
            state.observation(0)
        );
        assert_eq!(state.world.ships[0].life, original_health);
        assert_eq!(state.pilots[0].landing.supported_feet, 2);
        assert!(
            state
                .access_position(0)
                .distance_to(state.world.ships[0].position + SHIP_PIVOT)
                < 12.0
        );
        disembark(&mut state);
        interact(&mut state);
        assert_eq!(
            state.pilots[0].last_transfer,
            TransferResult::Boarded,
            "bearing {index}: {:?}",
            state.observation(0)
        );
    }
}

#[test]
fn thrust_lifts_off_physically_and_released_thrust_returns_to_a_landed_ship() {
    let mut state = parked();
    let start = state.world.ships[0].position;
    let bodies = state.world.physics.world.body_count();
    for index in 0..35 {
        let before = state.world.ships[0].position;
        tick(
            &mut state,
            SurfaceSortieAction {
                primary_held: true,
                ..SurfaceSortieAction::default()
            },
        );
        assert!(!state.vehicle_settled(0));
        assert!(!state.world.physics.ship_is_constrained(0));
        if index == 0 {
            assert!(state.world.ships[0].position.distance_to(before) < 0.05);
        }
    }
    assert!(state.world.ships[0].position.distance_to(start) > 3.0);
    assert_eq!(state.try_transfer(0), TransferResult::ShipNotSettled);
    idle(&mut state, 900);
    assert!(state.vehicle_settled(0), "{:?}", state.observation(0));
    assert_eq!(state.world.physics.world.body_count(), bodies);
}

#[test]
fn hovering_or_nose_contact_does_not_allow_transfer_and_settling_takes_time() {
    let mut state = approach(std::f32::consts::FRAC_PI_2, 1.0, 0.0, 0.0, 0.0);
    idle(&mut state, 1);
    assert_eq!(state.try_transfer(0), TransferResult::ShipNotSettled);
    let mut saw_settling = false;
    for _ in 0..300 {
        idle(&mut state, 1);
        if state.pilots[0].landing.phase == LandingPhase::Settling {
            saw_settling = true;
            assert_eq!(state.try_transfer(0), TransferResult::ShipNotSettled);
        }
        if state.vehicle_settled(0) {
            break;
        }
    }
    assert!(saw_settling && state.vehicle_settled(0));

    let mut nose_first = approach(
        std::f32::consts::FRAC_PI_2,
        0.0,
        std::f32::consts::PI,
        2.0,
        0.0,
    );
    let mut touched_hull = false;
    for _ in 0..60 {
        idle(&mut nose_first, 1);
        assert!(!nose_first.vehicle_settled(0));
        assert_eq!(nose_first.pilots[0].landing.assist_strength, 0.0);
        touched_hull |= nose_first.world.physics.contacts().iter().any(|contact| {
            matches!(
                (contact.a, contact.b),
                (
                    physics::MechanicalEntity::Body(BodyId::Planet(0)),
                    physics::MechanicalEntity::Ship(0)
                )
            )
        });
    }
    assert!(touched_hull, "test must exercise a physical hull collision");
    assert_eq!(nose_first.try_transfer(0), TransferResult::ShipNotSettled);
}

#[test]
fn assist_reduces_sideways_drift_but_is_bounded_and_does_not_auto_point_the_ship() {
    let mut aligned = approach(std::f32::consts::FRAC_PI_2, 8.0, 0.0, 4.0, 8.0);
    idle(&mut aligned, 30);
    assert!(
        aligned.pilots[0].landing.lateral_speed.abs() < 5.0,
        "{:?}",
        aligned.pilots[0].landing
    );
    assert!(
        aligned.pilots[0].landing.descent_speed > 0.5,
        "assist must descend, not hover"
    );
    let mut sideways = approach(
        std::f32::consts::FRAC_PI_2,
        12.0,
        std::f32::consts::FRAC_PI_2,
        0.0,
        0.0,
    );
    let original_angle = sideways.world.ships[0].rotation_radians;
    idle(&mut sideways, 20);
    assert_eq!(sideways.pilots[0].landing.assist_strength, 0.0);
    assert!((sideways.world.ships[0].rotation_radians - original_angle).abs() < 0.03);

    let mut fast = approach(std::f32::consts::FRAC_PI_2, 8.0, 0.0, 65.0, 0.0);
    let health = fast.world.ships[0].life;
    idle(&mut fast, 60);
    assert!(
        fast.world.ships[0].life < health || fast.world.ships[0].form != ShipForm::Ship,
        "a hard impact must not be silently converted to a safe landing: {:?}",
        fast.observation(0)
    );
}

fn interact(state: &mut SurfaceSortieState) {
    tick(
        state,
        SurfaceSortieAction {
            interact_held: true,
            ..SurfaceSortieAction::default()
        },
    );
}

fn disembark(state: &mut SurfaceSortieState) {
    interact(state);
    assert_eq!(state.pilots[0].last_transfer, TransferResult::Exited);
    idle(state, 120);
    assert!(
        state.spaceling_snapshot(0).unwrap().grounded(),
        "{:?}",
        state.observation(0)
    );
}

#[test]
fn parked_exit_walk_jump_return_and_reboard_preserves_identity_and_vehicle() {
    let mut state = parked();
    let original = state.observation(0);
    state.world.ships[0].life = 70.0;
    disembark(&mut state);
    assert_eq!(
        state.observation(0).physical_bodies,
        original.physical_bodies + 1
    );
    let start_position = state.observation(0).position;
    for _ in 0..40 {
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: 1.0,
                ..SurfaceSortieAction::default()
            },
        );
    }
    assert!(state.observation(0).position.distance_to(start_position) > 1.0);
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..SurfaceSortieAction::default()
        },
    );
    assert_eq!(state.spaceling_snapshot(0).unwrap().jumps, 1);
    idle(&mut state, 120);
    // Walk back toward the real access point; no test pose teleport is used.
    for _ in 0..600 {
        let position = state.observation(0).position;
        let offset = state.access_position(0) - position;
        if offset.length() < 1.3 {
            break;
        }
        let up = (position - state.world.planets[0].position).normalized();
        let right = Vec2::new(up.y, -up.x);
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: offset.dot(right).signum(),
                ..SurfaceSortieAction::default()
            },
        );
    }
    idle(&mut state, 60);
    assert!(state.vehicle_settled(0));
    interact(&mut state);
    assert_eq!(
        state.pilots[0].last_transfer,
        TransferResult::Boarded,
        "{:?}",
        state.observation(0)
    );
    let after = state.observation(0);
    assert_eq!(after.spaceling, original.spaceling);
    assert_eq!(after.owner, original.owner);
    assert_eq!(after.vehicle, original.vehicle);
    assert_eq!(after.ship_health, 70.0);
    assert_eq!(after.physical_bodies, original.physical_bodies);
    assert_eq!(after.transfers, 2);
    assert_eq!(state.world.planets[0].owner_id, None);
    assert!(state.world.rovers.is_empty());
}

#[test]
fn spawn_inherits_rotating_surface_velocity_without_transporting_airborne_body() {
    let mut state = parked();
    let planet = state.world.planets[0];
    assert_eq!(state.try_transfer(0), TransferResult::Exited);
    let snapshot = state.spaceling_snapshot(0).unwrap();
    assert_eq!(
        snapshot.motion.linear_velocity,
        motion::point_velocity(state.planet_motion(0), snapshot.motion.position)
    );
    assert_eq!(
        snapshot.motion.angular_velocity,
        state.planet_motion(0).angular_velocity
    );
    assert_eq!(snapshot.contacts, 0);
    // In flight, changing the terrain angle must not teleport the actor.
    let body = state.pilots[0].body.as_ref().unwrap().body();
    let position = planet.position + Vec2::Y * 100.0;
    state
        .world
        .physics
        .world
        .set_pose(body, position, 0.0, true);
    state
        .world
        .physics
        .world
        .set_velocity(body, Vec2::new(2.0, 0.0), 0.0, true);
    state.world.planets[0].wrapper_angle += 0.7;
    idle(&mut state, 1);
    assert!(state.observation(0).position.distance_to(position) < 0.1);
    let expected_velocity = Vec2::new(2.0, -GRAVITY * planet.mass / 100.0_f32.powi(2));
    assert!((state.observation(0).velocity - expected_velocity).length() < 1.0e-4);
}

#[test]
fn repeated_sorties_stay_supported_around_the_rotating_planet_without_body_leaks() {
    let mut state = parked();
    let bodies = state.observation(0).physical_bodies;
    for _ in 0..12 {
        disembark(&mut state);
        idle(&mut state, 1200);
        let snapshot = state.spaceling_snapshot(0).unwrap();
        assert!(snapshot.grounded());
        assert_eq!(snapshot.knockdowns, 0);
        assert!(
            snapshot
                .motion
                .position
                .distance_to(state.access_position(0))
                < 1.5
        );
        assert_eq!(state.observation(0).physical_bodies, bodies + 1);
        interact(&mut state);
        assert_eq!(state.pilots[0].last_transfer, TransferResult::Boarded);
        assert_eq!(state.observation(0).physical_bodies, bodies);
        idle(&mut state, 60);
    }
    assert_eq!(state.pilots[0].transfers, 24);
    assert_eq!(state.world.planets[0].owner_id, None);
}

#[test]
fn held_inputs_cannot_leak_across_either_transfer() {
    let mut state = parked();
    let held = SurfaceSortieAction {
        horizontal: 1.0,
        primary_held: true,
        interact_held: true,
        brake_held: false,
    };
    for _ in 0..180 {
        tick(&mut state, held);
    }
    assert_eq!(state.pilots[0].transfers, 1);
    assert!(!state.pilots[0].controls_armed);
    assert_eq!(state.spaceling_snapshot(0).unwrap().jumps, 0);
    assert_eq!(state.world.ships[0].thrust, 0.0);
    idle(&mut state, 120);
    for _ in 0..120 {
        tick(&mut state, held);
    }
    assert_eq!(state.pilots[0].transfers, 2, "{:?}", state.observation(0));
    assert!(matches!(state.location(0), PilotLocation::Aboard(_)));
    assert_eq!(state.world.ships[0].thrust, 0.0);
    assert_eq!(state.world.ships[0].turn, 0.0);
    assert!(!state.world.ships[0].laser_firing);
    idle(&mut state, 1);
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..SurfaceSortieAction::default()
        },
    );
    assert_eq!(state.world.ships[0].thrust, 1.0);
}

#[test]
fn unsafe_exit_blocked_capsule_remote_or_airborne_boarding_and_lost_vehicle_are_rejected() {
    let mut state = parked();
    state.world.ships[0].velocity += Vec2::Y * 20.0;
    idle(&mut state, 1);
    assert_eq!(state.try_transfer(0), TransferResult::ShipNotSettled);
    let mut state = parked();
    let obstacle = PhysicsId::new(45_000);
    let center = state.access_position(0) + state.access_up(0) * 1.0;
    let collider = ColliderSpec::ball(ColliderId::new(obstacle, ColliderRole::PRIMARY, 0), 1.3);
    assert!(state.world.physics.world.insert_body(
        PhysicsBodyId::new(obstacle, BodyRole::PRIMARY),
        BodySpec {
            kind: engine_rapier::world::BodyKind::Fixed,
            position: center,
            ..BodySpec::default()
        },
        &[collider]
    ));
    idle(&mut state, 1); // Publish the obstacle in the broad-phase index.
    assert_eq!(state.try_transfer(0), TransferResult::ExitBlocked);
    assert!(state.pilots[0].body.is_none());
    state.world.physics.world.remove_entity(obstacle);
    idle(&mut state, 1);
    disembark(&mut state);
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..SurfaceSortieAction::default()
        },
    );
    assert_eq!(state.try_transfer(0), TransferResult::MustBeSupported);
    idle(&mut state, 120);
    for _ in 0..120 {
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: 1.0,
                ..SurfaceSortieAction::default()
            },
        );
    }
    idle(&mut state, 60);
    assert_eq!(state.try_transfer(0), TransferResult::TooFar);
    state.world.ships[0].change_to_escape_pod();
    assert_eq!(state.try_transfer(0), TransferResult::VehicleUnavailable);
    assert!(state.pilots[0].body.is_some());
}

#[test]
fn restart_actions_and_observations_are_reproducible_and_malformed_actions_are_rejected() {
    let mut a = SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 3);
    let mut b = SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 3);
    for frame in 0..800 {
        let input = SurfaceSortieAction {
            interact_held: frame == 120 || frame == 600,
            horizontal: if (300..350).contains(&frame) {
                1.0
            } else {
                0.0
            },
            primary_held: frame == 400,
            ..SurfaceSortieAction::default()
        };
        tick(&mut a, input);
        tick(&mut b, input);
        assert_eq!(
            SurfaceSortieScenario::observe(&a).payload,
            SurfaceSortieScenario::observe(&b).payload
        );
    }
    for payload in [vec![], vec![0; 6], vec![0, 0, 0, 0, 2, 0, 0]] {
        assert!(SurfaceSortieAction::decode(&Action::scenario(CONTROL_V2, payload)).is_none());
    }
    assert!(
        SurfaceSortieAction::decode(
            &SurfaceSortieAction {
                horizontal: f32::NAN,
                ..SurfaceSortieAction::default()
            }
            .encode(PlayerId::PLAYER_1)
        )
        .is_none()
    );
    assert_eq!(
        SurfaceSortieScenario::observe(&SurfaceSortieScenario::init(
            SurfaceMotionPreset::default(),
            3
        ))
        .payload,
        SurfaceSortieScenario::observe(&SurfaceSortieScenario::init(
            SurfaceMotionPreset::default(),
            3
        ))
        .payload
    );
}
