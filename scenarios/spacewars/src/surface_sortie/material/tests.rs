use super::*;
use engine_rapier::world::{
    BodyId as PhysicsBodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
};
use engine_terrain::{CellCoord, MaterialId, TerrainSurface};

const DT: Duration = Duration::from_nanos(16_666_667);

fn step(state: &mut SurfaceSortieState, inputs: &[Action]) {
    SurfaceSortieScenario::step(state, inputs, DT);
}

fn idle(state: &mut SurfaceSortieState, ticks: usize) {
    let controls: Vec<_> = (0..state.player_count())
        .flat_map(|p| {
            let owner = PlayerId::from_index(p).unwrap();
            [
                SurfaceSortieAction::default().encode(owner),
                SurfaceMiningAction::default().encode(owner),
            ]
        })
        .collect();
    for _ in 0..ticks {
        step(state, &controls);
    }
}

fn parked(players: usize) -> SurfaceSortieState {
    parked_surface(players, TerrainSurface::Blocks)
}

fn parked_surface(players: usize, surface: TerrainSurface) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_surface(42, players, surface);
    state.world.planets[0].wrapper_omega = 0.0;
    idle(&mut state, 240);
    for p in 0..players {
        assert_eq!(
            state.observation(p).landing.phase,
            LandingPhase::Landed,
            "{:?}",
            state.observation(p)
        );
    }
    state
}

fn transfer(state: &mut SurfaceSortieState, player: usize) {
    idle(state, 1);
    step(
        state,
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::from_index(player).unwrap())],
    );
    idle(state, 1);
}

fn claimed() -> SurfaceSortieState {
    claimed_surface(TerrainSurface::Blocks)
}

fn claimed_surface(surface: TerrainSurface) -> SurfaceSortieState {
    let mut state = parked_surface(1, surface);
    transfer(&mut state, 0);
    assert_eq!(
        state.location(0),
        PilotLocation::OnFoot,
        "{:?}",
        state.observation(0)
    );
    idle(&mut state, 240);
    assert_eq!(
        state.world.planets[0].owner_id,
        Some(0),
        "{:?}",
        state.observation(0)
    );
    state
}

fn flag_cell(state: &SurfaceSortieState) -> CellCoord {
    let flag = state.observation(0).planet_claim.unwrap().flag.unwrap();
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let terrain = &state.world.terrain.planets[&0];
    terrain
        .geometry
        .contact_cell(
            &terrain.field,
            (flag.position - frame.position).rotate_radians(-frame.angle),
            flag.normal.rotate_radians(-frame.angle),
        )
        .unwrap()
}

#[test]
fn material_ships_land_without_berths_and_only_the_spaceling_claims() {
    let mut state = parked(2);
    let bodies = state.world.physics.world.body_count();
    assert_eq!(state.world.planets[0].owner_id, None);
    assert!(
        !state
            .world
            .physics
            .world
            .collider_ids()
            .any(|id| id == physics::spaceport_sensor_id(0))
    );
    assert!(
        state.terrain_diagnostics().issues.is_empty(),
        "{:?}",
        state.terrain_diagnostics()
    );
    transfer(&mut state, 0);
    assert_eq!(
        state.location(0),
        PilotLocation::OnFoot,
        "{:?}",
        state.observation(0)
    );
    assert_eq!(state.location(1), PilotLocation::Aboard(VehicleId(1)));
    for _ in 0..120 {
        if state.observation(0).planet_claim.unwrap().flag.is_some() {
            break;
        }
        idle(&mut state, 1);
    }
    let initial_flag = state.observation(0).planet_claim.unwrap().flag.unwrap();
    let support = state.spaceling_snapshot(0).unwrap().support.unwrap();
    assert!(initial_flag.position.distance_to(support.position) < 0.1);
    idle(&mut state, 240);
    assert_eq!(
        state.world.planets[0].owner_id,
        Some(0),
        "{:?}",
        state.observation(0)
    );
    let flag = state.observation(0).planet_claim.unwrap().flag.unwrap();
    assert!(flag.position.distance_to(initial_flag.position) < 0.001);
    assert!(flag.position.distance_to(state.world.planets[0].position) > 50.0);
    assert_eq!(state.world.physics.world.body_count(), bodies + 1);
    transfer(&mut state, 0);
    assert_eq!(
        state.location(0),
        PilotLocation::Aboard(VehicleId(0)),
        "{:?}",
        state.observation(0)
    );
    assert_eq!(state.world.physics.world.body_count(), bodies);
}

