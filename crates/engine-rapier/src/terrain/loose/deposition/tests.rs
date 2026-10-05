use super::*;
use crate::world::{BodyKind, PhysicsWorldConfig};
use engine_terrain::{Brush, EditMode, Material};

const DT: f32 = 1.0 / 60.0;
const CELL: CellCoord = CellCoord::new(6, 5);

fn fixture(
    surface: TerrainSurface,
    shape: GrainShape,
) -> (PhysicsWorld, TerrainFragment, LooseTerrain) {
    let mut terrain = Terrain::generate(
        13,
        13,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c.y <= 5 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    terrain
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CELL,
                radius: 0,
            },
            mode: EditMode::Damage(27),
        })
        .unwrap();
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -10.0),
        ..Default::default()
    });
    let source = TerrainFragment::insert(
        &mut world,
        PhysicsId::new(1),
        DetachedTerrain {
            terrain,
            parent_offset: Vec2::ZERO,
        },
        BodyMotion {
            position: Vec2::ZERO,
            angle: 0.0,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
        },
        Vec2::ZERO,
        TerrainSpec {
            surface,
            ..Default::default()
        },
    )
    .unwrap();
    world.set_body_kind(source.assembly.body(), BodyKind::Fixed, true);
    (
        world,
        source,
        LooseTerrain::new(LooseTerrainConfig {
            max_grains: 1,
            shape,
            ..Default::default()
        })
        .unwrap(),
    )
}

fn view(f: &mut TerrainFragment) -> TerrainBodyMut<'_> {
    TerrainBodyMut {
        terrain: &mut f.terrain,
        geometry: &mut f.geometry,
        assembly: &mut f.assembly,
    }
}
fn release(
    world: &mut PhysicsWorld,
    field: &mut TerrainFragment,
    pool: &mut LooseTerrain,
    next: &mut u64,
) {
    let plan = PreparedRelease::new(
        &field.terrain,
        &[TerrainEdit {
            brush: Brush::Circle {
                center: CELL,
                radius: 0,
            },
            mode: EditMode::Remove,
        }],
    )
    .unwrap();
    assert_eq!(plan.grain_count(), 1);
    assert!(
        pool.commit(world, view(field), plan, next)
            .unwrap()
            .fragments
            .is_empty()
    );
}
fn step(
    world: &mut PhysicsWorld,
    field: &mut TerrainFragment,
    pool: &mut LooseTerrain,
) -> Vec<DepositCommit> {
    world.step(DT);
    pool.settle(world, [view(field)], DT).unwrap()
}

#[test]
fn repeated_release_settle_release_conserves_damage_and_reuses_a_single_slot() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        for shape in [GrainShape::Round, GrainShape::Hexagon] {
            let (mut world, mut field, mut pool) = fixture(surface, shape);
            let original = field.terrain.cells().to_vec();
            let mut next = 2;
            for cycle in 1..=5 {
                release(&mut world, &mut field, &mut pool, &mut next);
                let id = pool.iter().next().unwrap().id();
                let mut retired = Vec::new();
                for _ in 0..180 {
                    retired.extend(
                        step(&mut world, &mut field, &mut pool)
                            .into_iter()
                            .flat_map(|c| c.retired_grains),
                    );
                }
                assert_eq!(retired, [id], "{surface:?} {shape:?} cycle {cycle}");
                assert!(pool.is_empty());
                assert_eq!(pool.deposited_cells(), cycle);
                assert_eq!(field.terrain.cells(), original);
                assert!(field.geometry.is_current(&field.terrain));
                assert_eq!(world.body_count(), 1);
                assert!(!world.contains_entity(id));
                pool.audit(&world).unwrap();
            }
        }
    }
}

