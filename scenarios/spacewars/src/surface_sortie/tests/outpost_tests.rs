use super::*;

pub(super) fn walk_to(
    state: &mut SurfaceSortieState,
    target: impl Fn(&SurfaceSortieState) -> Vec2,
) {
    for _ in 0..600 {
        let snapshot = state.spaceling_snapshot().unwrap();
        let delta = target(state) - snapshot.motion.position;
        if delta.length() < 0.3 {
            idle(state, 12);
            return;
        }
        let up = (snapshot.motion.position - state.world.planets[state.pilot.planet].position)
            .normalized();
        tick(
            state,
            SurfaceSortieAction {
                horizontal: delta.dot(Vec2::new(up.y, -up.x)).signum(),
                ..SurfaceSortieAction::default()
            },
        );
    }
    panic!("could not walk to target: {:?}", state.observation());
}

pub(super) fn terminal_approach(state: &SurfaceSortieState) -> Vec2 {
    let planet = &state.world.planets[state.outposts[0].planet];
    let up = state.outposts[0]
        .up(planet)
        .rotate_radians(2.0 / (planet.radius * BODY_BOUNDS_RADIUS_SCALE));
    planet.position
        + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + SurfaceSortieState::spec().half_height())
}

fn at_terminal() -> SurfaceSortieState {
    let mut state = parked();
    disembark(&mut state);
    walk_to(&mut state, terminal_approach);
    assert_eq!(
        state.observation().outpost.capture_status,
        CaptureStatus::Capturing,
        "{:?}",
        state.observation()
    );
    state
}

fn capture(state: &mut SurfaceSortieState) {
    for _ in 0..200 {
        idle(state, 1);
        if state.outposts[0].owner == Some(state.pilot.owner) {
            return;
        }
    }
    panic!("capture did not complete: {:?}", state.observation());
}

#[test]
fn outpost_capture_and_repair_replay_identically_and_do_not_advance_without_steps() {
    let mut a = SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 7);
    let mut b = SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 7);
    for frame in 0..650 {
        let observation = a.observation();
        let input = SurfaceSortieAction {
            interact_held: frame == 60,
            horizontal: if frame > 120
                && observation
                    .position
                    .distance_to(observation.outpost.position)
                    > 2.35
            {
                1.0
            } else {
                0.0
            },
            ..SurfaceSortieAction::default()
        };
        tick(&mut a, input);
        tick(&mut b, input);
        assert_eq!(a.observation(), b.observation(), "tick {frame}");
    }
    assert_eq!(a.outposts[0].owner, Some(a.pilot.owner));
    assert!(a.observation().outpost.repaired_health > 0.0);
    let before = a.observation();
    for _ in 0..10 {
        let _ = SurfaceSortieScenario::render_frame(&a);
        let _ = SurfaceSortieScenario::minimap_frame(&a, 16.0 / 9.0);
        let _ = SurfaceSortieScenario::observe(&a);
        SurfaceSortieScenario::step(&mut a, &[], Duration::ZERO);
    }
    assert_eq!(a.observation(), before);
    let fresh = SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 7).observation();
    assert_eq!(fresh.outpost.owner, None);
    assert_eq!(fresh.outpost.capture_progress, 0.0);
    assert_eq!(fresh.outpost.repaired_health, 0.0);
    assert_eq!(fresh.ship_health, 75.0);
}

#[test]
fn outpost_round_trip_captures_repairs_and_departs_using_only_player_controls() {
    for preset in [SurfaceMotionPreset::Stationary, SurfaceMotionPreset::Orbit] {
        round_trip(preset);
    }
}

fn round_trip(preset: SurfaceMotionPreset) {
    round_trip_state(SurfaceSortieScenario::init(preset, 7));
}