#[test]
fn flag_survives_remeshing_but_destroyed_footing_neutralizes_without_awarding_attacker() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        flag_survives_remeshing_but_destroyed_footing_neutralizes_without_awarding_attacker_on(
            surface,
        );
    }
}

fn flag_survives_remeshing_but_destroyed_footing_neutralizes_without_awarding_attacker_on(
    surface: TerrainSurface,
) {
    let mut state = claimed_surface(surface);
    let cell = flag_cell(&state);
    // A durability-only edit replaces a chunk's colliders without removing support.
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 0,
                },
                mode: EditMode::Damage(1),
            },
        )
        .unwrap();
    step(&mut state, &[]);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    assert!(state.observation(0).planet_claim.unwrap().flag.is_some());
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 2,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    let tick = state.world.tick;
    SurfaceSortieScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.world.tick, tick);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    step(&mut state, &[]);
    assert_eq!(state.world.planets[0].owner_id, None);
    let claim = state.observation(0).planet_claim.unwrap();
    assert!(claim.flag.is_none());
    assert_eq!(claim.neutralizations, 1);
    assert!(
        state.terrain_diagnostics().issues.is_empty(),
        "{:?}",
        state.terrain_diagnostics()
    );
}

#[test]
fn match_expiry_scores_a_flag_destroyed_on_the_final_step_as_neutral() {
    use crate::surface_sortie::match_rules::{MatchEndReason, MatchOutcome};
    let mut state = SurfaceSortieScenario::init_material_combat_flight(42, &[]);
    state.enable_match_rules();
    state.world.planets[0].wrapper_omega = 0.0;
    idle(&mut state, 240);
    transfer(&mut state, 0);
    idle(&mut state, 240);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    let cell = flag_cell(&state);
    let remaining = Duration::from_secs(600) - DT * state.world.tick as u32;
    state.advance_match_time(remaining - DT);
    let mut retained = state.clone();
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 2,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    step(&mut retained, &[]);
    step(&mut state, &[]);
    assert_eq!(
        retained.match_outcome(),
        Some(MatchOutcome::Winner(PlayerId::PLAYER_1))
    );
    assert_eq!(state.world.planets[0].owner_id, None);
    let observation = state.match_observation().unwrap();
    assert_eq!(observation.reason, Some(MatchEndReason::TimeLimit));
    assert_eq!(observation.outcome, Some(MatchOutcome::Draw));
    assert_eq!(observation.owned_planets, [0, 0]);
    assert!(observation.pilots.iter().all(|pilot| pilot.alive()));
}

#[test]
fn removal_of_ship_support_rejects_same_tick_transfer() {
    let mut state = parked(1);
    let body = state
        .world
        .physics
        .world
        .motion(state.world.physics.ship_body(0))
        .unwrap();
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    for foot in physics::LANDING_FEET {
        let point = body.position + foot.rotate_radians(body.angle) - Vec2::Y * 0.5;
        let cell = state.world.terrain.planets[&0]
            .field
            .local_to_cell((point - frame.position).rotate_radians(-frame.angle))
            .unwrap();
        state
            .world
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle {
                        center: cell,
                        radius: 3,
                    },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
    }
    step(
        &mut state,
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
    );
    assert_eq!(state.location(0), PilotLocation::Aboard(VehicleId(0)));
    assert_eq!(
        state.pilots[0].last_transfer,
        TransferResult::ShipNotSettled
    );
}

#[test]
fn hatch_uses_neighboring_footing_beside_a_narrow_cut() {
    let mut state = parked(1);
    let before = state.material_access(0).unwrap();
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let cell = state.world.terrain.planets[&0]
        .field
        .local_to_cell(
            (before.point - before.normal * 0.08 - frame.position).rotate_radians(-frame.angle),
        )
        .unwrap();
    // The controlled north surface is local +X. Make the central hatch
    // probe miss, while leaving a standing place one cell to either side.
    for depth in 0..4 {
        state
            .world
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle {
                        center: CellCoord::new(cell.x - depth, cell.y),
                        radius: 0,
                    },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
    }
    idle(&mut state, 90);
    assert_eq!(state.pilots[0].landing.phase, LandingPhase::Landed);
    let after = state.material_access(0).expect("nearby surviving footing");
    assert!(after.point.distance_to(before.point) < 1.5);
    transfer(&mut state, 0);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    idle(&mut state, 60);
    assert!(state.observation(0).grounded);
    assert_eq!(state.observation(0).pilot_support_planet, Some(0));
}

