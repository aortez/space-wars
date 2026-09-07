use super::*;

fn pair(preset: SurfaceMotionPreset) -> SurfaceSortieState {
    let mut world = SurfaceSortieScenario::init(SurfaceMotionPreset::Stationary, 7).world;
    let mut second = world.planets[0];
    second.position += Vec2::Y * 300.0;
    world.planets.push(second);
    world.rover_builds.push(RoverBuildState::default());
    let mut state = SurfaceSortieScenario::on_surface(world, preset, 0, Vec2::Y, -0.34);
    // Controlled route: both terminals sit beside the respective departure/
    // arrival spots. Only fixture construction writes poses or geometry.
    // The destination spins during the flight; put its terminal on the hatch
    // side of the arrival corridor, with room to walk without crossing the hull.
    let angle = -std::f32::consts::FRAC_PI_2 - second.wrapper_angle - 0.75;
    assert!(
        state
            .world
            .physics
            .insert_surface_terminal(1, second.radius, angle)
    );
    state
        .outposts
        .push(outpost::SurfaceOutpost::new(OutpostId(2), 1, angle));
    state.enable_travel();
    state
}

fn local_service(state: &mut SurfaceSortieState) {
    disembark(state);
    outpost_tests::walk_to(state, |s| {
        let post = s.focused_outpost();
        let planet = &s.world.planets[post.planet];
        let up = post
            .up(planet)
            .rotate_radians(2.0 / (planet.radius * BODY_BOUNDS_RADIUS_SCALE));
        planet.position
            + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE
                + SurfaceSortieState::spec().half_height())
    });
    for _ in 0..240 {
        if state.focused_outpost().owner == Some(state.pilot.owner) {
            break;
        }
        idle(state, 1);
    }
    assert_eq!(state.focused_outpost().owner, Some(state.pilot.owner));
    assert!(state.observation().outpost.repaired_health > 0.0);
    outpost_tests::walk_to(state, |s| {
        s.access_position() + s.access_up() * SurfaceSortieState::spec().half_height()
    });
    interact(state);
    assert_eq!(
        state.last_transfer,
        TransferResult::Boarded,
        "{:?}",
        state.observation()
    );
    idle(state, 1);
}

