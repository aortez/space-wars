//! Canonical actors must use ground that passed through the loose-material pool.
use super::*;
use engine_rapier::terrain::{GrainShape, LooseTerrainConfig, RadialImpulse};
use engine_terrain::{Brush, CellCoord, EditMode, MaterialId, TerrainEdit, TerrainSurface};

fn release(state: &mut SurfaceSortieState, point: Vec2, radius: u32, speed: f32) {
    let planet = state.world.planets[0];
    let local = (point - planet.position).rotate_radians(-planet.wrapper_angle);
    let center = state
        .world
        .planet_terrain(0)
        .unwrap()
        .local_to_cell(local)
        .unwrap();
    state.world.queue_test_blast(
        TerrainEdit {
            brush: Brush::Circle { center, radius },
            mode: EditMode::Remove,
        },
        RadialImpulse {
            center: local,
            radius: radius as f32 + 2.0,
            speed,
        },
    );
}

fn ground(state: &SurfaceSortieState, x: f32) -> engine_rapier::world::RayHit {
    let planet = state.world.planets[0];
    state
        .world
        .physics
        .material_ground_ray(
            0,
            planet.position + Vec2::new(x, planet.radius + 20.0),
            -Vec2::Y,
            40.0,
        )
        .unwrap()
}

fn footing(state: &SurfaceSortieState, hit: engine_rapier::world::RayHit) -> CellCoord {
    let planet = state.world.planets[0];
    let terrain = &state.world.terrain.planets[&0];
    terrain
        .geometry
        .contact_cell(
            &terrain.field,
            (hit.point - planet.position).rotate_radians(-planet.wrapper_angle),
            hit.normal.rotate_radians(-planet.wrapper_angle),
        )
        .unwrap()
}

fn rebuilt_pad(shape: GrainShape, slumping: bool) -> (SurfaceSortieState, Vec<CellCoord>, u64) {
    let mut state =
        SurfaceSortieScenario::init_material_surface(42, 1, TerrainSurface::Interpolated);
    state.world.planets[0].wrapper_omega = 0.0;
    state.world.ships[0].position = state.world.planets[0].position + Vec2::new(200.0, 200.0);
    state
        .enable_loose_terrain_with_config(LooseTerrainConfig {
            shape,
            slumping: slumping.then(Default::default),
            ..Default::default()
        })
        .unwrap();
    idle(&mut state, 1);
    let initial = state.terrain_diagnostics().occupied_cells;
    // Loosen a shallow strip, preserving a traversable grade. Gravity and the
    // shared quiet-contact path must rebuild it before either actor arrives.
    // A deep crater can legitimately leave ledges too tall for this tiny pilot.
    for x in -10..=10 {
        let center = footing(&state, ground(&state, x as f32));
        let local = state.world.planet_terrain(0).unwrap().cell_center(center);
        state.world.queue_test_blast(
            TerrainEdit {
                brush: Brush::Circle { center, radius: 0 },
                mode: EditMode::Remove,
            },
            RadialImpulse {
                center: local,
                radius: 2.0,
                speed: 0.0,
            },
        );
    }
    idle(&mut state, 1);
    let terrain = state.world.planet_terrain(0).unwrap();
    let released: Vec<_> = state
        .world
        .loose_terrain()
        .unwrap()
        .iter()
        .map(|grain| {
            let motion = state.world.physics.world.motion(grain.body()).unwrap();
            terrain
                .local_to_cell(
                    (motion.position - state.world.planets[0].position)
                        .rotate_radians(-state.world.planets[0].wrapper_angle),
                )
                .unwrap()
        })
        .collect();
    assert!(
        released.len() >= 18,
        "fixture must release a real surface patch"
    );
    idle(&mut state, 600);
    let audit = state.terrain_diagnostics();
    assert!(audit.deposited_cells >= 18, "{shape:?}: {audit:?}");
    assert!(released.contains(&footing(&state, ground(&state, 0.0))));
    assert_conserved(&state, initial);
    (state, released, initial)
}

fn assert_conserved(state: &SurfaceSortieState, initial: u64) {
    let audit = state.terrain_diagnostics();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert_eq!(audit.occupied_cells + audit.loose_cells as u64, initial);
    assert_eq!(audit.removed_cells, 0);
}

