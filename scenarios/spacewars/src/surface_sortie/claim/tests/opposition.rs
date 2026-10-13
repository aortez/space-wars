use super::*;
use crate::surface_sortie::surface_forces::{
    GroundSurfaceForce, SurfaceAllegiance, SurfaceForceId,
};

fn blocked(player: usize) -> [SurfaceOpposition; 2] {
    let mut result = [SurfaceOpposition::default(); 2];
    result[player].ground_units = 1;
    result
}

#[test]
fn opposition_on_the_completion_step_pauses_both_stages_and_cancellation_still_wins() {
    for lowering in [false, true] {
        let mut claim = SurfacePlanetClaim::new(0);
        let mut owner = None;
        if lowering {
            claim.update(
                &mut owner,
                frame(),
                &[standing(P2, 0.0)],
                Default::default(),
                FLAG_STAGE_TIME,
            );
        }
        let contender = standing(P1, 1.0);
        claim.update(
            &mut owner,
            frame(),
            &[contender],
            Default::default(),
            FLAG_STAGE_TIME - Duration::from_millis(1),
        );
        let before = claim.observation(owner, frame(), 0);
        for _ in 0..5 {
            claim.update(
                &mut owner,
                frame(),
                &[contender],
                blocked(0),
                Duration::from_secs(10),
            );
            let now = claim.observation(owner, frame(), 0);
            assert_eq!(now.owner, before.owner);
            assert_eq!(now.phase, before.phase);
            assert_eq!(now.progress, before.progress);
            assert_eq!(now.flag, before.flag);
            assert_eq!(now.status, PlanetClaimStatus::OpposingSurfaceForces);
            assert_eq!(now.opposition.ground_units, 1);
        }
        let mut cancelled = claim.clone();
        let mut invalid = contender;
        invalid.status = PlanetClaimStatus::NeedBalance;
        let mut cancelled_owner = owner;
        cancelled.update(
            &mut cancelled_owner,
            frame(),
            &[invalid],
            blocked(0),
            Duration::from_secs(1),
        );
        assert_eq!(
            cancelled.observation(cancelled_owner, frame(), 0).phase,
            PlanetClaimPhase::Idle
        );
        assert_eq!(
            cancelled.observation(cancelled_owner, frame(), 0).progress,
            0.0
        );
        claim.update(
            &mut owner,
            frame(),
            &[contender],
            Default::default(),
            Duration::from_millis(1),
        );
        assert_eq!(owner, if lowering { None } else { Some(0) });
        assert_eq!(
            claim.observation(owner, frame(), 0).phase,
            if lowering {
                PlanetClaimPhase::Raising
            } else {
                PlanetClaimPhase::Idle
            }
        );
    }
}

#[test]
fn a_blocked_new_claimant_cannot_inherit_progress_or_create_a_flag() {
    let mut claim = SurfacePlanetClaim::new(0);
    let mut owner = None;
    claim.update(
        &mut owner,
        frame(),
        &[standing(P1, 0.0)],
        Default::default(),
        Duration::from_secs(2),
    );
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 1.0)],
        blocked(1),
        Duration::from_secs(20),
    );
    assert!(claim.progress.is_none());
    assert!(claim.flag.is_none());
    assert_eq!(owner, None);
    assert_eq!(claim.statuses[1], PlanetClaimStatus::OpposingSurfaceForces);
    claim.update(
        &mut owner,
        frame(),
        &[standing(P2, 1.0)],
        Default::default(),
        Duration::from_secs(1),
    );
    assert_eq!(claim.observation(owner, frame(), 1).progress, 1.0 / 3.0);
}