#[test]
fn airborne_and_fast_sliding_grains_never_pack_and_clone_keeps_quiet_time() {
    let (mut world, mut field, mut pool) = fixture(TerrainSurface::Blocks, GrainShape::Round);
    release(&mut world, &mut field, &mut pool, &mut 2);
    let grain = pool.iter().next().unwrap().body();
    world.set_pose(grain, Vec2::new(0.0, 20.0), 0.0, true);
    world.set_gravity_scale(grain, 0.0, true);
    for _ in 0..90 {
        step(&mut world, &mut field, &mut pool);
    }
    assert_eq!(pool.len(), 1);
    assert!(pool.settling.is_empty());
    world.set_pose(grain, field.terrain.cell_center(CELL), 0.0, true);
    world.set_gravity_scale(grain, 1.0, true);
    for _ in 0..60 {
        world.set_pose(grain, field.terrain.cell_center(CELL), 0.0, true);
        world.step(DT);
        world.set_velocity(grain, Vec2::new(2.0, 0.0), 0.0, true);
        pool.settle(&mut world, [view(&mut field)], DT).unwrap();
    }
    assert_eq!(pool.len(), 1);
    assert!(pool.settling.is_empty());
    world.set_velocity(grain, Vec2::ZERO, 0.0, true);
    for _ in 0..15 {
        step(&mut world, &mut field, &mut pool);
    }
    assert!(!pool.settling.is_empty());
    assert_eq!(pool.len(), 1);
    let (mut replay_world, mut replay_field, mut replay_pool) =
        (world.clone(), field.clone(), pool.clone());
    for _ in 0..90 {
        step(&mut world, &mut field, &mut pool);
        step(&mut replay_world, &mut replay_field, &mut replay_pool);
        assert_eq!(pool.settling_hash(), replay_pool.settling_hash());
        assert_eq!(pool.len(), replay_pool.len());
        assert_eq!(field.terrain, replay_field.terrain);
    }
    assert!(pool.is_empty());
}

#[test]
fn occupied_space_blocks_addition_even_before_the_next_broadphase_update() {
    let (mut world, mut field, mut pool) = fixture(TerrainSurface::Blocks, GrainShape::Round);
    release(&mut world, &mut field, &mut pool, &mut 2);
    for _ in 0..28 {
        step(&mut world, &mut field, &mut pool);
    }
    let blocker = PhysicsId::new(99);
    let center = field.terrain.cell_center(CELL);
    // Tiny obstacle in a corner of the future square, outside the round grain.
    world.insert_body(
        BodyId::new(blocker, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: center + Vec2::new(0.43, 0.43),
            ..Default::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(blocker, ColliderRole::PRIMARY, 0),
            0.03,
        )],
    );
    assert!(!world.current_cuboid_is_clear(
        center,
        0.0,
        Vec2::new(0.5, 0.5),
        CollisionGroups::ALL,
        &[field.id, pool.iter().next().unwrap().id()]
    ));
    let before = field.terrain.clone();
    for _ in 0..60 {
        step(&mut world, &mut field, &mut pool);
    }
    assert_eq!(pool.len(), 1);
    assert_eq!(field.terrain, before);
    let world_before = world.snapshot_bytes().unwrap();
    let hash_before = pool.settling_hash();
    let captures = pool.capture_packing(&world, [view(&mut field)]).unwrap();
    assert_eq!(captures.len(), 1);
    let inspection = PackingSnapshot::from_bytes(&captures[0].to_bytes().unwrap())
        .unwrap()
        .inspect()
        .unwrap();
    assert!(inspection.accepted.is_empty());
    assert!(
        inspection
            .attempts
            .iter()
            .flat_map(|a| &a.patches)
            .flat_map(|p| &p.blockers)
            .any(|id| id.entity == blocker)
    );
    assert_eq!(world.snapshot_bytes().unwrap(), world_before);
    assert_eq!(pool.settling_hash(), hash_before);
    assert_eq!(field.terrain, before);
    world.remove_entity(blocker);
    for _ in 0..60 {
        step(&mut world, &mut field, &mut pool);
    }
    assert!(pool.is_empty());
}

#[test]
fn fast_translating_rotating_support_can_accept_resting_material() {
    let (mut world, mut field, mut pool) = fixture(TerrainSurface::Blocks, GrainShape::Round);
    world.set_body_kind(field.assembly.body(), BodyKind::KinematicVelocity, true);
    world.set_velocity(field.assembly.body(), Vec2::new(12.0, 0.0), 0.1, true);
    release(&mut world, &mut field, &mut pool, &mut 2);
    let mut replay = (world.clone(), field.clone(), pool.clone());
    for _ in 0..180 {
        step(&mut world, &mut field, &mut pool);
        step(&mut replay.0, &mut replay.1, &mut replay.2);
        assert_eq!(pool.settling_hash(), replay.2.settling_hash());
        assert_eq!(field.terrain, replay.1.terrain);
    }
    assert_eq!(pool.deposited_cells(), 1);
    assert!(pool.is_empty());
    let motion = world.motion(field.assembly.body()).unwrap();
    assert!(motion.linear_velocity.x > 10.0 && motion.angle > 0.2);
}