#[test]
fn journey_between_two_planets_uses_only_actions_after_setup() {
    for preset in [
        SurfaceMotionPreset::Stationary,
        SurfaceMotionPreset::Translating,
    ] {
        let mut state = pair(preset);
        let identity = state.observation();
        idle(&mut state, 120);
        assert_eq!(state.ship_support_planet(), Some(0));
        assert!(state.vehicle_settled());
        local_service(&mut state);
        for _ in 0..180 {
            tick(
                &mut state,
                SurfaceSortieAction {
                    primary_held: true,
                    ..SurfaceSortieAction::default()
                },
            );
            if state.landing.altitude > 32.0 {
                break;
            }
        }
        assert!(state.landing.altitude > 32.0);
        for _ in 0..600 {
            let ship = state.world.ships[0].position + SHIP_PIVOT;
            let midpoint =
                (state.world.planets[0].position.y + state.world.planets[1].position.y) * 0.5;
            if ship.y > midpoint + 25.0 {
                break;
            }
            idle(&mut state, 1);
        }
        eprintln!(
            "coast {preset:?}: landing={:?} ship={:?} velocity={:?}",
            state.landing, state.world.ships[0].position, state.world.ships[0].velocity
        );
        assert_eq!(state.landing.planet, Some(1));
        assert_eq!(state.ship_support_planet(), None);
        interact(&mut state);
        assert_eq!(state.last_transfer, TransferResult::ShipNotSettled);
        // Brake while turning rear-first toward B. This is a bounded scripted
        // input pilot, not a pose/velocity correction or a gameplay autopilot.
        for _ in 0..300 {
            let up = (state.world.ships[0].position + SHIP_PIVOT - state.world.planets[1].position)
                .normalized();
            let error = (rotation_for_direction(up) - state.world.ships[0].rotation_radians
                + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            if error.abs() < 0.025 && state.world.ships[0].omega.abs() < 0.12 {
                break;
            }
            tick(
                &mut state,
                SurfaceSortieAction {
                    horizontal: (-error * 3.0).clamp(-1.0, 1.0),
                    brake_held: true,
                    ..SurfaceSortieAction::default()
                },
            );
        }
        for _ in 0..900 {
            if state.vehicle_settled() {
                break;
            }
            idle(&mut state, 1);
        }
        assert!(state.vehicle_settled());
        assert_eq!(state.ship_support_planet(), Some(1));
        assert_eq!(state.outposts[0].owner, Some(state.pilot.owner));
        assert_eq!(state.outposts[1].owner, None);
        assert_eq!(
            state.observation().outposts[0].repair_status,
            RepairStatus::NeedLanding
        );
        local_service(&mut state);
        assert!(
            state
                .outposts
                .iter()
                .all(|post| post.owner == Some(state.pilot.owner))
        );
        assert!(
            state
                .world
                .planets
                .iter()
                .all(|planet| planet.owner_id.is_none())
        );
        assert_eq!(
            state.observation().physical_bodies,
            identity.physical_bodies
        );
        assert_eq!(state.observation().spaceling, identity.spaceling);
        assert_eq!(state.observation().vehicle, identity.vehicle);
        for _ in 0..60 {
            tick(
                &mut state,
                SurfaceSortieAction {
                    primary_held: true,
                    ..SurfaceSortieAction::default()
                },
            );
        }
        assert!(!state.vehicle_settled());
        assert!(state.landing.altitude > 10.0);
        assert_eq!(state.motion_metrics.ship_damage, 0.0);
        assert_eq!(state.transfers, 4);
        eprintln!(
            "journey {preset:?}: ticks={} transfers={} captured={} damage={:.1}",
            state.world.tick,
            state.transfers,
            state
                .outposts
                .iter()
                .filter(|post| post.owner.is_some())
                .count(),
            state.motion_metrics.ship_damage
        );
    }
}

#[test]
fn approach_selection_uses_surface_distance_and_hysteresis_without_granting_support() {
    let mut state = pair(SurfaceMotionPreset::Stationary);
    let body = state.world.physics.ship_body(0);
    for (y, expected) in [(650.4, 0), (652.0, 1), (649.6, 1), (648.0, 0)] {
        state
            .world
            .physics
            .world
            .set_pose(body, Vec2::new(500.0, y), 0.0, true);
        state
            .pilot
            .select_approach_planet(&state.world.physics, &state.world.planets);
        assert_eq!(state.pilot.planet, expected);
        assert_eq!(state.ship_support_planet(), None);
        assert_eq!(state.try_transfer(), TransferResult::ShipNotSettled);
    }
    // Closer center is not necessarily closer ground. Rebuild the fixture's
    // geometry before querying, rather than changing radii under live colliders.
    let mut world = state.world;
    world.planets[0].radius = 15.0;
    world.planets[1].radius = 120.0;
    let mut state = SurfaceSortieScenario::on_surface(
        world,
        SurfaceMotionPreset::Stationary,
        0,
        Vec2::Y,
        -0.34,
    );
    state.enable_travel();
    state
        .world
        .physics
        .world
        .set_pose(body, Vec2::new(500.0, 620.0), 0.0, true);
    state
        .pilot
        .select_approach_planet(&state.world.physics, &state.world.planets);
    assert_eq!(state.pilot.planet, 1);
}

#[test]
fn generated_arrivals_find_the_actual_planet_and_do_not_carry_over_settling_time() {
    for (seed, planet, bearing) in [(0, 3, 2), (1, 2, 1), (2, 5, 3)] {
        let mut state = compatibility::GeneratedSurfaceCase::new(seed, planet, bearing)
            .with_profile(GeneratedSurfaceProfile::SurfaceV1)
            .init()
            .unwrap();
        state.enable_travel();
        state.pilot.planet = 0; // Stale departure frame, not the physical start.
        idle(&mut state, 120);
        assert!(state.vehicle_settled(), "{:?}", state.observation());
        assert_eq!(state.pilot.planet, planet);
        assert_eq!(state.ship_support_planet(), Some(planet));
        assert_eq!(state.focused_outpost().planet, planet);
        let before = state.landing;
        state.landing.planet = Some(0);
        state.landing.update(
            &state.world.physics,
            0,
            planet,
            &state.world.planets[planet],
            &state.world.ships[0],
            1.0 / 60.0,
        );
        assert_eq!(state.landing.supported_feet, 2);
        assert_eq!(state.landing.settled_seconds, 1.0 / 60.0);
        assert_ne!(state.landing.phase, LandingPhase::Landed);
        state.landing = before;
        disembark(&mut state);
        assert_eq!(state.pilot_support_planet(), Some(planet));
        assert_eq!(state.motion_observation().planet, planet);
        interact(&mut state);
        assert_eq!(state.last_transfer, TransferResult::Boarded);
    }
}

#[test]
fn nearby_friendly_outpost_cannot_repair_a_ship_landed_on_a_different_planet() {
    let mut state = pair(SurfaceMotionPreset::Stationary);
    idle(&mut state, 120);
    assert_eq!(state.ship_support_planet(), Some(0));
    state.outposts[1].owner = Some(state.pilot.owner);
    // Isolate the service policy with a foreign site at zero distance. There
    // is deliberately no physics step after this policy-only geometry setup.
    let ship = state.world.ships[0].position + SHIP_PIVOT;
    let up = state.outposts[1].up(&state.world.planets[1]);
    state.world.planets[1].position =
        ship - up * (state.world.planets[1].radius * BODY_BOUNDS_RADIUS_SCALE);
    let health = state.world.ships[0].life;
    state.update_outpost(Duration::from_secs(1));
    assert_eq!(state.world.ships[0].life, health);
    assert_eq!(
        state.observation().outposts[1].repair_status,
        RepairStatus::NeedLanding
    );
    assert_eq!(state.observation().outposts[1].repaired_health, 0.0);
}

#[test]
fn boarding_requires_support_from_the_landed_planet_not_a_nearby_platform() {
    let mut state = pair(SurfaceMotionPreset::Stationary);
    state.world.planets[0].wrapper_omega = 0.0;
    idle(&mut state, 120);
    assert!(state.vehicle_settled());
    let up = state.access_up();
    let hatch = state.access_position();
    let platform = PhysicsId::new(45_123);
    let body = PhysicsBodyId::new(platform, BodyRole::PRIMARY);
    assert!(state.world.physics.world.insert_body(
        body,
        BodySpec {
            kind: engine_rapier::world::BodyKind::Fixed,
            position: hatch + up * 0.5,
            angle: rotation_for_direction(up),
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(platform, ColliderRole::PRIMARY, 0),
            0.7,
            0.2
        )]
    ));
    state.pilot.body = SpacelingAssembly::insert(
        &mut state.world.physics.world,
        PILOT_PHYSICS_ID,
        hatch + up * 1.65,
        rotation_for_direction(up),
        SurfaceSortieState::spec(),
    );
    idle(&mut state, 60);
    let snapshot = state.spaceling_snapshot().unwrap();
    assert!(snapshot.grounded());
    assert_eq!(snapshot.balance, SpacelingBalance::Balanced);
    assert!(
        snapshot
            .motion
            .position
            .distance_to(state.access_position())
            < BOARDING_RANGE
    );
    assert_eq!(state.pilot_support_planet(), None);
    assert_eq!(state.try_transfer(), TransferResult::MustBeSupported);
    state.world.physics.world.remove_entity(platform);
    idle(&mut state, 120);
    assert_eq!(state.pilot_support_planet(), Some(0));
    assert_eq!(state.try_transfer(), TransferResult::Boarded);
}