#[test]
fn pilot_walks_across_rebuilt_ground_and_loses_blasted_support() {
    for (shape, slumping) in [GrainShape::Round, GrainShape::Hexagon]
        .into_iter()
        .flat_map(|shape| [false, true].map(|enabled| (shape, enabled)))
    {
        let (mut state, released, initial) = rebuilt_pad(shape, slumping);
        state.world.ships[0].dead = true;
        state.world.ships[0].fragmented = true;
        let hit = ground(&state, -4.0);
        assert!(released.contains(&footing(&state, hit)));
        let up = (hit.point - state.world.planets[0].position).normalized();
        let spec = SurfaceSortieState::spec();
        let body = SpacelingAssembly::insert(
            &mut state.world.physics.world,
            pilot_physics_id(PlayerId::PLAYER_1),
            hit.point + up * (spec.half_height() + 0.05),
            rotation_for_direction(up),
            spec,
        )
        .unwrap();
        state.pilots[0].body = Some(body);
        state.pilots[0].controls_armed = true;
        idle(&mut state, 120);
        let start = state.spaceling_snapshot(0).unwrap();
        assert!(start.grounded(), "{shape:?}: {start:?}");
        for _ in 0..90 {
            tick(
                &mut state,
                SurfaceSortieAction {
                    horizontal: 1.0,
                    ..Default::default()
                },
            );
            let pilot = state.spaceling_snapshot(0).unwrap();
            assert!(!pilot.needs_get_up(), "{shape:?}: {pilot:?}");
        }
        let walked = state.spaceling_snapshot(0).unwrap();
        let distance = walked.motion.position.x - start.motion.position.x;
        assert!(
            distance > 1.5 * spec.walk_speed * 0.8,
            "{shape:?}: distance={distance}, start={start:?}, end={walked:?}"
        );
        assert!(walked.grounded());
        assert_eq!(walked.jumps, 0);
        idle(&mut state, 60);
        let before = state.spaceling_snapshot(0).unwrap();
        let underfoot = before.motion.position - before.up * spec.half_height();
        release(&mut state, underfoot, 4, 8.0);
        let mut replay = state.clone();
        let mut lost_support = false;
        for _ in 0..180 {
            idle(&mut state, 1);
            idle(&mut replay, 1);
            let pilot = state.spaceling_snapshot(0).unwrap();
            lost_support |= !pilot.grounded();
            assert!(pilot.motion.position.x.is_finite() && pilot.motion.position.y.is_finite());
            assert_eq!(
                SurfaceSortieScenario::observe(&state).payload,
                SurfaceSortieScenario::observe(&replay).payload
            );
        }
        assert!(
            lost_support,
            "removed footing must not leave a standing constraint"
        );
        assert_conserved(&state, initial);
    }
}

#[test]
fn ship_lands_on_deposited_cells_and_revalidates_after_they_are_blasted() {
    for (shape, slumping) in [GrainShape::Round, GrainShape::Hexagon]
        .into_iter()
        .flat_map(|shape| [false, true].map(|enabled| (shape, enabled)))
    {
        let (mut state, released, initial) = rebuilt_pad(shape, slumping);
        let point = ground(&state, 0.0).point;
        let ship = &mut state.world.ships[0];
        ship.position = point + Vec2::Y * 18.0 - SHIP_PIVOT;
        ship.rotation_radians = 0.0;
        ship.direction = Vec2::Y;
        ship.velocity = -Vec2::Y * 3.0;
        ship.omega = 0.0;
        for _ in 0..900 {
            idle(&mut state, 1);
            if state.vehicle_settled(0) {
                break;
            }
        }
        assert!(
            state.vehicle_settled(0),
            "{shape:?}: {:?}",
            state.observation(0)
        );
        let feet = state.material_footings()[0];
        assert!(
            feet.into_iter()
                .all(|cell| cell.is_some_and(|cell| released.contains(&cell))),
            "{feet:?}"
        );
        assert_eq!(state.pilots[0].landing.supported_feet, 2);
        let terrain = state.world.planet_terrain(0).unwrap();
        let local = terrain.cell_center(feet[0].unwrap());
        assert_ne!(
            terrain.cell(feet[0].unwrap()).unwrap().material,
            MaterialId::VOID
        );
        let planet = state.world.planets[0];
        release(
            &mut state,
            planet.position + local.rotate_radians(planet.wrapper_angle),
            4,
            8.0,
        );
        idle(&mut state, 1);
        assert!(!state.vehicle_settled(0));
        assert_eq!(state.try_transfer(0), TransferResult::ShipNotSettled);
        assert_conserved(&state, initial);
    }
}

