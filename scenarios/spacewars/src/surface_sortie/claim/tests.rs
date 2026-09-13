use super::*;
use engine_rapier::world::{
    BodyId as PhysicsBodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
};

const P1: PlayerId = PlayerId::PLAYER_1;
const P2: PlayerId = PlayerId::PLAYER_2;
const NEUTRAL: SurfaceSortieAction = SurfaceSortieAction {
    horizontal: 0.0,
    primary_held: false,
    interact_held: false,
    brake_held: false,
};
const INTERACT: SurfaceSortieAction = SurfaceSortieAction {
    interact_held: true,
    ..NEUTRAL
};

fn frame() -> motion::SurfaceFrame {
    motion::SurfaceFrame {
        position: Vec2::new(40.0, -20.0),
        angle: 0.4,
        linear_velocity: Vec2::new(3.0, 2.0),
        angular_velocity: 0.05,
    }
}

fn standing(player: PlayerId, x: f32) -> Claimant {
    Claimant {
        player,
        planet: Some(0),
        status: PlanetClaimStatus::Ready,
        anchor: Some(FlagAnchor {
            surface_revision: 0,
            footing: None,
            position: Vec2::new(x, 10.0),
            normal: Vec2::Y,
        }),
    }
}

#[test]
fn claim_requires_full_raise_then_nearby_lowering_before_a_fresh_raise() {
    let mut claim = SurfacePlanetClaim::new(0);
    let mut owner = None;
    claim.update(
        &mut owner,
        frame(),
        &[standing(P1, 7.0)],
        Duration::from_secs(2),
    );
    assert_eq!(owner, None);
    let partial = claim.observation(owner, frame(), 0);
    assert_eq!(partial.phase, PlanetClaimPhase::Raising);
    assert!((partial.flag.unwrap().raised_fraction - 2.0 / 3.0).abs() < 1e-6);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P1, 7.0)],
        Duration::from_secs(1),
    );
    assert_eq!(owner, Some(0));
    let original = claim.observation(owner, frame(), 0).flag.unwrap();
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 20.0)],
        Duration::from_secs(20),
    );
    assert_eq!(owner, Some(0));
    assert_eq!(
        claim.observation(owner, frame(), 1).status,
        PlanetClaimStatus::ApproachFlag
    );
    assert_eq!(claim.observation(owner, frame(), 1).flag.unwrap(), original);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 8.0)],
        Duration::from_secs(1),
    );
    assert_eq!(owner, Some(0));
    let lowering = claim.observation(owner, frame(), 1);
    assert_eq!(lowering.phase, PlanetClaimPhase::Lowering);
    assert!((lowering.flag.unwrap().raised_fraction - 2.0 / 3.0).abs() < 1e-6);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 8.0)],
        Duration::from_secs(2),
    );
    assert_eq!(owner, None);
    let lowered = claim.observation(owner, frame(), 1);
    assert_eq!(lowered.phase, PlanetClaimPhase::Raising);
    assert_eq!(lowered.progress, 0.0);
    assert_eq!(lowered.flag.unwrap().player, P2);
    assert_eq!(lowered.flag.unwrap().raised_fraction, 0.0);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 8.0)],
        Duration::from_secs(2),
    );
    assert_eq!(owner, None, "lowering time must not count toward raising");
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 8.0)],
        Duration::from_secs(1),
    );
    assert_eq!(owner, Some(1));
    assert_eq!(claim.captures, 2);
    assert_eq!(claim.neutralizations, 1);
    assert_eq!(
        claim
            .observation(owner, frame(), 1)
            .flag
            .unwrap()
            .raised_fraction,
        1.0
    );
}

