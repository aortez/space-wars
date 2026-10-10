use super::*;
use crate::world::PhysicsWorldConfig;
use engine_terrain::{Brush, EditMode, Material};

const DT: f32 = 1.0 / 60.0;
fn view(f: &mut TerrainFragment) -> TerrainBodyMut<'_> {
    TerrainBodyMut {
        terrain: &mut f.terrain,
        geometry: &mut f.geometry,
        assembly: &mut f.assembly,
    }
}
fn fixture(
    angle: f32,
    config: LooseTerrainConfig,
) -> (PhysicsWorld, TerrainFragment, LooseTerrain) {
    shaped_fixture(angle, config, false)
}
fn shaped_fixture(
    angle: f32,
    config: LooseTerrainConfig,
    irregular: bool,
) -> (PhysicsWorld, TerrainFragment, LooseTerrain) {
    // Deliberately smaller bearing bank: selecting the largest component would
    // retain the wrong side. Damaged seam and chunk cells must remain damaged.
    let mut terrain = Terrain::generate(
        30,
        24,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            let bottom = if irregular { c.x } else { 11 };
            if (c.x == 10 && (8..=if irregular { 20 } else { 12 }).contains(&c.y))
                || ((11..=16).contains(&c.x) && (bottom..=bottom + 1).contains(&c.y))
            {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    terrain
        .apply(TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(11, 12),
                end: CellCoord::new(16, 12),
                radius: 0,
            },
            mode: EditMode::Damage(27),
        })
        .unwrap();
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -10.0).rotate_radians(angle),
        ..Default::default()
    });
    let field = TerrainFragment::insert(
        &mut world,
        PhysicsId::new(1),
        DetachedTerrain {
            terrain,
            parent_offset: Vec2::ZERO,
        },
        BodyMotion {
            position: Vec2::new(4.0, -3.0),
            angle,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
        },
        Vec2::new(4.0, -3.0),
        TerrainSpec::default(),
    )
    .unwrap();
    world.set_body_kind(field.assembly.body(), BodyKind::Fixed, true);
    (world, field, LooseTerrain::new(config).unwrap())
}

#[test]
fn irregular_rotating_banks_release_through_the_same_atomic_material_boundary() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for angle in [0.4, 1.7] {
            let (mut world, mut field, mut pool) =
                shaped_fixture(angle, LooseTerrainConfig { shape, ..config() }, true);
            let initial = quantity(&field.terrain);
            pool.disturb(
                field.assembly.body(),
                &field.terrain,
                [CellCoord::new(14, 13)],
            );
            let (mut replay_world, mut replay_field, mut replay_pool) =
                (world.clone(), field.clone(), pool.clone());
            let (mut next, mut replay_next) = (2, 2);
            let mut fragments = Vec::new();
            for _ in 0..90 {
                let commit = tick(&mut world, &mut field, &mut pool, &mut next, 1, angle);
                let replay = tick(
                    &mut replay_world,
                    &mut replay_field,
                    &mut replay_pool,
                    &mut replay_next,
                    1,
                    angle,
                );
                assert_eq!(commit.is_some(), replay.is_some());
                assert_eq!(field.terrain, replay_field.terrain);
                assert_eq!(pool.settling_hash(), replay_pool.settling_hash());
                if let Some(commit) = commit {
                    fragments.extend(commit.release.fragments);
                }
            }
            assert_eq!(pool.cohesion_diagnostics().irregular_failures, 1);
            assert_eq!(fragments.len(), 1);
            assert_eq!(quantity(&field.terrain).0, 13, "bearing retained");
            let mut remaining = quantity(&field.terrain);
            for fragment in fragments {
                let (n, durability) = quantity(&fragment.terrain);
                remaining.0 += n;
                remaining.1 += durability;
            }
            for grain in pool.iter() {
                remaining.0 += 1;
                remaining.1 += u64::from(grain.cell().durability);
            }
            assert_eq!(remaining, initial);
            pool.audit(&world).unwrap();
        }
    }
}
fn config() -> LooseTerrainConfig {
    LooseTerrainConfig {
        cohesion: Some(CohesionConfig::default()),
        ..Default::default()
    }
}