#[test]
fn a_landed_enemy_on_the_far_side_blocks_and_real_takeoff_clears_it() {
    for preset in [SurfaceMotionPreset::Stationary, SurfaceMotionPreset::Orbit] {
        let mut state = fixture(preset, 0.7, 2);
        step(&mut state, [INTERACT, NEUTRAL]);
        idle(&mut state, 240);
        let claim = state.claim_observation(0, 0).unwrap();
        assert_eq!(claim.status, PlanetClaimStatus::OpposingSurfaceForces);
        assert_eq!(
            claim.opposition,
            SurfaceOpposition {
                ships: 1,
                ..Default::default()
            }
        );
        assert!(claim.flag.is_none());
        assert_eq!(claim.owner, None);
        let flying = SurfaceSortieAction {
            primary_held: true,
            ..NEUTRAL
        };
        step(&mut state, [NEUTRAL, flying]);
        assert!(
            state.claim_observation(0, 0).unwrap().opposition.is_empty(),
            "takeoff clears at this step"
        );
        for _ in 0..240 {
            step(&mut state, [NEUTRAL, flying]);
        }
        assert_eq!(state.world.planets[0].owner_id, Some(0));
        assert!(state.ship_support_planet(1).is_none());
        assert_eq!(state.location(1), PilotLocation::Aboard(VehicleId(1)));
    }
}

#[test]
fn disembarked_defender_counts_when_unbalanced_and_boarding_does_not_change_ship_allegiance() {
    let mut state = fixture(SurfaceMotionPreset::Stationary, 0.0, 2);
    step(&mut state, [INTERACT, INTERACT]);
    idle(&mut state, 180);
    assert_eq!(
        state.claim_observation(0, 0).unwrap().opposition,
        SurfaceOpposition {
            pilots: 1,
            ships: 1,
            ground_units: 0
        }
    );
    let actor = state.pilots[1].body.as_ref().unwrap().body();
    let snapshot = state.spaceling_snapshot(1).unwrap();
    state.world.physics.world.set_velocity(
        actor,
        snapshot.motion.linear_velocity,
        SurfaceSortieState::spec().balance.knockdown_angular_speed * 1.25,
        true,
    );
    idle(&mut state, 1);
    assert_eq!(
        state.spaceling_snapshot(1).unwrap().balance,
        SpacelingBalance::KnockedDown
    );
    assert_eq!(state.claim_observation(0, 0).unwrap().opposition.pilots, 1);
    assert_eq!(state.world.planets[0].owner_id, None);
    // Keep the corpse body to prove that physics alone does not imply a live force.
    state.pilots[1].vitals = Some(match_rules::PilotVitals {
        death_tick: Some(state.world.tick),
        ..Default::default()
    });
    state.update_planet_claims(Duration::from_secs_f32(1.0 / 60.0));
    assert_eq!(
        state.claim_observation(0, 0).unwrap().opposition,
        SurfaceOpposition {
            ships: 1,
            ..Default::default()
        }
    );
    assert!(state.world.physics.world.motion(actor).is_some());
    assert!(state.claim_candidate(1).anchor.is_none());

    let mut boarded = fixture(SurfaceMotionPreset::Stationary, 0.0, 2);
    step(&mut boarded, [INTERACT, INTERACT]);
    idle(&mut boarded, 180);
    step(&mut boarded, [NEUTRAL, INTERACT]);
    assert_eq!(boarded.location(1), PilotLocation::Aboard(VehicleId(1)));
    idle(&mut boarded, 240);
    assert_eq!(boarded.world.planets[0].owner_id, None);
    assert_eq!(
        boarded.claim_observation(0, 0).unwrap().opposition,
        SurfaceOpposition {
            ships: 1,
            ..Default::default()
        }
    );
}