#[test]
fn interruptions_reset_only_the_unfinished_stage_and_never_borrow_another_claim() {
    let mut claim = SurfacePlanetClaim::new(0);
    let mut owner = None;
    claim.update(&mut owner, frame(), &[standing(P1, 0.0)], FLAG_STAGE_TIME);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 1.0)],
        Duration::from_secs(2),
    );
    let mut airborne = standing(P2, 1.0);
    airborne.planet = None;
    airborne.anchor = None;
    airborne.status = PlanetClaimStatus::NeedSupport;
    claim.update(&mut owner, frame(), &[airborne], Duration::from_secs(10));
    assert_eq!(owner, Some(0));
    let reset = claim.observation(owner, frame(), 1);
    assert_eq!(reset.phase, PlanetClaimPhase::Idle);
    assert_eq!(reset.status, PlanetClaimStatus::NeedSupport);
    assert_eq!(reset.flag.unwrap().raised_fraction, 1.0);
    claim.update(&mut owner, frame(), &[standing(P2, 1.0)], FLAG_STAGE_TIME);
    assert_eq!(owner, None);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 1.0)],
        Duration::from_secs(2),
    );
    claim.update(&mut owner, frame(), &[airborne], Duration::from_secs(1));
    assert_eq!(
        owner, None,
        "completed neutralization survives an interrupted raise"
    );
    assert!(claim.flag.is_none());
    claim.update(
        &mut owner,
        frame(),
        &[standing(P1, -9.0)],
        Duration::from_secs(1),
    );
    assert_eq!(owner, None);
    assert_eq!(claim.observation(owner, frame(), 0).progress, 1.0 / 3.0);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P1, -9.0)],
        Duration::from_secs(2),
    );
    assert_eq!(owner, Some(0));
    assert_eq!(claim.captures, 2);
}

#[test]
fn contests_are_local_to_existing_flags_and_independent_of_seat_order() {
    let mut a = SurfacePlanetClaim::new(0);
    let mut b = SurfacePlanetClaim::new(0);
    let mut owner_a = None;
    let mut owner_b = None;
    for (claim, owner) in [(&mut a, &mut owner_a), (&mut b, &mut owner_b)] {
        claim.update(owner, frame(), &[standing(P1, 0.0)], FLAG_STAGE_TIME);
        claim.update(owner, frame(), &[standing(P2, 1.0)], Duration::from_secs(1));
    }
    a.update(
        &mut owner_a,
        frame(),
        &[standing(P1, -1.0), standing(P2, 1.0)],
        Duration::from_secs(20),
    );
    b.update(
        &mut owner_b,
        frame(),
        &[standing(P2, 1.0), standing(P1, -1.0)],
        Duration::from_secs(20),
    );
    for player in 0..2 {
        assert_eq!(
            a.observation(owner_a, frame(), player),
            b.observation(owner_b, frame(), player)
        );
        assert_eq!(
            a.observation(owner_a, frame(), player).status,
            PlanetClaimStatus::Contested
        );
    }
    assert_eq!(a.observation(owner_a, frame(), 1).progress, 1.0 / 3.0);
    a.update(
        &mut owner_a,
        frame(),
        &[standing(P1, 20.0), standing(P2, 1.0)],
        Duration::from_secs(2),
    );
    assert_eq!(
        owner_a, None,
        "a distant defender cannot contest the old flag"
    );
    a.update(
        &mut owner_a,
        frame(),
        &[standing(P1, 20.0), standing(P2, 1.0)],
        FLAG_STAGE_TIME,
    );
    assert_eq!(
        owner_a,
        Some(1),
        "a distant opponent cannot contest the new flag either"
    );
}

#[test]
fn simultaneous_neutral_claimants_do_not_win_by_iteration_order() {
    let mut claim = SurfacePlanetClaim::new(0);
    let mut owner = None;
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 10.0), standing(P1, -10.0)],
        Duration::from_secs(20),
    );
    assert_eq!(owner, None);
    assert!(claim.flag.is_none());
    assert_eq!(claim.statuses, [PlanetClaimStatus::Contested; 2]);
    claim.update(&mut owner, frame(), &[standing(P1, -10.0)], FLAG_STAGE_TIME);
    assert_eq!(owner, Some(0));
}