fn roof_fixture(
    config: LooseTerrainConfig,
    connected_floor: bool,
    angle: f32,
) -> (PhysicsWorld, TerrainFragment, LooseTerrain) {
    let (mut world, old, pool) = fixture(angle, config);
    let mut terrain = Terrain::generate(
        40,
        32,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            let filled = (connected_floor && c.y == 0)
                || (c.y <= 14 && (c.x <= 10 || c.x >= 25))
                || ((11..25).contains(&c.x) && (11..15).contains(&c.y));
            if filled && !(c.x == 14 && (11..13).contains(&c.y)) {
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
                center: CellCoord::new(14, 13),
                radius: 1,
            },
            mode: EditMode::Damage(27),
        })
        .unwrap();
    world.remove_entity(old.id);
    let field = TerrainFragment::insert(
        &mut world,
        old.id,
        DetachedTerrain {
            terrain,
            parent_offset: Vec2::ZERO,
        },
        BodyMotion {
            position: Vec2::ZERO,
            angle,
            linear_velocity: Vec2::new(2.0, 3.0),
            angular_velocity: 0.3,
        },
        Vec2::ZERO,
        TerrainSpec::default(),
    )
    .unwrap();
    world.set_body_kind(field.assembly.body(), BodyKind::KinematicVelocity, true);
    (world, field, pool)
}

#[test]
fn roof_crack_then_cantilever_release_conserve_cells_damage_and_motion() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for angle in [0.0, 1.1] {
            let (mut world, mut field, mut pool) =
                roof_fixture(LooseTerrainConfig { shape, ..config() }, true, angle);
            let initial = quantity(&field.terrain);
            pool.disturb(
                field.assembly.body(),
                &field.terrain,
                [CellCoord::new(14, 12)],
            );
            let (mut w2, mut f2, mut p2) = (world.clone(), field.clone(), pool.clone());
            let (mut next, mut n2) = (2, 2);
            let mut fragments = Vec::new();
            for _ in 0..120 {
                // A crack that leaves the floor connected needs no fragment slot.
                let capacity = usize::from(pool.cohesion_diagnostics().roof_failures > 0);
                let commit = tick(
                    &mut world, &mut field, &mut pool, &mut next, capacity, angle,
                );
                let replay = tick(&mut w2, &mut f2, &mut p2, &mut n2, capacity, angle);
                assert_eq!(commit.is_some(), replay.is_some());
                assert_eq!(pool.settling_hash(), p2.settling_hash());
                assert_eq!(field.terrain, f2.terrain);
                if let Some(commit) = commit {
                    if capacity == 0 {
                        assert!(commit.release.fragments.is_empty());
                    }
                    fragments.extend(commit.release.fragments);
                }
            }
            assert_eq!(pool.cohesion_diagnostics().roof_failures, 1);
            assert!(pool.cohesion_diagnostics().failures > 1);
            assert!(!fragments.is_empty());
            for x in [10, 25] {
                assert_ne!(
                    field.terrain.cell(CellCoord::new(x, 11)).unwrap().material,
                    MaterialId::VOID
                );
            }
            let mut remaining = quantity(&field.terrain);
            for fragment in &fragments {
                let q = quantity(&fragment.terrain);
                remaining.0 += q.0;
                remaining.1 += q.1;
            }
            for grain in pool.iter() {
                remaining.0 += 1;
                remaining.1 += u64::from(grain.cell().durability);
            }
            assert_eq!(remaining, initial);
            for id in pool
                .iter()
                .map(|g| g.body())
                .chain(fragments.iter().map(|f| f.assembly.body()))
            {
                let center = world.center_of_mass(id).unwrap();
                let motion = world.motion(id).unwrap();
                assert!(
                    motion.linear_velocity.distance_to(
                        world
                            .velocity_at_point(field.assembly.body(), center)
                            .unwrap()
                    ) < 0.0001
                );
                assert_eq!(motion.angular_velocity, 0.3);
            }
            pool.audit(&world).unwrap();
        }
    }
}