#[test]
fn hatch_prefers_a_clear_neighbor_when_the_central_floor_is_obstructed() {
    let mut state = parked(1);
    let before = state.material_access(0).unwrap();
    let spec = SurfaceSortieState::spec();
    let obstacle = PhysicsId::new(45_000);
    let mut collider = ColliderSpec::ball(ColliderId::new(obstacle, ColliderRole::PRIMARY, 0), 0.2);
    // This blocks the exiting capsule without impersonating material ground.
    collider.collision_groups = physics::spaceling_collision_groups();
    assert!(state.world.physics.world.insert_body(
        PhysicsBodyId::new(obstacle, BodyRole::PRIMARY),
        BodySpec {
            kind: engine_rapier::world::BodyKind::Fixed,
            position: before.point + before.normal * (spec.half_height() + 0.12),
            ..BodySpec::default()
        },
        &[collider]
    ));
    idle(&mut state, 1);
    let body = state
        .world
        .physics
        .world
        .motion(state.world.physics.ship_body(0))
        .unwrap();
    let first = state
        .material_access_candidates_at(0, ShipForm::Ship, body.position, body.angle)
        .next()
        .expect("central floor still exists");
    assert!(first.point.distance_to(before.point) < 0.1);
    assert!(!state.world.physics.world.capsule_is_clear(
        first.point + first.normal * (spec.half_height() + 0.12),
        rotation_for_direction(first.normal),
        spec.half_segment,
        spec.radius + 0.04,
        spec.collision_groups,
    ));
    let after = state.material_access(0).expect("clear neighboring exit");
    assert!((0.8..1.5).contains(&after.point.distance_to(first.point)));
    assert_eq!(state.pilots[0].landing.phase, LandingPhase::Landed);
    let bodies = state.world.physics.world.body_count();
    transfer(&mut state, 0);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    assert_eq!(state.world.physics.world.body_count(), bodies + 1);
}

#[test]
fn mined_out_hatch_rejects_exit_even_when_both_ship_feet_remain_supported() {
    let mut state = parked(1);
    let hit = state.material_access(0).unwrap();
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let cell = state.world.terrain.planets[&0]
        .field
        .local_to_cell(
            (hit.point - hit.normal * 0.08 - frame.position).rotate_radians(-frame.angle),
        )
        .unwrap();
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 3,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    idle(&mut state, 90);
    assert_eq!(state.pilots[0].landing.phase, LandingPhase::Landed);
    transfer(&mut state, 0);
    assert_eq!(state.location(0), PilotLocation::Aboard(VehicleId(0)));
    assert_eq!(state.pilots[0].last_transfer, TransferResult::ExitBlocked);
}

#[test]
fn edits_block_transfer_queries_until_the_next_completed_physics_step() {
    let mut state = parked(1);
    // This distant edit leaves both feet and the hatch floor intact, but its
    // new rectangle colliders are absent from the completed broad-phase index.
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(60, 60),
                    radius: 2,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    step(
        &mut state,
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
    );
    assert_eq!(state.pilots[0].last_transfer, TransferResult::ExitBlocked);
    assert_eq!(state.location(0), PilotLocation::Aboard(VehicleId(0)));
    assert!(!state.world.physics.material_queries_dirty);
    transfer(&mut state, 0);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
}

#[test]
fn durability_damage_preserves_landing_contacts_and_same_tick_transfer() {
    let mut state = parked(1);
    let cell = state.material_footings()[0][0].unwrap();
    let damage = TerrainEdit {
        brush: Brush::Circle {
            center: cell,
            radius: 0,
        },
        mode: EditMode::Damage(1),
    };
    for _ in 0..20 {
        state.world.queue_planet_edit(0, damage).unwrap();
        idle(&mut state, 8);
        assert_eq!(state.pilots[0].landing.phase, LandingPhase::Landed);
        assert_eq!(state.world.terrain.removed_cells, 0);
    }
    state.world.queue_planet_edit(0, damage).unwrap();
    step(
        &mut state,
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
    );
    assert_eq!(
        state.location(0),
        PilotLocation::OnFoot,
        "{:?}",
        state.observation(0)
    );
}