#[test]
fn flags_keep_the_exact_contact_anchor_as_the_planet_rotates_and_translates() {
    let mut claim = SurfacePlanetClaim::new(0);
    let mut owner = None;
    let candidate = standing(P1, 17.0);
    claim.update(&mut owner, frame(), &[candidate], FLAG_STAGE_TIME);
    let moved = motion::SurfaceFrame {
        position: Vec2::new(-100.0, 300.0),
        angle: 1.9,
        ..frame()
    };
    let flag = claim.observation(owner, moved, 0).flag.unwrap();
    let anchor = candidate.anchor.unwrap();
    assert_eq!(
        flag.position,
        moved.position + anchor.position.rotate_radians(moved.angle)
    );
    assert_eq!(flag.normal, anchor.normal.rotate_radians(moved.angle));
    assert_eq!(owner, Some(0));
}

#[test]
fn nearby_support_on_another_planet_cannot_lower_a_flag() {
    let mut claim = SurfacePlanetClaim::new(0);
    let mut owner = None;
    claim.update(&mut owner, frame(), &[standing(P1, 0.0)], FLAG_STAGE_TIME);
    let mut foreign = standing(P2, 0.0);
    foreign.planet = Some(1);
    claim.update(&mut owner, frame(), &[foreign], Duration::from_secs(30));
    assert_eq!(owner, Some(0));
    assert_eq!(claim.statuses[1], PlanetClaimStatus::Elsewhere);
    assert!(claim.progress.is_none());
    // Verify both sides of the declared range using the same completed frame.
    claim.update(&mut owner, frame(), &[standing(P2, 3.01)], FLAG_STAGE_TIME);
    assert_eq!(owner, Some(0));
    assert_eq!(claim.statuses[1], PlanetClaimStatus::ApproachFlag);
    claim.update(&mut owner, frame(), &[standing(P2, 2.99)], FLAG_STAGE_TIME);
    assert_eq!(owner, None);
    assert_eq!(claim.neutralizations, 1);
}

fn step(state: &mut SurfaceSortieState, inputs: [SurfaceSortieAction; 2]) {
    SurfaceSortieScenario::step(
        state,
        &[inputs[0].encode(P1), inputs[1].encode(P2)],
        Duration::from_secs_f64(1.0 / 60.0),
    );
}

fn idle(state: &mut SurfaceSortieState, ticks: usize) {
    for _ in 0..ticks {
        step(state, [NEUTRAL; 2]);
    }
}

fn fixture(preset: SurfaceMotionPreset, bearing: f32, players: usize) -> SurfaceSortieState {
    let world = SurfaceSortieScenario::init(preset, 7).world;
    let up = Vec2::Y.rotate_radians(bearing);
    let mut state = SurfaceSortieScenario::on_surface(world, preset, 0, up, None);
    state.enable_planet_claims();
    state.world.ships[0].life = state.world.ships[0].life_max;
    if players == 2 {
        let planet = state.world.planets[0];
        let up = -up;
        let center = planet.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 5.5);
        let ship = &mut state.world.ships[1];
        ship.position = center - SHIP_PIVOT;
        ship.rotation_radians = rotation_for_direction(up);
        ship.direction = up;
        ship.velocity = preset.initial_velocity(&planet)
            + Vec2::new(-(center - planet.position).y, (center - planet.position).x)
                * planet.wrapper_omega;
        ship.life = ship.life_max;
        state.pilots.push(SurfacePilot::new(P2, 0, true));
        state
            .world
            .physics
            .enable_surface_sortie(&[0, 1], &state.world.ships);
    }
    idle(&mut state, 120);
    state
}

