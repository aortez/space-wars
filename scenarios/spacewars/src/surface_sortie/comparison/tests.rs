use super::*;

const DT: Duration = Duration::from_nanos(16_666_667);
fn step(state: &mut SurfaceSortieState, controls: SurfaceSortieAction) {
    SurfaceSortieScenario::step(state, &[controls.encode(PlayerId::PLAYER_1)], DT);
}
fn idle(state: &mut SurfaceSortieState, ticks: usize) {
    for _ in 0..ticks {
        step(state, SurfaceSortieAction::default());
    }
}
fn parked(surface: TerrainSurface) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_surface_comparison(42, 2, surface);
    idle(&mut state, 600);
    for p in 0..2 {
        assert_eq!(
            state.observation(p).landing.phase,
            LandingPhase::Landed,
            "{surface:?}: {:?}",
            state.observation(p)
        );
    }
    assert!(
        state.terrain_diagnostics().issues.is_empty(),
        "{:?}",
        state.terrain_diagnostics()
    );
    state
}

#[test]
fn matched_fields_land_exit_and_claim_on_diagonal_ground() {
    let mut fields = Vec::new();
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        let mut state = SurfaceSortieScenario::init_surface_comparison(42, 2, surface);
        idle(&mut state, 600);
        fields.push(state.world.terrain.planets[&0].field.cells().to_vec());
        println!(
            "landing {surface:?}: P1={:?}, P2={:?}",
            state.observation(0).landing.phase,
            state.observation(1).landing.phase
        );
        // The stepped control reproduces an existing landing stall. It is a
        // comparison result, not an acceptance requirement for the new surface.
        if surface == TerrainSurface::Blocks {
            continue;
        }
        for p in 0..2 {
            assert_eq!(state.observation(p).landing.phase, LandingPhase::Landed);
        }
        step(
            &mut state,
            SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            },
        );
        idle(&mut state, 240);
        assert_eq!(state.location(0), PilotLocation::OnFoot);
        assert_eq!(
            state.world.planets[0].owner_id,
            Some(0),
            "{surface:?}: {:?}",
            state.observation(0)
        );
        assert!(state.observation(0).planet_claim.unwrap().flag.is_some());
    }
    assert!(fields.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn contour_mining_invalidates_flag_support_and_replays() {
    for surface in [TerrainSurface::Contour, TerrainSurface::Interpolated] {
        mining_support_and_replay(surface);
    }
}

fn mining_support_and_replay(surface: TerrainSurface) {
    let mut state = parked(surface);
    step(
        &mut state,
        SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        },
    );
    idle(&mut state, 240);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    let before = state.terrain_diagnostics().occupied_cells;
    let aim = -state.spaceling_snapshot(0).unwrap().up;
    let mut replay = state.clone();
    let actions = [
        SurfaceSortieAction::default().encode(PlayerId::PLAYER_1),
        SurfaceMiningAction {
            aim,
            held: true,
            cycle: false,
        }
        .encode(PlayerId::PLAYER_1),
    ];
    for _ in 0..40 {
        SurfaceSortieScenario::step(&mut state, &actions, DT);
        SurfaceSortieScenario::step(&mut replay, &actions, DT);
        assert_eq!(
            SurfaceSortieScenario::observe(&state).payload,
            SurfaceSortieScenario::observe(&replay).payload
        );
    }
    assert!(state.terrain_diagnostics().occupied_cells < before);
    assert_eq!(state.world.planets[0].owner_id, None);
    // Standing on surviving ground may already start raising a new flag.
    assert!(state.observation(0).planet_claim.unwrap().neutralizations >= 1);
    assert!(
        state.terrain_diagnostics().issues.is_empty(),
        "{:?}",
        state.terrain_diagnostics()
    );
}

#[test]
fn final_bridge_cut_preserves_fragment_surface_material_and_mass() {
    for surface in [TerrainSurface::Contour, TerrainSurface::Interpolated] {
        bridge_cut(surface);
    }
}

fn bridge_cut(surface: TerrainSurface) {
    let mut state = SurfaceSortieScenario::init_surface_comparison(42, 1, surface);
    assert!(state.world.terrain.fragments.is_empty());
    let before = state.terrain_diagnostics();
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(15, 60),
                    radius: 0,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    idle(&mut state, 1);
    assert_eq!(state.world.terrain.fragments.len(), 1);
    for fragment in state.world.terrain.fragments.values() {
        assert_eq!(fragment.geometry.surface(), surface);
        let cells = fragment
            .terrain
            .cells()
            .iter()
            .filter(|c| c.material != engine_terrain::MaterialId::VOID)
            .count();
        assert!(
            (state
                .world
                .physics
                .world
                .body_mass(fragment.assembly.body())
                .unwrap()
                - cells as f32)
                .abs()
                < 0.01
        );
    }
    let after = state.terrain_diagnostics();
    assert_eq!(
        after.occupied_cells + after.removed_cells,
        before.occupied_cells + before.removed_cells
    );
    assert!(after.issues.is_empty(), "{after:?}");
    idle(&mut state, 240);
    assert!(
        state.terrain_diagnostics().issues.is_empty(),
        "{:?}",
        state.terrain_diagnostics()
    );
}