#[test]
fn structural_remeshing_preserves_earned_landing_when_both_contact_cells_survive() {
    let mut state = parked(1);
    let foot = state.material_footings()[0][0].unwrap();
    for offset in 3..11 {
        let cell = CellCoord::new(foot.x - offset, foot.y);
        assert_ne!(
            state.world.terrain.planets[&0]
                .field
                .cell(cell)
                .unwrap()
                .material,
            MaterialId::VOID
        );
        state
            .world
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle {
                        center: cell,
                        radius: 0,
                    },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
        step(&mut state, &[]);
        assert_eq!(
            state.pilots[0].landing.phase,
            LandingPhase::Landed,
            "offset {offset}: {:?}",
            state.observation(0)
        );
        idle(&mut state, 7);
    }
    transfer(&mut state, 0);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
}

#[test]
fn held_jump_is_not_retriggered_by_unrelated_terrain_edits() {
    let mut state = claimed();
    let held = SurfaceSortieAction {
        primary_held: true,
        ..Default::default()
    }
    .encode(PlayerId::PLAYER_1);
    for _ in 0..180 {
        step(&mut state, std::slice::from_ref(&held));
    }
    assert_eq!(state.observation(0).jumps, 1);
    assert!(state.observation(0).grounded);
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(60, 60),
                    radius: 2,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    for _ in 0..120 {
        step(&mut state, std::slice::from_ref(&held));
    }
    assert_eq!(state.observation(0).jumps, 1);
    assert!(state.observation(0).grounded);
}

#[test]
fn aimed_mining_removes_material_and_clone_continuation_preserves_physics() {
    let mut state = claimed();
    let up = state.spaceling_snapshot(0).unwrap().up;
    let mine = SurfaceMiningAction {
        aim: -up,
        held: true,
        cycle: false,
    }
    .encode(PlayerId::PLAYER_1);
    step(&mut state, std::slice::from_ref(&mine));
    let mut cloned = state.clone();
    let before = state.terrain_diagnostics().occupied_cells;
    for _ in 0..120 {
        step(&mut state, std::slice::from_ref(&mine));
        step(&mut cloned, std::slice::from_ref(&mine));
    }
    assert!(state.terrain_diagnostics().occupied_cells < before);
    assert_eq!(
        SurfaceSortieScenario::observe(&state).payload,
        SurfaceSortieScenario::observe(&cloned).payload
    );
    assert_eq!(
        state.terrain_diagnostics().motion_hash,
        cloned.terrain_diagnostics().motion_hash
    );
    let audit = state.terrain_diagnostics();
    assert!(audit.issues.is_empty(), "{audit:?}");
    assert_eq!(audit.occupied_cells + audit.removed_cells, 11_069);
    assert!(
        !state.world.terrain.planets[&0]
            .field
            .cells()
            .iter()
            .all(|cell| cell.material == MaterialId::VOID)
    );
}

#[test]
fn detached_flag_footing_does_not_carry_planet_ownership_with_the_fragment() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        detached_flag_footing_does_not_carry_planet_ownership_with_the_fragment_on(surface);
    }
}