#[test]
fn roof_capacity_refusals_strong_disabled_and_free_falling_are_atomic() {
    for case in 0..5 {
        let mut config = config();
        if case == 0 {
            config.max_grains = 1;
        }
        if case == 2 {
            config.cohesion.as_mut().unwrap().strength = 4000.0;
        }
        if case == 3 {
            config.cohesion = None;
        }
        let (mut world, mut field, mut pool) = roof_fixture(config, case != 1, 0.0);
        if case == 4 {
            world.set_body_kind(field.assembly.body(), BodyKind::Dynamic, true);
        }
        let initial = field.terrain.clone();
        pool.disturb(
            field.assembly.body(),
            &field.terrain,
            [CellCoord::new(14, 12)],
        );
        let mut next = 2;
        for _ in 0..120 {
            assert!(tick(&mut world, &mut field, &mut pool, &mut next, 0, 0.0).is_none());
        }
        assert_eq!(field.terrain, initial);
        assert_eq!(next, 2);
        assert_eq!(world.body_count(), 1);
        let d = pool.cohesion_diagnostics();
        match case {
            0 => assert!(d.capacity_denials > 0),
            1 => {
                assert!(d.fragment_denials > 0);
                assert!(
                    (0..120).any(
                        |_| tick(&mut world, &mut field, &mut pool, &mut next, 1, 0.0).is_some()
                    )
                );
            }
            2 => assert!(d.roof_held_checks > 0),
            _ => assert_eq!(d.failures, 0),
        }
    }
}
fn tick(
    world: &mut PhysicsWorld,
    field: &mut TerrainFragment,
    pool: &mut LooseTerrain,
    next: &mut u64,
    capacity: usize,
    angle: f32,
) -> Option<SlumpCommit> {
    pool.fracture_banks(
        world,
        [view(field)],
        |_, _| Vec2::new(0.0, -10.0).rotate_radians(angle),
        next,
        capacity,
        DT,
    )
    .unwrap()
}
fn quantity(terrain: &Terrain) -> (usize, u64) {
    terrain
        .cells()
        .iter()
        .filter(|c| c.material != MaterialId::VOID)
        .fold((0, 0), |(n, d), c| (n + 1, d + u64::from(c.durability)))
}

#[test]
fn weak_bank_releases_one_chunk_and_seam_with_conserved_cells_and_inherited_motion() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for angle in [0.0, 1.1] {
            let (mut world, mut field, mut pool) =
                fixture(angle, LooseTerrainConfig { shape, ..config() });
            let initial = quantity(&field.terrain);
            let body = field.assembly.body();
            world.set_body_kind(body, BodyKind::KinematicVelocity, true);
            world.set_velocity(body, Vec2::new(2.0, 3.0), 0.3, true);
            pool.disturb(body, &field.terrain, [CellCoord::new(14, 10)]);
            let mut next = 2;
            let mut committed = None;
            for step in 0..90 {
                if let Some(commit) = tick(&mut world, &mut field, &mut pool, &mut next, 1, angle) {
                    assert!(step >= 29, "respect yield delay");
                    committed = Some(commit);
                    break;
                }
            }
            let commit = committed.expect("weak bank must fail");
            assert_eq!(commit.release.fragments.len(), 1);
            assert_eq!(pool.len(), 2);
            let chunk = &commit.release.fragments[0];
            assert_eq!(quantity(&field.terrain).0, 5, "bearing stays in source");
            let (mut n, mut durability) = quantity(&field.terrain);
            let fragment = quantity(&chunk.terrain);
            n += fragment.0;
            durability += fragment.1;
            for g in pool.iter() {
                n += 1;
                durability += u64::from(g.cell().durability);
            }
            assert_eq!((n, durability), initial);
            for id in pool.iter().map(|g| g.body()).chain([chunk.assembly.body()]) {
                let motion = world.motion(id).unwrap();
                let center = world.center_of_mass(id).unwrap();
                assert!(
                    motion
                        .linear_velocity
                        .distance_to(world.velocity_at_point(body, center).unwrap())
                        < 0.0001
                );
                assert_eq!(motion.angular_velocity, 0.3);
            }
            assert_eq!(pool.cohesion_diagnostics().failures, 1);
            assert_eq!(pool.cohesion_diagnostics().released_cells, 12);
            pool.audit(&world).unwrap();
        }
    }
}