#[test]
fn deposited_flag_footing_must_be_claimed_again_after_destruction_and_return() {
    for (shape, slumping) in [GrainShape::Round, GrainShape::Hexagon]
        .into_iter()
        .flat_map(|shape| [false, true].map(|enabled| (shape, enabled)))
    {
        let (mut state, released, initial) = rebuilt_pad(shape, slumping);
        let spec = SurfaceSortieState::spec();
        let hit = ground(&state, 0.0);
        let standing = hit.point + Vec2::Y * (spec.half_height() + 0.05);
        state.pilots[0].body = Some(
            SpacelingAssembly::insert(
                &mut state.world.physics.world,
                pilot_physics_id(PlayerId::PLAYER_1),
                standing,
                0.0,
                spec,
            )
            .unwrap(),
        );
        state.pilots[0].controls_armed = true;
        idle(&mut state, 360);
        let claim = state.claim_observation(0, 0).unwrap();
        assert_eq!(
            claim.owner,
            Some(PlayerId::PLAYER_1),
            "{shape:?}: {claim:?}"
        );
        assert_eq!(claim.captures, 1);
        let flag = claim.flag.unwrap();
        let planet = state.world.planets[0];
        let terrain = &state.world.terrain.planets[&0];
        let cell = terrain
            .geometry
            .contact_cell(
                &terrain.field,
                (flag.position - planet.position).rotate_radians(-planet.wrapper_angle),
                flag.normal.rotate_radians(-planet.wrapper_angle),
            )
            .unwrap();
        assert!(
            released.contains(&cell),
            "claim must stand on deposited material"
        );
        let local = terrain.field.cell_center(cell);
        let returned = state.terrain_diagnostics().deposited_cells;

        // Keep the claimant away while material returns: deposition must never
        // restore ownership merely because the old flag's footing reappears.
        let body = state.pilots[0].body.as_ref().unwrap().body();
        state
            .world
            .physics
            .world
            .set_pose(body, planet.position + Vec2::Y * 1000.0, 0.0, true);
        state
            .world
            .physics
            .world
            .set_velocity(body, Vec2::ZERO, 0.0, true);
        state.world.queue_test_blast(
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 0,
                },
                mode: EditMode::Remove,
            },
            RadialImpulse {
                center: local,
                radius: 2.0,
                speed: 0.0,
            },
        );
        idle(&mut state, 1);
        let destroyed = state.claim_observation(0, 0).unwrap();
        assert_eq!(destroyed.owner, None);
        assert!(destroyed.flag.is_none());
        assert_eq!(destroyed.neutralizations, 1);
        for _ in 0..600 {
            idle(&mut state, 1);
            let claim = state.claim_observation(0, 0).unwrap();
            assert_eq!(claim.owner, None, "{shape:?}: {claim:?}");
            assert!(claim.flag.is_none());
        }
        assert!(state.terrain_diagnostics().deposited_cells > returned);
        assert_ne!(
            state
                .world
                .planet_terrain(0)
                .unwrap()
                .cell(cell)
                .unwrap()
                .material,
            MaterialId::VOID
        );
        assert_conserved(&state, initial);

        state
            .world
            .physics
            .world
            .set_pose(body, standing, 0.0, true);
        state
            .world
            .physics
            .world
            .set_velocity(body, Vec2::ZERO, 0.0, true);
        idle(&mut state, 60);
        assert_eq!(state.claim_observation(0, 0).unwrap().owner, None);
        idle(&mut state, 300);
        let reclaimed = state.claim_observation(0, 0).unwrap();
        assert_eq!(
            reclaimed.owner,
            Some(PlayerId::PLAYER_1),
            "{shape:?}: {reclaimed:?}"
        );
        assert_eq!(reclaimed.captures, 2);
        assert_eq!(reclaimed.neutralizations, 1);
        assert_eq!(reclaimed.flag.unwrap().raised_fraction, 1.0);
        assert_conserved(&state, initial);
    }
}