#[test]
fn landing_disembarking_and_waiting_claims_without_a_terminal_on_moving_planets() {
    for preset in [
        SurfaceMotionPreset::Stationary,
        SurfaceMotionPreset::Translating,
        SurfaceMotionPreset::Orbit,
    ] {
        for bearing in 0..4 {
            let mut state = fixture(preset, bearing as f32 * std::f32::consts::FRAC_PI_2, 1);
            let initial = state.observation(0);
            assert!(
                state.vehicle_settled(0),
                "{preset:?}/{bearing}: {initial:?}"
            );
            assert!(initial.outpost.is_none());
            assert!(state.outposts.is_empty());
            assert_eq!(
                state.world.planets[0].owner_id, None,
                "landing alone cannot claim"
            );
            let colliders = state.world.physics.world.collider_count();
            step(&mut state, [INTERACT, NEUTRAL]);
            assert_eq!(state.location(0), PilotLocation::OnFoot);
            let mut anchor = None;
            for _ in 0..360 {
                idle(&mut state, 1);
                let claim = state.claim_observation(0, 0).unwrap();
                if anchor.is_none() && claim.phase == PlanetClaimPhase::Raising {
                    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
                    let support = state.spaceling_snapshot(0).unwrap().support.unwrap();
                    let current_contact =
                        frame.position + support.local_surface.position.rotate_radians(frame.angle);
                    assert!(claim.flag.unwrap().position.distance_to(current_contact) < 0.02);
                    anchor = Some(state.claims[0].flag.unwrap().anchor);
                    assert!((claim.flag.unwrap().position - frame.position).length() > 1.0);
                }
                if claim.owner == Some(P1) {
                    break;
                }
            }
            assert_eq!(
                state.world.planets[0].owner_id,
                Some(0),
                "{preset:?}/{bearing}: {:?}",
                state.observation(0)
            );
            assert_eq!(state.claims[0].flag.unwrap().anchor, anchor.unwrap());
            assert_eq!(state.claims[0].captures, 1);
            assert_eq!(
                state.world.physics.world.body_count(),
                initial.physical_bodies + 1
            );
            assert_eq!(state.world.physics.world.collider_count(), colliders + 1);
            assert_eq!(
                state.observation(0).ship_health,
                initial.ship_health,
                "claiming is not repair"
            );
            step(&mut state, [INTERACT, NEUTRAL]);
            assert_eq!(state.location(0), PilotLocation::Aboard(VehicleId(0)));
            idle(&mut state, 60);
            assert_eq!(
                state.world.physics.world.body_count(),
                initial.physical_bodies
            );
            assert_eq!(state.world.physics.world.collider_count(), colliders);
            assert_eq!(state.claims[0].flag.unwrap().anchor, anchor.unwrap());
            assert_eq!(state.world.planets[0].owner_id, Some(0));
        }
    }
}

#[test]
fn another_landing_cannot_lower_the_flag_until_the_pilot_reaches_it() {
    let mut state = fixture(SurfaceMotionPreset::Orbit, 0.0, 2);
    assert!(state.vehicle_settled(0) && state.vehicle_settled(1));
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 240);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 1);
    step(&mut state, [NEUTRAL, INTERACT]);
    idle(&mut state, 240);
    let old_flag = state.claim_observation(0, 1).unwrap().flag.unwrap();
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    assert_eq!(
        state.claim_observation(0, 1).unwrap().status,
        PlanetClaimStatus::ApproachFlag
    );
    assert_eq!(old_flag.raised_fraction, 1.0);
    // Arrange arrival near the existing flag, then let real Rapier contact,
    // balance and relative speed gates drive every stage of the takeover.
    let planet = state.world.planets[0];
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let up = old_flag
        .normal
        .rotate_radians(-2.0 / (planet.radius * BODY_BOUNDS_RADIUS_SCALE));
    let position = frame.position
        + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE
            + SurfaceSortieState::spec().half_height()
            + 0.1);
    let body = state.pilots[1].body.as_ref().unwrap().body();
    state
        .world
        .physics
        .world
        .set_pose(body, position, rotation_for_direction(up), true);
    state.world.physics.world.set_velocity(
        body,
        motion::point_velocity(frame, position),
        frame.angular_velocity,
        true,
    );
    let mut saw_lowering = false;
    let mut saw_neutral = false;
    for _ in 0..480 {
        idle(&mut state, 1);
        let claim = state.claim_observation(0, 1).unwrap();
        match claim.phase {
            PlanetClaimPhase::Lowering => {
                saw_lowering = true;
                assert_eq!(claim.owner, Some(P1));
                assert_eq!(claim.flag.unwrap().player, P1);
            }
            PlanetClaimPhase::Raising => {
                saw_neutral = true;
                assert!(saw_lowering);
                assert_eq!(claim.owner, None);
                assert_eq!(claim.flag.unwrap().player, P2);
            }
            PlanetClaimPhase::Idle => {}
        }
        if claim.owner == Some(P2) {
            break;
        }
    }
    assert!(saw_lowering && saw_neutral);
    assert_eq!(
        state.world.planets[0].owner_id,
        Some(1),
        "{:?}",
        state.observation(1)
    );
    assert_eq!(state.claims[0].captures, 2);
    assert_eq!(state.claims[0].neutralizations, 1);
}