#[test]
fn strong_undisturbed_disabled_and_free_falling_banks_hold() {
    for mode in 0..4 {
        let mut cfg = config();
        if mode == 0 {
            cfg.cohesion.as_mut().unwrap().strength = 1000.0;
        }
        if mode == 2 {
            cfg.cohesion = None;
        }
        let (mut world, mut field, mut pool) = fixture(0.0, cfg);
        let initial = field.terrain.clone();
        let body = field.assembly.body();
        if mode == 3 {
            world.set_body_kind(body, BodyKind::Dynamic, true);
        }
        if mode != 1 {
            pool.disturb(body, &field.terrain, [CellCoord::new(14, 10)]);
        }
        let mut next = 2;
        for _ in 0..120 {
            assert!(tick(&mut world, &mut field, &mut pool, &mut next, 1, 0.0).is_none());
        }
        assert_eq!(field.terrain, initial);
        assert_eq!(next, 2);
        if mode == 0 {
            assert!(pool.cohesion_diagnostics().held_checks > 0);
        }
    }
}

#[test]
fn capacity_refusal_is_atomic_and_retries_after_headroom_returns() {
    for grain_limit in [1, 192] {
        let (mut world, mut field, mut pool) = fixture(
            0.0,
            LooseTerrainConfig {
                max_grains: grain_limit,
                ..config()
            },
        );
        pool.disturb(
            field.assembly.body(),
            &field.terrain,
            [CellCoord::new(14, 10)],
        );
        let initial = field.terrain.clone();
        let mut next = 2;
        for _ in 0..120 {
            assert!(tick(&mut world, &mut field, &mut pool, &mut next, 0, 0.0).is_none());
        }
        assert_eq!(field.terrain, initial);
        assert_eq!(next, 2);
        assert_eq!(world.body_count(), 1);
        let d = pool.cohesion_diagnostics();
        if grain_limit == 1 {
            assert!(d.capacity_denials > 0);
        } else {
            assert!(d.fragment_denials > 0);
        }
        if grain_limit > 1 {
            assert!(
                (0..120)
                    .any(|_| tick(&mut world, &mut field, &mut pool, &mut next, 1, 0.0).is_some())
            );
        }
    }
}

#[test]
fn queued_failure_replays_after_clone_and_work_is_bounded() {
    let (mut world, mut field, mut pool) = fixture(
        0.0,
        LooseTerrainConfig {
            cohesion: Some(CohesionConfig {
                max_tracked: 3,
                max_scans: 2,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    pool.disturb(
        field.assembly.body(),
        &field.terrain,
        [CellCoord::new(14, 10)],
    );
    assert_eq!(pool.cohesion_diagnostics().pending, 3);
    assert!(pool.cohesion_diagnostics().tracking_overflow > 0);
    let mut next = 2;
    for _ in 0..10 {
        tick(&mut world, &mut field, &mut pool, &mut next, 1, 0.0);
    }
    let (mut w2, mut f2, mut p2, mut n2) = (world.clone(), field.clone(), pool.clone(), next);
    for _ in 0..120 {
        let a = tick(&mut world, &mut field, &mut pool, &mut next, 1, 0.0);
        let b = tick(&mut w2, &mut f2, &mut p2, &mut n2, 1, 0.0);
        assert_eq!(a.is_some(), b.is_some());
        assert_eq!(pool.settling_hash(), p2.settling_hash());
        assert_eq!(field.terrain, f2.terrain);
        assert_eq!(next, n2);
        assert!(pool.cohesion_diagnostics().scanned <= 2);
    }
    assert_eq!(pool.cohesion_diagnostics().failures, 1);
}