#[test]
fn dynamic_attachment_preserves_linear_and_angular_momentum() {
    let (mut world, mut field, mut pool) = fixture(TerrainSurface::Blocks, GrainShape::Round);
    world.set_body_kind(field.assembly.body(), BodyKind::Dynamic, true);
    world.set_velocity(field.assembly.body(), Vec2::new(3.0, 1.0), 0.03, true);
    release(&mut world, &mut field, &mut pool, &mut 2);
    let mut attached = false;
    for _ in 0..180 {
        world.step(DT);
        let mut before = Momentum::default();
        before.add(&world, field.assembly.body());
        for grain in pool.iter() {
            before.add(&world, grain.body());
        }
        let commits = pool.settle(&mut world, [view(&mut field)], DT).unwrap();
        if !commits.is_empty() {
            let mut after = Momentum::default();
            after.add(&world, field.assembly.body());
            for i in 0..2 {
                assert!((before.linear[i] - after.linear[i]).abs() < 0.001);
            }
            assert!((before.angular - after.angular).abs() < 0.01);
            attached = true;
            break;
        }
    }
    assert!(attached);
}

#[test]
fn blocked_candidates_cannot_starve_later_grains_under_the_per_tick_budget() {
    let terrain = Terrain::generate(
        98,
        // No spare row above the blockers: local redistribution cannot bypass
        // them, so admission really must advance to later candidates.
        2,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c.y <= 1 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let geometry = TerrainGeometry::new(&terrain);
    let mut world = PhysicsWorld::new(Default::default());
    let assembly = TerrainAssembly::insert(
        &mut world,
        PhysicsId::new(1),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &terrain,
        &geometry,
        TerrainSpec::default(),
    )
    .unwrap();
    let mut field = TerrainFragment {
        id: PhysicsId::new(1),
        hash: terrain.hash(),
        terrain,
        geometry,
        assembly,
        edited_chunks: Vec::new(),
    };
    let mut pool = LooseTerrain::new(LooseTerrainConfig {
        max_grains: 96,
        ..Default::default()
    })
    .unwrap();
    let plan = PreparedRelease::new(
        &field.terrain,
        &[TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(0, 1),
                end: CellCoord::new(95, 1),
                radius: 0,
            },
            mode: EditMode::Remove,
        }],
    )
    .unwrap();
    assert_eq!(plan.grain_count(), 96);
    pool.commit(&mut world, view(&mut field), plan, &mut 2)
        .unwrap();
    for x in 0..64 {
        let id = PhysicsId::new(1000 + x as u64);
        world.insert_body(
            BodyId::new(id, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: field.terrain.cell_center(CellCoord::new(x, 1)) + Vec2::new(0.43, 0.43),
                ..Default::default()
            },
            &[ColliderSpec::ball(
                ColliderId::new(id, ColliderRole::PRIMARY, 0),
                0.03,
            )],
        );
    }
    for _ in 0..10 {
        world.step(0.25);
        let commits = pool.settle(&mut world, [view(&mut field)], 0.25).unwrap();
        assert!(
            commits
                .iter()
                .map(|c| c.retired_grains.len())
                .sum::<usize>()
                <= MAX_DEPOSITS
        );
        assert_eq!(pool.len() as u64 + pool.deposited_cells(), 96);
    }
    assert_eq!(pool.deposited_cells(), 32);
    assert_eq!(pool.len(), 64);
}

