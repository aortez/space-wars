use super::*;
use engine_terrain::{CellCoord, MaterialId};

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
    let mut state = SurfaceSortieScenario::init_material(42, players);
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
    let mut state = parked(1);
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
    state.world.terrain.planets[&0]
        .field
        .local_to_cell(
            (flag.position - flag.normal * 0.08 - frame.position).rotate_radians(-frame.angle),
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
    let mut state = claimed();
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
    let mut state = claimed();
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
    let mut state = claimed();
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
    let mut state = parked(1);
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