#[test]
fn walking_jumping_and_knockdown_interrupt_a_physically_started_claim() {
    for interruption in 0..3 {
        let mut state = fixture(SurfaceMotionPreset::Stationary, 0.0, 1);
        step(&mut state, [INTERACT, NEUTRAL]);
        idle(&mut state, 80);
        let partial = state.claim_observation(0, 0).unwrap();
        assert_eq!(partial.phase, PlanetClaimPhase::Raising);
        assert!(partial.progress > 0.2 && partial.progress < 1.0);
        match interruption {
            0 => {
                for _ in 0..20 {
                    step(
                        &mut state,
                        [
                            SurfaceSortieAction {
                                horizontal: 1.0,
                                ..NEUTRAL
                            },
                            NEUTRAL,
                        ],
                    );
                }
                assert_eq!(
                    state.claim_candidate(0).status,
                    PlanetClaimStatus::NeedSettle
                );
            }
            1 => {
                step(
                    &mut state,
                    [
                        SurfaceSortieAction {
                            primary_held: true,
                            ..NEUTRAL
                        },
                        NEUTRAL,
                    ],
                );
                assert!(!state.spaceling_snapshot(0).unwrap().grounded());
            }
            _ => {
                let snapshot = state.spaceling_snapshot(0).unwrap();
                let body = state.pilots[0].body.as_ref().unwrap().body();
                state.world.physics.world.set_velocity(
                    body,
                    snapshot.motion.linear_velocity,
                    10.0,
                    true,
                );
                idle(&mut state, 1);
                assert_eq!(
                    state.spaceling_snapshot(0).unwrap().balance,
                    SpacelingBalance::KnockedDown
                );
            }
        }
        let claim = state.claim_observation(0, 0).unwrap();
        assert_eq!(claim.owner, None);
        assert_eq!(claim.phase, PlanetClaimPhase::Idle);
        assert_eq!(claim.progress, 0.0);
        assert!(claim.flag.is_none());
    }
}

#[test]
fn standing_on_an_unrelated_platform_is_not_planetary_support() {
    let mut state = fixture(SurfaceMotionPreset::Stationary, 0.0, 1);
    state.world.planets[0].wrapper_omega = 0.0;
    let up = state.access_up(0);
    let hatch = state.access_position(0);
    let platform = PhysicsId::new(45_123);
    assert!(state.world.physics.world.insert_body(
        PhysicsBodyId::new(platform, BodyRole::PRIMARY),
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
        )],
    ));
    state.pilots[0].body = SpacelingAssembly::insert(
        &mut state.world.physics.world,
        pilot_physics_id(P1),
        hatch + up * 1.65,
        rotation_for_direction(up),
        SurfaceSortieState::spec(),
    );
    idle(&mut state, 240);
    let snapshot = state.spaceling_snapshot(0).unwrap();
    assert!(snapshot.grounded());
    assert_eq!(snapshot.balance, SpacelingBalance::Balanced);
    assert_eq!(
        state.claim_candidate(0).status,
        PlanetClaimStatus::NeedSupport
    );
    assert_eq!(state.world.planets[0].owner_id, None);
    assert!(state.claims[0].flag.is_none());
    state.world.physics.world.remove_entity(platform);
    idle(&mut state, 300);
    assert_eq!(
        state.world.planets[0].owner_id,
        Some(0),
        "real planetary support enables claiming"
    );
}