fn detached_flag_footing_does_not_carry_planet_ownership_with_the_fragment_on(
    surface: TerrainSurface,
) {
    let mut state = claimed_surface(surface);
    let cell = flag_cell(&state);
    assert!(cell.x > 100, "controlled north surface is local +X");
    let point = state.world.terrain.planets[&0].field.cell_center(cell);
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let world_point = frame.position + point.rotate_radians(frame.angle);
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Capsule {
                    start: CellCoord::new(cell.x - 2, 0),
                    end: CellCoord::new(cell.x - 2, 120),
                    radius: 0,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    step(&mut state, &[]);
    assert_eq!(state.world.planets[0].owner_id, None);
    assert!(state.observation(0).planet_claim.unwrap().flag.is_none());
    assert_eq!(
        state.world.terrain.planets[&0]
            .field
            .cell(cell)
            .unwrap()
            .material,
        MaterialId::VOID
    );
    assert!(
        state.world.terrain.fragments.values().any(|fragment| {
            let body = state
                .world
                .physics
                .world
                .motion(fragment.assembly.body())
                .unwrap();
            let local = (world_point - body.position).rotate_radians(-body.angle);
            fragment
                .terrain
                .local_to_cell(local)
                .and_then(|cell| fragment.terrain.cell(cell))
                .is_some_and(|cell| cell.material != MaterialId::VOID)
        }),
        "flag footing survives as material in a detached body"
    );
    for collider in state
        .world
        .physics
        .world
        .collider_ids()
        .filter(|id| id.entity.value() >= terrain::FRAGMENT_ID_BASE)
    {
        assert_eq!(physics::planet_surface_support_index(collider), None);
    }
    assert!(
        state.terrain_diagnostics().issues.is_empty(),
        "{:?}",
        state.terrain_diagnostics()
    );
}

#[test]
fn material_flag_follows_its_contact_frame_during_translation_and_spin() {
    let mut state = claimed();
    let before = state.observation(0).planet_claim.unwrap().flag.unwrap();
    let old_frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let local = (before.position - old_frame.position).rotate_radians(-old_frame.angle);
    state.motion_preset = SurfaceMotionPreset::Translating;
    state.world.planets[0].wrapper_omega = 0.015;
    idle(&mut state, 90);
    let after = state.observation(0).planet_claim.unwrap().flag.unwrap();
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    assert!(
        after
            .position
            .distance_to(frame.position + local.rotate_radians(frame.angle))
            < 0.001
    );
    assert!(after.position.distance_to(before.position) > 1.0);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
}

#[test]
fn held_mining_cannot_leak_across_disembark_and_seats_are_independent() {
    let mut state = parked(2);
    let mine = SurfaceMiningAction {
        aim: -Vec2::Y,
        held: true,
        cycle: false,
    }
    .encode(PlayerId::PLAYER_1);
    step(
        &mut state,
        &[
            mine.clone(),
            SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1),
        ],
    );
    for _ in 0..60 {
        step(
            &mut state,
            &[
                mine.clone(),
                SurfaceSortieAction::default().encode(PlayerId::PLAYER_1),
            ],
        );
    }
    assert_eq!(state.world.terrain.removed_cells, 0);
    assert_eq!(state.location(1), PilotLocation::Aboard(VehicleId(1)));
    assert!(!state.mining_observation(0).unwrap().armed);
    idle(&mut state, 1);
    for _ in 0..40 {
        step(&mut state, std::slice::from_ref(&mine));
    }
    assert!(state.world.terrain.removed_cells > 0);
    assert_eq!(state.mining_observation(1).unwrap().radius, 0);
}

#[test]
fn two_material_claimants_contest_without_seat_order_ownership() {
    let mut state = parked(2);
    step(
        &mut state,
        &[
            SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1),
            SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_2),
        ],
    );
    idle(&mut state, 300);
    for p in 0..2 {
        assert_eq!(state.location(p), PilotLocation::OnFoot);
        assert_eq!(
            state.observation(p).planet_claim.unwrap().status,
            PlanetClaimStatus::Contested
        );
    }
    assert_eq!(state.world.planets[0].owner_id, None);
    transfer(&mut state, 1);
    idle(&mut state, 240);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
}

#[test]
fn material_recovery_rebuilds_once_and_flag_loss_interrupts_construction() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        material_recovery_rebuilds_once_and_flag_loss_interrupts_construction_on(surface);
    }
}

fn material_recovery_rebuilds_once_and_flag_loss_interrupts_construction_on(
    surface: TerrainSurface,
) {
    let mut state = claimed_surface(surface);
    let identity = state.observation(0).spaceling;
    let health = state.world.ships[0].life_max;
    state.world.ships[0].translate_life(-health);
    idle(&mut state, 120);
    assert!(!state.vehicle_available(0));
    assert!(state.observation(0).recovery.unwrap().rebuild_progress > 0.0);
    let mut interrupted = state.clone();
    let cell = flag_cell(&interrupted);
    interrupted
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 2,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    step(&mut interrupted, &[]);
    assert_eq!(
        interrupted
            .observation(0)
            .recovery
            .unwrap()
            .rebuild_progress,
        0.0
    );
    assert_eq!(interrupted.world.planets[0].owner_id, None);
    idle(&mut state, 780);
    assert!(state.vehicle_available(0), "{:?}", state.observation(0));
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    assert_eq!(state.observation(0).spaceling, identity);
    assert_eq!(state.observation(0).recovery.unwrap().rebuilds, 1);
    assert_eq!(state.world.ships[0].life, health);
    assert_eq!(state.pilots[0].landing.phase, LandingPhase::Landed);
}

#[test]
fn material_escape_pod_can_land_and_disembark() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        material_escape_pod_can_land_and_disembark_on(surface);
    }
}