#[test]
fn contour_flag_survives_nearby_edits_then_loses_ownership_with_its_cell() {
    for surface in [TerrainSurface::Contour, TerrainSurface::Interpolated] {
        flag_reanchoring(surface);
    }
}

fn flag_reanchoring(surface: TerrainSurface) {
    let mut state = parked(surface);
    step(
        &mut state,
        SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        },
    );
    idle(&mut state, 240);
    let flag = state.observation(0).planet_claim.unwrap().flag.unwrap();
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let t = &state.world.terrain.planets[&0];
    let cell = t
        .geometry
        .contact_cell(
            &t.field,
            (flag.position - frame.position).rotate_radians(-frame.angle),
            flag.normal.rotate_radians(-frame.angle),
        )
        .unwrap();
    let neighbor = CellCoord::new(cell.x - 1, cell.y);
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: neighbor,
                    radius: 0,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    // Inspect the edit boundary before an actor can start another claim.
    terrain::commit(&mut state.world);
    state.invalidate_flag_footings();
    assert_eq!(
        state.world.planets[0].owner_id,
        Some(0),
        "owner cell {cell:?}, removed {neighbor:?}, surviving {:?}, fragments {}",
        state.world.terrain.planets[&0].field.cell(cell),
        state.world.terrain.fragments.len()
    );
    let flag = state.observation(0).planet_claim.unwrap().flag.unwrap();
    let position = (flag.position - frame.position).rotate_radians(-frame.angle);
    let normal = flag.normal.rotate_radians(-frame.angle);
    let terrain = &state.world.terrain.planets[&0];
    assert!(
        terrain
            .geometry
            .source_cell(&terrain.field, position - normal * 0.001)
            .is_some()
    );
    assert!(
        terrain
            .geometry
            .source_cell(&terrain.field, position + normal * 0.001)
            .is_none()
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
    terrain::commit(&mut state.world);
    state.invalidate_flag_footings();
    assert_eq!(state.world.planets[0].owner_id, None);
    assert!(state.observation(0).planet_claim.unwrap().flag.is_none());
}

fn walk(surface: TerrainSurface, bearing: f32, direction: f32) -> (f32, usize, u64) {
    let mut state = SurfaceSortieScenario::init_material_surface(42, 1, surface);
    state.world.planets[0].wrapper_omega = 0.0;
    state.world.planets[0].wrapper_angle = 0.0;
    state.world.physics.world.set_pose(
        physics::primary_body(physics::planet_entity(0)),
        state.world.planets[0].position,
        0.0,
        true,
    );
    state.world.ships[0].position += Vec2::new(200.0, 200.0);
    state.world.physics.material_queries_dirty = true;
    let up = Vec2::from_radians(bearing);
    let center = state.world.planets[0].position;
    let body = SpacelingAssembly::insert(
        &mut state.world.physics.world,
        pilot_physics_id(PlayerId::PLAYER_1),
        center + up * 60.7,
        rotation_for_direction(up),
        SurfaceSortieState::spec(),
    )
    .unwrap();
    state.pilots[0].body = Some(body);
    state.pilots[0].controls_armed = true;
    idle(&mut state, 120);
    let initial = state.spaceling_snapshot(0).unwrap().motion.position - center;
    let mut grounded = 0;
    for _ in 0..360 {
        step(
            &mut state,
            SurfaceSortieAction {
                horizontal: direction,
                ..Default::default()
            },
        );
        grounded += usize::from(state.spaceling_snapshot(0).unwrap().grounded());
    }
    let snapshot = state.spaceling_snapshot(0).unwrap();
    let end = snapshot.motion.position - center;
    let angle = (initial.x * end.y - initial.y * end.x).atan2(initial.dot(end));
    (angle.abs() * 59.4, grounded, snapshot.knockdowns)
}

#[test]
fn walking_comparison_on_untouched_diagonal_ground() {
    for quadrant in 0..4 {
        let bearing = std::f32::consts::FRAC_PI_4 + quadrant as f32 * std::f32::consts::FRAC_PI_2;
        for direction in [-1.0, 1.0] {
            let blocks = walk(TerrainSurface::Blocks, bearing, direction);
            let contour = walk(TerrainSurface::Contour, bearing, direction);
            let round = walk(TerrainSurface::Interpolated, bearing, direction);
            println!(
                "walk quadrant={quadrant} direction={direction}: blocks={blocks:?} contour={contour:?} round={round:?}"
            );
            assert!(contour.0 > 20.0, "contour walk stalled: {contour:?}");
            assert_eq!(contour.2, 0, "contour walk knocked down");
            assert!(round.0 > 20.0, "round walk stalled: {round:?}");
            assert_eq!(round.2, 0, "round walk knocked down");
        }
    }
}
