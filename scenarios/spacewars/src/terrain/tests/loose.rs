use super::*;

fn material(state: &SpacewarsState) -> BTreeMap<MaterialId, u64> {
    let mut quantities = BTreeMap::new();
    for field in state
        .terrain
        .planets
        .values()
        .map(|p| &p.field)
        .chain(state.terrain.fragments.values().map(|f| &f.terrain))
    {
        for cell in field
            .cells()
            .iter()
            .filter(|c| c.material != MaterialId::VOID)
        {
            *quantities.entry(cell.material).or_default() += 1;
        }
    }
    for quantity in state.loose_terrain().unwrap().quantities() {
        *quantities.entry(quantity.material).or_default() += quantity.cells;
    }
    quantities
}

fn enabled(surface: TerrainSurface, limit: usize) -> SpacewarsState {
    let mut state = fixture();
    state.physics.world.remove_entity(physics::planet_entity(0));
    state.terrain.planets.clear();
    state.terrain.surface = surface;
    state.enable_planet_terrain(0).unwrap();
    state
        .enable_loose_terrain(LooseTerrainConfig {
            max_grains: limit,
            ..Default::default()
        })
        .unwrap();
    step(&mut state);
    state
}

fn fire(state: &mut SpacewarsState) {
    let center = state.planets[0].position;
    let angle = state.planets[0].wrapper_angle;
    let up = Vec2::Y.rotate_radians(angle);
    state.ships[0].position = center + Vec2::new(150.0, 100.0);
    state.debris.push(DebrisState::new_shell(
        0,
        0,
        center + up * 85.0,
        -up * 1800.0,
        angle,
    ));
    let before = state.terrain_cannon_hits();
    for _ in 0..10 {
        step(state);
        if state.terrain_cannon_hits() > before {
            return;
        }
    }
    panic!("physical cannon missed terrain");
}

#[test]
fn physical_cannon_releases_material_on_rotating_surfaces_and_replays() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        let mut state = enabled(surface, 192);
        state.planets[0].wrapper_angle = 0.73;
        state.planets[0].wrapper_omega = 0.04;
        step(&mut state);
        let initial = material(&state);
        fire(&mut state);
        assert_eq!(state.loose_terrain().unwrap().len(), 0);
        assert_eq!(state.terrain.pending.len(), 1);
        let mut checkpoint = state.clone();
        for _ in 0..120 {
            step(&mut state);
            step(&mut checkpoint);
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "{:?}", audit.issues);
            assert!(audit.loose_cells > 0 && audit.loose_cells <= 113);
            assert_eq!(audit.removed_cells, 0);
            assert_eq!(material(&state), initial);
            assert_eq!(
                observation(&state).payload,
                observation(&checkpoint).payload
            );
            assert_eq!(
                state.physics.snapshot_bytes(),
                checkpoint.physics.snapshot_bytes()
            );
        }
        assert!(
            state
                .planet_terrain(0)
                .unwrap()
                .cells()
                .iter()
                .any(|c| c.material == ORE && c.durability == 20)
        );
    }
}

#[test]
fn full_pool_rejects_damage_atomically_but_keeps_mining_operational() {
    let mut state = enabled(TerrainSurface::Blocks, 1);
    fire(&mut state);
    let original = state.planet_terrain(0).unwrap().clone();
    let world = state.physics.snapshot_bytes();
    let next = state.terrain.next_fragment;
    commit(&mut state);
    assert_eq!(state.terrain.rejected_releases, 1);
    assert_eq!(state.planet_terrain(0).unwrap(), &original);
    assert_eq!(state.physics.snapshot_bytes(), world);
    assert_eq!(state.terrain.next_fragment, next);
    assert!(state.loose_terrain().unwrap().is_empty());
    fire(&mut state);
    cut(&mut state, Vec2::ZERO, Vec2::ZERO, 0);
    commit(&mut state);
    assert_eq!(state.terrain_removed_cells(), 1);
    assert_eq!(state.terrain.rejected_releases, 2);
    assert!(state.loose_terrain().unwrap().is_empty());
    assert!(state.terrain_diagnostics().issues.is_empty());
}