pub(super) fn round_trip_state(mut state: SurfaceSortieState) {
    let preset = state.motion_preset();
    idle(&mut state, 120);
    assert!(
        state.vehicle_settled(),
        "{preset:?}: {:?}",
        state.observation()
    );
    let initial = state.observation();
    assert_eq!(initial.ship_health, state.world.ships[0].life_max * 0.75);
    idle(&mut state, 360);
    assert_eq!(
        state.outposts[0].owner, None,
        "landing alone cannot capture"
    );
    assert_eq!(state.world.ships[0].life, initial.ship_health);
    disembark(&mut state);
    assert_eq!(
        state.outposts[0].owner, None,
        "must walk away from the hatch"
    );
    walk_to(&mut state, terminal_approach);
    idle(&mut state, 60);
    let partial = state.observation().outpost;
    assert!(partial.capture_progress > 0.2 && partial.capture_progress < 1.0);
    assert_eq!(partial.owner, None);
    assert_eq!(partial.repair_status, RepairStatus::NeedsCapture);
    capture(&mut state);
    assert!(state.vehicle_settled());
    assert_eq!(state.observation().outpost.captures, 1);
    assert_eq!(
        state.world.planets[state.pilot.planet].owner_id, None,
        "outpost ownership is not planet ownership"
    );
    assert!(state.world.spaceport_contacts.is_empty());
    assert_eq!(
        state.observation().physical_bodies,
        initial.physical_bodies + 1
    );
    for _ in 0..360 {
        let before = state.world.ships[0].life;
        idle(&mut state, 1);
        let after = state.world.ships[0].life;
        assert!(
            after >= before && after - before <= state.world.ships[0].life_max * 0.05 / 60.0 + 1e-5
        );
    }
    assert_eq!(state.world.ships[0].life, state.world.ships[0].life_max);
    let initial_damage = state.world.ships[0].life_max - initial.ship_health;
    assert!((state.observation().outpost.repaired_health - initial_damage).abs() < 1e-3);
    assert_eq!(
        state.observation().outpost.repair_status,
        RepairStatus::Ready
    );
    walk_to(&mut state, |s| {
        s.access_position() + s.access_up() * SurfaceSortieState::spec().half_height()
    });
    interact(&mut state);
    assert_eq!(
        state.last_transfer,
        TransferResult::Boarded,
        "{:?}",
        state.observation()
    );
    assert_eq!(state.observation().physical_bodies, initial.physical_bodies);
    assert_eq!(state.observation().spaceling, initial.spaceling);
    assert_eq!(state.observation().vehicle, initial.vehicle);
    idle(&mut state, 1); // Neutral transfer handoff.
    state.world.ships[0].life = 60.0;
    for _ in 0..30 {
        tick(
            &mut state,
            SurfaceSortieAction {
                primary_held: true,
                ..SurfaceSortieAction::default()
            },
        );
        assert_eq!(state.world.ships[0].life, 60.0, "no repair once taking off");
        assert_eq!(
            state.observation().outpost.repair_status,
            RepairStatus::NeedLanding
        );
    }
    assert!(state.landing.altitude > 1.0);
    assert_eq!(state.observation().outpost.owner, Some(initial.owner));
}

#[test]
fn interrupted_outpost_capture_resets_when_walking_away_jumping_or_knocked_down() {
    for interruption in 0..3 {
        let mut state = at_terminal();
        idle(&mut state, 60);
        assert!(state.observation().outpost.capture_progress > 0.2);
        match interruption {
            0 => {
                for _ in 0..40 {
                    tick(
                        &mut state,
                        SurfaceSortieAction {
                            horizontal: -1.0,
                            ..SurfaceSortieAction::default()
                        },
                    );
                }
                assert_eq!(
                    state.observation().outpost.capture_status,
                    CaptureStatus::TooFar
                );
            }
            1 => {
                tick(
                    &mut state,
                    SurfaceSortieAction {
                        primary_held: true,
                        ..SurfaceSortieAction::default()
                    },
                );
                assert!(!state.spaceling_snapshot().unwrap().grounded());
            }
            _ => {
                let snapshot = state.spaceling_snapshot().unwrap();
                let body = state.pilot.body.as_ref().unwrap().body();
                state.world.physics.world.set_velocity(
                    body,
                    snapshot.motion.linear_velocity,
                    10.0,
                    true,
                );
                idle(&mut state, 1);
                assert_eq!(
                    state.spaceling_snapshot().unwrap().balance,
                    SpacelingBalance::KnockedDown
                );
            }
        }
        let outpost = state.observation().outpost;
        assert_eq!(outpost.capture_progress, 0.0);
        assert_eq!(outpost.capturing_player, None);
        assert_eq!(outpost.owner, None);
        assert_eq!(outpost.repaired_health, 0.0);
    }
}

#[test]
fn unrelated_physical_support_near_the_terminal_does_not_capture() {
    let mut state = parked();
    state.world.planets[0].wrapper_omega = 0.0;
    let planet = state.world.planets[0];
    let position = terminal_approach(&state);
    let up = (position - planet.position).normalized();
    let platform = PhysicsId::new(45_123);
    let body_id = PhysicsBodyId::new(platform, BodyRole::PRIMARY);
    let shape = ColliderSpec::cuboid(
        ColliderId::new(platform, ColliderRole::PRIMARY, 0),
        0.7,
        0.2,
    );
    assert!(state.world.physics.world.insert_body(
        body_id,
        BodySpec {
            kind: engine_rapier::world::BodyKind::Fixed,
            position: position - up * 0.4,
            angle: rotation_for_direction(up),
            ..BodySpec::default()
        },
        &[shape]
    ));
    // Construct a supported actor on an unrelated platform inside the capture
    // radius. This isolates the surface-identity gate from the walk-up test.
    let spec = SurfaceSortieState::spec();
    state.pilot.body = SpacelingAssembly::insert(
        &mut state.world.physics.world,
        PILOT_PHYSICS_ID,
        position + up * 0.75,
        rotation_for_direction(up),
        spec,
    );
    assert!(state.pilot.body.is_some());
    idle(&mut state, 240);
    assert!(state.spaceling_snapshot().unwrap().grounded());
    assert_eq!(
        state.observation().outpost.capture_status,
        CaptureStatus::NeedSupport,
        "{:?}",
        state.observation()
    );
    assert_eq!(state.outposts[0].owner, None);
    state.world.physics.world.remove_entity(platform);
    idle(&mut state, 300);
    assert_eq!(
        state.outposts[0].owner,
        Some(state.pilot.owner),
        "actual planet support permits capture"
    );
}