#[test]
fn expedition_replays_restarts_and_adds_only_one_site_collider_per_planet() {
    let mut a = SurfaceSortieScenario::init_expedition(0);
    let mut b = SurfaceSortieScenario::init_expedition(0);
    let baseline = SurfaceSortieScenario::init(SurfaceMotionPreset::GeneratedSurfaceV1, 0);
    assert!(!baseline.travel_enabled());
    assert!(a.travel_enabled());
    assert_eq!(a.world.planets, baseline.world.planets);
    assert_eq!(
        a.world.physics.world.body_count(),
        baseline.world.physics.world.body_count()
    );
    assert_eq!(
        a.world.physics.world.collider_count(),
        baseline.world.physics.world.collider_count() + a.world.planets.len() - 1
    );
    assert_eq!(a.outposts.len(), a.world.planets.len());
    for (index, post) in a.outposts.iter().enumerate() {
        assert_eq!(post.planet, index);
        assert_eq!(post.id, OutpostId(index as u64 + 1));
    }
    for frame in 0..650 {
        let input = SurfaceSortieAction {
            interact_held: frame == 60,
            horizontal: if frame > 120
                && a.observation()
                    .position
                    .distance_to(a.observation().outpost.position)
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
        assert_eq!(
            SurfaceSortieScenario::observe(&a).payload,
            SurfaceSortieScenario::observe(&b).payload
        );
    }
    assert_eq!(a.outposts[0].owner, Some(a.pilot.owner));
    assert!(a.outposts[1..].iter().all(|post| post.owner.is_none()));
    let fresh = SurfaceSortieScenario::init_expedition(0);
    assert!(fresh.outposts.iter().all(|post| post.owner.is_none()));
    assert_eq!(fresh.observation().transfers, 0);
    assert_eq!(fresh.observation().ship_health, 75.0);
    let before = a.observation();
    SurfaceSortieScenario::render_frame(&a);
    SurfaceSortieScenario::minimap_frame(&a, 16.0 / 9.0);
    SurfaceSortieScenario::step(&mut a, &[], Duration::ZERO);
    assert_eq!(a.observation(), before);
}

#[test]
fn airborne_pilot_focus_does_not_follow_the_parked_ships_planet() {
    let mut state = pair(SurfaceMotionPreset::Stationary);
    let position = state.world.planets[1].position + Vec2::Y * 90.0;
    state.pilot.body = SpacelingAssembly::insert(
        &mut state.world.physics.world,
        PILOT_PHYSICS_ID,
        position,
        0.0,
        SurfaceSortieState::spec(),
    );
    assert_eq!(state.pilot_support_planet(), None);
    assert_eq!(state.pilot.planet, 0);
    assert_eq!(state.motion_observation().planet, 1);
    assert_eq!(state.focused_outpost().planet, 1);
}