#[test]
fn a_native_pilot_hop_keeps_opposition_and_clones_its_contact_history() {
    let mut state = fixture(SurfaceMotionPreset::Stationary, 0.0, 2);
    step(&mut state, [INTERACT, INTERACT]);
    idle(&mut state, 180);
    step(
        &mut state,
        [
            NEUTRAL,
            SurfaceSortieAction {
                primary_held: true,
                ..NEUTRAL
            },
        ],
    );
    let mut replay = state.clone();
    let mut missed_contact = false;
    let mut landed = false;
    for tick in 0..180 {
        idle(&mut state, 1);
        idle(&mut replay, 1);
        assert_eq!(
            SurfaceSortieScenario::observe(&state).payload,
            SurfaceSortieScenario::observe(&replay).payload
        );
        let supported = state.spaceling_snapshot(1).unwrap().support.is_some();
        missed_contact |= !supported;
        assert_eq!(
            state.claim_observation(0, 0).unwrap().opposition.pilots,
            1,
            "hop tick {tick}: {:?}",
            state.surface_force_observations()
        );
        if missed_contact && supported {
            landed = true;
            break;
        }
    }
    assert!(
        missed_contact && landed,
        "ordinary jump must leave and regain native support"
    );
    assert_eq!(state.world.planets[0].owner_id, None);
}

fn add_ground_force(state: &mut SurfaceSortieState, serial: u64) -> GroundSurfaceForce {
    let id = PhysicsId::new(70_000 + serial);
    let body = PhysicsBodyId::new(id, BodyRole::PRIMARY);
    let collider = ColliderId::new(id, ColliderRole::PRIMARY, 0);
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let up = -Vec2::Y;
    let position =
        frame.position + up * (state.world.planets[0].radius * BODY_BOUNDS_RADIUS_SCALE + 0.49);
    assert!(state.world.physics.world.insert_body(
        body,
        BodySpec {
            position,
            angle: 1.2,
            linear_velocity: motion::point_velocity(frame, position),
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(collider, 0.5)]
    ));
    GroundSurfaceForce {
        body,
        colliders: vec![collider],
        allegiance: Some(SurfaceAllegiance::Government(4)),
    }
}

fn step_force(state: &mut SurfaceSortieState, force: &GroundSurfaceForce) {
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let position = state
        .world
        .physics
        .world
        .motion(force.body)
        .unwrap()
        .position;
    state.world.physics.apply_gravity_delta(
        force.body,
        (frame.position - position).normalized() * (8.0 / 60.0),
        1.0 / 60.0,
    );
    idle(state, 1);
}

#[test]
fn registered_native_government_force_blocks_without_a_player_seat_and_retires_with_its_wreck() {
    let mut state = fixture(SurfaceMotionPreset::Stationary, 0.0, 1);
    state.world.planets[0].wrapper_omega = 0.0;
    let force = add_ground_force(&mut state, 0);
    assert!(state.register_ground_surface_force(force.clone()));
    step(&mut state, [INTERACT, NEUTRAL]);
    for _ in 0..240 {
        step_force(&mut state, &force);
    }
    assert_eq!(state.player_count(), 1);
    assert_eq!(
        state
            .claim_observation(0, 0)
            .unwrap()
            .opposition
            .ground_units,
        1
    );
    assert_eq!(
        state.claim_observation(0, 0).unwrap().status,
        PlanetClaimStatus::OpposingSurfaceForces
    );
    assert_eq!(state.world.planets[0].owner_id, None);
    for allegiance in [None, Some(SurfaceAllegiance::Player(P1))] {
        let mut friendly = force.clone();
        friendly.allegiance = allegiance;
        assert!(state.register_ground_surface_force(friendly));
        step_force(&mut state, &force);
        assert!(state.claim_observation(0, 0).unwrap().opposition.is_empty());
        assert!(state.register_ground_surface_force(force.clone()));
        step_force(&mut state, &force);
        assert_eq!(
            state
                .claim_observation(0, 0)
                .unwrap()
                .opposition
                .ground_units,
            1
        );
    }
    // A tilted/immobile body is still live. Explicit actor retirement, not
    // motion or orientation, makes its retained physics body a noncombat wreck.
    state.unregister_ground_surface_force(force.body.entity);
    assert!(state.world.physics.world.motion(force.body).is_some());
    for _ in 0..240 {
        step_force(&mut state, &force);
    }
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    assert_eq!(
        state
            .surface_force_observations()
            .iter()
            .filter(|f| matches!(f.id, SurfaceForceId::Ground(_)))
            .count(),
        0
    );
}