#[test]
fn outpost_terminal_is_solid_and_rotates_with_its_planet_without_an_extra_body() {
    let mut state = parked();
    let bodies = state.world.physics.world.body_count();
    let colliders = state.world.physics.world.collider_count();
    let terminal_center = |s: &SurfaceSortieState| {
        let planet = &s.world.planets[0];
        s.outposts[0].position(planet)
            + s.outposts[0].up(planet) * physics::OUTPOST_TERMINAL_HALF_SIZE.y
    };
    let original = terminal_center(&state);
    let groups = SurfaceSortieState::spec().collision_groups;
    assert!(
        !state
            .world
            .physics
            .world
            .capsule_is_clear(original, 0.0, 0.1, 0.1, groups)
    );
    idle(&mut state, 600);
    assert!(terminal_center(&state).distance_to(original) > 5.0);
    assert!(!state.world.physics.world.capsule_is_clear(
        terminal_center(&state),
        0.0,
        0.1,
        0.1,
        groups
    ));
    assert!(
        state
            .world
            .physics
            .world
            .capsule_is_clear(original, 0.0, 0.1, 0.1, groups)
    );
    assert_eq!(state.world.physics.world.body_count(), bodies);
    assert_eq!(state.world.physics.world.collider_count(), colliders);
}

#[test]
fn outpost_repair_policy_requires_friendly_landed_in_range_and_live_ship() {
    let mut state = parked();
    let initial = state.world.ships[0].clone();
    let post = &mut state.outposts[0];
    let mut ship = initial.clone();
    post.repair_ship(&mut ship, true, 0.0, Duration::from_secs(1));
    assert_eq!(
        post.observation(&state.world.planets[0]).repair_status,
        RepairStatus::NeedsCapture
    );
    assert_eq!(ship.life, initial.life);
    post.owner = Some(PlayerId::PLAYER_2);
    post.repair_ship(&mut ship, true, 0.0, Duration::from_secs(1));
    assert_eq!(
        post.observation(&state.world.planets[0]).repair_status,
        RepairStatus::NotFriendly
    );
    post.owner = Some(PlayerId::PLAYER_1);
    for (landed, distance, expected) in [
        (false, 0.0, RepairStatus::NeedLanding),
        (true, 25.0, RepairStatus::OutOfRange),
    ] {
        post.repair_ship(&mut ship, landed, distance, Duration::from_secs(1));
        assert_eq!(
            post.observation(&state.world.planets[0]).repair_status,
            expected
        );
        assert_eq!(ship.life, initial.life);
    }
    for unavailable in 0..3 {
        let mut ship = initial.clone();
        match unavailable {
            0 => ship.dead = true,
            1 => ship.form = ShipForm::EscapePod,
            _ => ship.life = 0.0,
        }
        let before = ship.life;
        post.repair_ship(&mut ship, true, 0.0, Duration::from_secs(1));
        assert_eq!(
            post.observation(&state.world.planets[0]).repair_status,
            RepairStatus::VehicleUnavailable
        );
        assert_eq!(ship.life, before);
    }
    post.repair_ship(&mut ship, true, 0.0, Duration::from_secs(1));
    assert_eq!(ship.life, initial.life + initial.life_max * 0.05);
    ship.life = ship.life_max - 0.02;
    post.repair_ship(&mut ship, true, 0.0, Duration::from_secs(1));
    assert_eq!(ship.life, ship.life_max);
    let repaired = post.observation(&state.world.planets[0]).repaired_health;
    post.repair_ship(&mut ship, true, 0.0, Duration::from_secs(10));
    assert_eq!(
        post.observation(&state.world.planets[0]).repaired_health,
        repaired
    );
    assert_eq!(
        post.observation(&state.world.planets[0]).repair_status,
        RepairStatus::Ready
    );
}

#[test]
fn outpost_capture_uses_elapsed_time_and_never_shares_partial_progress_between_players() {
    let mut state = parked();
    let post = &mut state.outposts[0];
    post.update_capture(
        PlayerId::PLAYER_1,
        CaptureStatus::Capturing,
        Duration::from_secs(2),
    );
    assert_eq!(post.owner, None);
    post.update_capture(
        PlayerId::PLAYER_2,
        CaptureStatus::Capturing,
        Duration::from_secs(2),
    );
    assert_eq!(post.owner, None);
    post.update_capture(
        PlayerId::PLAYER_2,
        CaptureStatus::Capturing,
        Duration::from_secs(1),
    );
    assert_eq!(post.owner, Some(PlayerId::PLAYER_2));
    post.update_capture(
        PlayerId::PLAYER_2,
        CaptureStatus::Capturing,
        Duration::from_secs(10),
    );
    assert_eq!(post.observation(&state.world.planets[0]).captures, 1);
    post.update_capture(
        PlayerId::PLAYER_1,
        CaptureStatus::Capturing,
        Duration::from_secs(3),
    );
    assert_eq!(post.owner, Some(PlayerId::PLAYER_1));
    assert_eq!(post.observation(&state.world.planets[0]).captures, 2);
}