#[test]
fn ordered_mining_and_impact_share_one_split_and_do_not_double_count() {
    for mining_first in [true, false] {
        let mut state = enabled(TerrainSurface::Interpolated, 192);
        let before = remaining(&state);
        let edit = TerrainEdit {
            brush: Brush::Circle {
                center: state
                    .planet_terrain(0)
                    .unwrap()
                    .local_to_cell(Vec2::ZERO)
                    .unwrap(),
                radius: 1,
            },
            mode: EditMode::Remove,
        };
        let release = PendingEdit {
            body: physics::planet_entity(0),
            edit,
            blast: Some(RadialImpulse {
                center: Vec2::ZERO,
                radius: 3.0,
                speed: 0.0,
            }),
        };
        let mining = PendingEdit {
            blast: None,
            ..release
        };
        state.terrain.pending = if mining_first {
            vec![mining, release]
        } else {
            vec![release, mining]
        };
        commit(&mut state);
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty(), "{:?}", audit.issues);
        assert_eq!(
            audit.occupied_cells + audit.removed_cells + audit.loose_cells as u64,
            before as u64
        );
        assert_eq!(audit.loose_cells, if mining_first { 0 } else { 5 });
        assert_eq!(audit.removed_cells, if mining_first { 5 } else { 0 });
    }
}

#[test]
fn releasing_base_footing_disables_existing_services() {
    let mut state = enabled(TerrainSurface::Blocks, 192);
    state.ships[0].position = spaceport_docking_anchor(&state.planets[0]);
    state.ships[0].set_brake(1.0);
    for _ in 0..5 {
        step(&mut state);
    }
    assert!(state.physics.ship_is_constrained(0));
    cut(&mut state, Vec2::new(58.0, -5.0), Vec2::new(58.0, 5.0), 4);
    state.terrain.pending[0].blast = Some(RadialImpulse {
        center: Vec2::new(58.0, 0.0),
        radius: 10.0,
        speed: 0.0,
    });
    step(&mut state);
    assert!(!state.planet_base_supported(0));
    assert!(!state.physics.ship_is_constrained(0));
    assert!(state.spaceport_contacts.is_empty());
    assert_eq!(state.terrain_removed_cells(), 0);
    assert!(!state.loose_terrain().unwrap().is_empty());
    assert!(state.terrain_diagnostics().issues.is_empty());
}

#[test]
fn real_shell_hitting_loose_dirt_is_consumed_and_reblasts_existing_material() {
    let mut state = enabled(TerrainSurface::Blocks, 192);
    fire(&mut state);
    commit(&mut state);
    let initial = material(&state);
    let body = state.loose_terrain().unwrap().iter().next().unwrap().body();
    let at = state.planets[0].position + Vec2::new(-120.0, 0.0);
    state.physics.world.set_pose(body, at, 0.0, true);
    state
        .physics
        .world
        .set_velocity(body, Vec2::ZERO, 0.0, true);
    state.debris.push(DebrisState::new_shell(
        0,
        0,
        at + Vec2::Y * 5.0,
        -Vec2::Y * 300.0,
        0.0,
    ));
    for _ in 0..5 {
        step(&mut state);
        if !state.terrain.pending_blasts.is_empty() {
            break;
        }
    }
    assert_eq!(state.terrain.pending_blasts.len(), 1);
    assert_eq!(state.terrain_cannon_hits(), 2);
    let before = state.physics.world.motion(body).unwrap().linear_velocity;
    commit(&mut state);
    let after = state.physics.world.motion(body).unwrap().linear_velocity;
    assert!(after.distance_to(before) > 1.0);
    assert_eq!(material(&state), initial);
    for _ in 0..5 {
        step(&mut state);
    }
    assert_eq!(state.terrain_cannon_hits(), 2);
    assert!(state.terrain_diagnostics().issues.is_empty());
}