#[test]
fn destroying_flag_footing_keeps_the_hostile_force_and_clone_replays_for_both_dirt_shapes() {
    use engine_rapier::terrain::{GrainShape, LooseTerrainConfig, RadialImpulse};
    use engine_terrain::{Brush, EditMode, TerrainEdit, TerrainSurface};
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state =
            SurfaceSortieScenario::init_material_surface(42, 1, TerrainSurface::Interpolated);
        state.world.planets[0].wrapper_omega = 0.0;
        state
            .enable_loose_terrain_with_config(LooseTerrainConfig {
                shape,
                ..Default::default()
            })
            .unwrap();
        idle(&mut state, 240);
        step(&mut state, [INTERACT, NEUTRAL]);
        idle(&mut state, 240);
        assert_eq!(state.world.planets[0].owner_id, Some(0));
        let cell = state.claims[0].flag.unwrap().anchor.footing.unwrap();
        let force = add_ground_force(&mut state, 0);
        assert!(state.register_ground_surface_force(force.clone()));
        for _ in 0..120 {
            step_force(&mut state, &force);
        }
        assert_eq!(
            state.world.planets[0].owner_id,
            Some(0),
            "arrival does not neutralize ownership"
        );
        assert_eq!(
            state
                .claim_observation(0, 0)
                .unwrap()
                .opposition
                .ground_units,
            1
        );
        let local = state.world.planet_terrain(0).unwrap().cell_center(cell);
        let initial = state.terrain_diagnostics().occupied_cells;
        state.world.queue_test_blast(
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 1,
                },
                mode: EditMode::Remove,
            },
            RadialImpulse {
                center: local,
                radius: 3.0,
                speed: 4.0,
            },
        );
        let mut replay = state.clone();
        for tick in 0..240 {
            step_force(&mut state, &force);
            step_force(&mut replay, &force);
            assert_eq!(
                state.world.planets[0].owner_id, None,
                "{shape:?} tick {tick}"
            );
            assert_eq!(
                state
                    .claim_observation(0, 0)
                    .unwrap()
                    .opposition
                    .ground_units,
                1
            );
            assert_eq!(
                SurfaceSortieScenario::observe(&state).payload,
                SurfaceSortieScenario::observe(&replay).payload
            );
        }
        assert_eq!(state.claims[0].captures, 1);
        assert_eq!(state.claims[0].neutralizations, 1);
        assert_eq!(
            state.world.physics.world.snapshot_bytes().unwrap(),
            replay.world.physics.world.snapshot_bytes().unwrap()
        );
        assert!(state.terrain_diagnostics().issues.is_empty());
        let audit = state.terrain_diagnostics();
        assert_eq!(audit.occupied_cells + audit.loose_cells as u64, initial);
        assert_eq!(audit.removed_cells, 0);
    }
}

#[test]
fn ground_registrations_are_bounded_idempotent_and_pruned_with_native_bodies() {
    let mut state = fixture(SurfaceMotionPreset::Stationary, 0.0, 1);
    for i in 0..surface_forces::MAX_GROUND_FORCES {
        let force = add_ground_force(&mut state, i as u64);
        assert!(state.register_ground_surface_force(force.clone()));
        assert!(state.register_ground_surface_force(force));
    }
    let extra = add_ground_force(&mut state, 100);
    assert!(!state.register_ground_surface_force(extra));
    for i in 0..surface_forces::MAX_GROUND_FORCES {
        state
            .world
            .physics
            .world
            .remove_entity(PhysicsId::new(70_000 + i as u64));
    }
    state.update_planet_claims(Duration::ZERO);
    let replacement = add_ground_force(&mut state, 101);
    assert!(state.register_ground_surface_force(replacement));
    assert!(
        state
            .surface_force_observations()
            .iter()
            .all(|f| !matches!(f.id, SurfaceForceId::Ground(_)))
    );
}