#[test]
fn smooth_surface_clearance_checks_growth_without_rejecting_unchanged_ground() {
    for surface in [TerrainSurface::Contour, TerrainSurface::Interpolated] {
        for obstructs in [false, true] {
            let (mut world, mut field, mut pool) = fixture(surface, GrainShape::Round);
            release(&mut world, &mut field, &mut pool, &mut 2);
            for _ in 0..28 {
                step(&mut world, &mut field, &mut pool);
            }
            let blocker = PhysicsId::new(99);
            let offset = if obstructs {
                Vec2::new(0.43, 0.43)
            } else {
                Vec2::new(1.1, 0.7)
            };
            world.insert_body(
                BodyId::new(blocker, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: field.terrain.cell_center(CELL) + offset,
                    ..Default::default()
                },
                &[ColliderSpec::ball(
                    ColliderId::new(blocker, ColliderRole::PRIMARY, 0),
                    0.03,
                )],
            );
            let before = field.terrain.clone();
            for _ in 0..120 {
                step(&mut world, &mut field, &mut pool);
            }
            assert_eq!(
                pool.len(),
                usize::from(obstructs),
                "{surface:?} obstructs={obstructs}"
            );
            assert!(world.contains_entity(blocker));
            if obstructs {
                assert_eq!(field.terrain, before);
                assert_eq!(pool.settling_diagnostics().obstructed, 1);
            } else {
                assert_eq!(pool.deposited_cells(), 1);
                assert_eq!(field.terrain.cell(CELL).unwrap().durability, 73);
            }
        }
    }
}

#[test]
fn grains_on_an_actor_cannot_use_it_as_a_path_to_terrain() {
    let (mut world, mut field, mut pool) = fixture(TerrainSurface::Blocks, GrainShape::Round);
    release(&mut world, &mut field, &mut pool, &mut 2);
    let center = field.terrain.cell_center(CELL);
    let actor = PhysicsId::new(99);
    world.insert_body(
        BodyId::new(actor, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: center + Vec2::new(0.0, 1.0),
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(actor, ColliderRole::PRIMARY, 0),
            2.0,
            0.25,
        )],
    );
    let grain = pool.iter().next().unwrap().body();
    world.set_pose(grain, center + Vec2::new(0.0, 1.75), 0.0, true);
    let before = field.terrain.clone();
    for _ in 0..180 {
        step(&mut world, &mut field, &mut pool);
    }
    assert_eq!(pool.len(), 1);
    assert_eq!(pool.settling_diagnostics().unsupported, 1);
    assert_eq!(field.terrain, before);
}

#[test]
fn compact_groups_keep_every_cells_damage_across_all_surfaces_and_shapes() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        for shape in [GrainShape::Round, GrainShape::Hexagon] {
            let (mut world, mut field, _) = fixture(surface, shape);
            let mut pool = LooseTerrain::new(LooseTerrainConfig {
                max_grains: 5,
                shape,
                ..Default::default()
            })
            .unwrap();
            let plan = PreparedRelease::new(
                &field.terrain,
                &[TerrainEdit {
                    brush: Brush::Capsule {
                        start: CellCoord::new(4, 5),
                        end: CellCoord::new(8, 5),
                        radius: 0,
                    },
                    mode: EditMode::Remove,
                }],
            )
            .unwrap();
            assert_eq!(plan.grain_count(), 5);
            pool.commit(&mut world, view(&mut field), plan, &mut 2)
                .unwrap();
            let center = field.terrain.cell_center(CELL);
            for (i, g) in pool.iter().enumerate() {
                let offset = if i < 3 {
                    Vec2::new((i as f32 - 1.0) * 0.95, 0.0)
                } else {
                    Vec2::new((i as f32 - 3.5) * 0.95, 0.9)
                };
                world.set_pose(g.body(), center + offset, 0.0, true);
            }
            let mut replay = (world.clone(), field.clone(), pool.clone());
            for _ in 0..600 {
                step(&mut world, &mut field, &mut pool);
                step(&mut replay.0, &mut replay.1, &mut replay.2);
                assert_eq!(pool.settling_hash(), replay.2.settling_hash());
                assert_eq!(field.terrain, replay.1.terrain);
                pool.audit(&world).unwrap();
            }
            assert_eq!(
                pool.deposited_cells(),
                5,
                "{surface:?} {shape:?}: {:?}",
                pool.settling_diagnostics()
            );
            assert_eq!(
                field
                    .terrain
                    .cells()
                    .iter()
                    .filter(|c| c.durability == 73)
                    .count(),
                1
            );
            assert_eq!(world.body_count(), 1);
        }
    }
}