fn material_escape_pod_can_land_and_disembark_on(surface: TerrainSurface) {
    let mut state = parked_surface(1, surface);
    let identity = state.observation(0).spaceling;
    let health = state.world.ships[0].life_max;
    state.world.ships[0].translate_life(-health);
    idle(&mut state, 480);
    assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
    assert_eq!(
        state.pilots[0].landing.phase,
        LandingPhase::Landed,
        "{:?}",
        state.observation(0)
    );
    transfer(&mut state, 0);
    assert_eq!(
        state.location(0),
        PilotLocation::OnFoot,
        "{:?}",
        state.observation(0)
    );
    assert_eq!(state.observation(0).spaceling, identity);
    idle(&mut state, 840);
    assert!(state.vehicle_available(0), "{:?}", state.observation(0));
    assert_eq!(state.observation(0).recovery.unwrap().rebuilds, 1);
}

#[test]
fn corner_contacts_only_preserve_an_earned_landing_with_two_live_feet() {
    let mut state = SurfaceSortieScenario::init_material_arena_trial(0, true, 0.0);
    step(&mut state, &[]);
    let planet = 2;
    let frame = motion::SurfaceFrame::read(&state.world.physics, planet);
    // A physical corner pose from the seed-0 mirrored Pi return replay. This
    // unit fixture supplies prior landing state separately to test hysteresis;
    // the mission regression flies the approach and earns landing normally.
    let center = frame.position + Vec2::new(-100.61465, -21.234776).rotate_radians(frame.angle);
    let angle = frame.angle + 1.8736967;
    let body = state.world.physics.ship_body(0);
    assert!(
        state
            .world
            .physics
            .world
            .set_pose(body, center, angle, true)
    );
    assert!(
        state
            .world
            .physics
            .world
            .set_velocity(body, Vec2::ZERO, 0.0, true)
    );
    let ship = &mut state.world.ships[0];
    ship.position = center - SHIP_PIVOT;
    ship.rotation_radians = angle;
    ship.direction = Vec2::Y.rotate_radians(angle);
    ship.velocity = Vec2::ZERO;
    ship.omega = 0.0;
    state.world.physics.world.step(DT.as_secs_f32());
    let frame = motion::SurfaceFrame::read(&state.world.physics, planet);
    let center = state.world.physics.world.motion(body).unwrap().position;
    let up = (center - frame.position).normalized();
    let strict = state.world.physics.landing_support_contacts(0, planet, up);
    let parked = state
        .world
        .physics
        .parked_landing_support_contacts(0, planet, up);
    assert_eq!(strict.iter().flatten().count(), 1, "{strict:?}");
    assert_eq!(parked.iter().flatten().count(), 2, "{parked:?}");
    assert!(
        state
            .world
            .physics
            .parked_landing_support_contacts(0, 0, up)
            .iter()
            .all(Option::is_none)
    );
    assert!(
        state
            .world
            .physics
            .parked_landing_support_contacts(0, planet, -up)
            .iter()
            .all(Option::is_none)
    );
    let update = |landing: &mut LandingTelemetry, state: &SurfaceSortieState| {
        landing.update(
            &state.world.physics,
            0,
            planet,
            &state.world.planets[planet],
            &state.world.ships[0],
            DT.as_secs_f32(),
        );
    };
    let mut arriving = LandingTelemetry::default();
    update(&mut arriving, &state);
    assert_ne!(arriving.phase, LandingPhase::Landed);
    assert!(!arriving.corner_support);
    let earned = LandingTelemetry {
        planet: Some(planet),
        phase: LandingPhase::Landed,
        settled_seconds: 0.25,
        ..Default::default()
    };
    let mut landing = earned;
    update(&mut landing, &state);
    assert_eq!(landing.phase, LandingPhase::Landed);
    assert!(landing.corner_support);
    let mut foreign = earned;
    foreign.planet = Some(0);
    update(&mut foreign, &state);
    assert_ne!(foreign.phase, LandingPhase::Landed);
    for takeoff in [true, false] {
        state.world.ships[0].thrust = if takeoff { 1.0 } else { 0.0 };
        state.world.ships[0].wings_closed = !takeoff;
        let mut landing = earned;
        update(&mut landing, &state);
        assert_ne!(landing.phase, LandingPhase::Landed);
    }
    state.world.ships[0].wings_closed = false;
    // Losing an actual support collider invalidates the parked pair at once.
    let planet_body = state.world.physics.planet_body(planet);
    assert!(state.world.physics.world.replace_colliders(
        planet_body,
        parked[1].unwrap().collider.role,
        &[],
    ));
    let mut landing = earned;
    update(&mut landing, &state);
    assert_ne!(landing.phase, LandingPhase::Landed);
    assert!(!landing.corner_support);
}
