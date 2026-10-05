use super::*;
use crate::world::{BodyKind, PhysicsWorldConfig};
use engine_terrain::Material;

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
    let mut terrain = Terrain::generate(
        25,
        25,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c.y <= 5 || (c.x <= 10 && c.y <= 17) {
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
                center: CellCoord::new(10, 17),
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
fn config() -> LooseTerrainConfig {
    LooseTerrainConfig {
        slumping: Some(SlumpingConfig::default()),
        ..Default::default()
    }
}
fn quantity(field: &TerrainFragment, pool: &LooseTerrain) -> (usize, u64) {
    let cells = field
        .terrain
        .cells()
        .iter()
        .copied()
        .filter(|c| c.material != MaterialId::VOID)
        .chain(pool.iter().map(|g| g.cell()));
    cells.fold((0, 0), |(n, durability), c| {
        (n + 1, durability + u64::from(c.durability))
    })
}

#[test]
fn undisturbed_banks_and_disturbed_flat_surfaces_stay_solid() {
    let (mut world, mut field, mut pool) = fixture(0.0, config());
    let initial = field.terrain.clone();
    let mut next = 2;
    for tick in 0..120 {
        if tick == 60 {
            pool.disturb(
                field.assembly.body(),
                &field.terrain,
                [CellCoord::new(19, 5)],
            );
        }
        assert!(
            pool.slump(
                &mut world,
                [view(&mut field)],
                |_, _| Vec2::new(0.0, -10.0),
                &mut next,
                8,
                DT
            )
            .unwrap()
            .is_none()
        );
    }
    assert!(initial == field.terrain);
    assert_eq!(next, 2);
    assert_eq!(pool.slumping_diagnostics().pending, 0);
}

#[test]
fn bank_collapses_gradually_and_conserves_damaged_material_with_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let (mut world, mut field, mut pool) =
            fixture(0.0, LooseTerrainConfig { shape, ..config() });
        let initial = quantity(&field, &pool);
        let mut fragments = Vec::<TerrainFragment>::new();
        pool.disturb(
            field.assembly.body(),
            &field.terrain,
            [CellCoord::new(10, 17)],
        );
        let mut next = 2;
        let mut lowest = f32::MAX;
        for tick in 0..600 {
            let capacity = 8usize.saturating_sub(fragments.len());
            world.step(DT);
            let commit = pool
                .slump(
                    &mut world,
                    std::iter::once(view(&mut field)).chain(fragments.iter_mut().map(view)),
                    |_, _| Vec2::new(0.0, -10.0),
                    &mut next,
                    capacity,
                    DT,
                )
                .unwrap();
            if tick < 20 {
                assert!(commit.is_none());
            }
            if let Some(commit) = commit {
                fragments.extend(commit.release.fragments);
                assert!(commit.release.new_grains <= 4);
            }
            for g in pool.iter() {
                lowest = lowest.min(world.motion(g.body()).unwrap().position.y);
            }
            pool.settle(
                &mut world,
                std::iter::once(view(&mut field)).chain(fragments.iter_mut().map(view)),
                DT,
            )
            .unwrap();
            let mut actual = quantity(&field, &pool);
            for fragment in &fragments {
                for cell in fragment
                    .terrain
                    .cells()
                    .iter()
                    .filter(|c| c.material != MaterialId::VOID)
                {
                    actual.0 += 1;
                    actual.1 += u64::from(cell.durability);
                }
            }
            assert_eq!(actual, initial);
            assert!(pool.slumping_diagnostics().scanned <= 64);
            assert!(pool.slumping_diagnostics().pending <= 4096);
        }
        assert!(pool.slumping_diagnostics().released_cells >= 4);
        assert!(lowest < -5.0, "released grains fall down the bank");
        assert!(!occupied(&field.terrain, CellCoord::new(10, 17)));
        pool.audit(&world).unwrap();
    }
}

#[test]
fn rotated_gravity_and_source_motion_preserve_local_yield_and_clone_continuation() {
    let mut hashes = Vec::new();
    for angle in [0.0, 1.1] {
        let (mut world, mut field, mut pool) = fixture(angle, config());
        let body = field.assembly.body();
        world.set_body_kind(body, BodyKind::KinematicVelocity, true);
        world.set_velocity(body, Vec2::new(2.0, 3.0), 0.3, true);
        let mut next = 2;
        pool.disturb(body, &field.terrain, [CellCoord::new(10, 17)]);
        for _ in 0..20 {
            pool.slump(
                &mut world,
                [view(&mut field)],
                |_, _| Vec2::new(0.0, -10.0).rotate_radians(angle),
                &mut next,
                8,
                DT,
            )
            .unwrap();
        }
        let (mut replay_world, mut replay_field, mut replay_pool, mut replay_next) =
            (world.clone(), field.clone(), pool.clone(), next);
        for _ in 0..20 {
            for (world, field, pool, next) in [
                (&mut world, &mut field, &mut pool, &mut next),
                (
                    &mut replay_world,
                    &mut replay_field,
                    &mut replay_pool,
                    &mut replay_next,
                ),
            ] {
                pool.slump(
                    world,
                    [view(field)],
                    |_, _| Vec2::new(0.0, -10.0).rotate_radians(angle),
                    next,
                    8,
                    DT,
                )
                .unwrap();
            }
            assert_eq!(pool.settling_hash(), replay_pool.settling_hash());
            assert_eq!(field.terrain, replay_field.terrain);
            assert_eq!(next, replay_next);
        }
        assert!(!pool.is_empty());
        for grain in pool.iter() {
            let m = world.motion(grain.body()).unwrap();
            assert!(
                m.linear_velocity
                    .distance_to(world.velocity_at_point(body, m.position).unwrap())
                    < 0.0001
            );
            assert_eq!(m.angular_velocity, 0.3);
        }
        hashes.push(field.terrain.hash());
    }
    assert_eq!(hashes[0], hashes[1]);
}

#[test]
fn capacity_and_tracking_limits_retain_material_and_allocator() {
    let (mut world, mut field, mut pool) = fixture(
        0.0,
        LooseTerrainConfig {
            max_grains: 1,
            ..config()
        },
    );
    pool.disturb(
        field.assembly.body(),
        &field.terrain,
        [CellCoord::new(10, 17)],
    );
    let mut next = 2;
    let plan = PreparedRelease::new(
        &field.terrain,
        &[TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(19, 5),
                radius: 0,
            },
            mode: EditMode::Remove,
        }],
    )
    .unwrap();
    pool.commit(&mut world, view(&mut field), plan, &mut next)
        .unwrap();
    let initial = field.terrain.clone();
    let initial_next = next;
    let mut blocked = false;
    for _ in 0..60 {
        pool.slump(
            &mut world,
            [view(&mut field)],
            |_, _| Vec2::new(0.0, -10.0),
            &mut next,
            0,
            DT,
        )
        .unwrap();
        blocked |= pool.slumping_diagnostics().capacity_blocked > 0;
    }
    assert!(blocked);
    assert!(initial == field.terrain);
    assert_eq!(next, initial_next);
    let mut pool = LooseTerrain::new(LooseTerrainConfig {
        slumping: Some(SlumpingConfig {
            max_tracked: 3,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    pool.disturb(
        field.assembly.body(),
        &field.terrain,
        [CellCoord::new(10, 17)],
    );
    assert_eq!(pool.slumping_diagnostics().pending, 3);
    assert!(pool.slumping_diagnostics().tracking_overflow > 0);
    assert!(initial == field.terrain);
}

#[test]
fn yielding_a_corner_cannot_publish_unadmitted_fragments() {
    let terrain = Terrain::generate(
        25,
        25,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c.y <= 2 || (c.x == 10 && c.y <= 11) || (c.y == 11 && (3..=10).contains(&c.x)) {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let mut world = PhysicsWorld::new(Default::default());
    let mut field = TerrainFragment::insert(
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
        TerrainSpec::default(),
    )
    .unwrap();
    let mut pool = LooseTerrain::new(config()).unwrap();
    pool.disturb(
        field.assembly.body(),
        &field.terrain,
        [CellCoord::new(10, 11)],
    );
    let initial = field.terrain.clone();
    let mut next = 2;
    for _ in 0..60 {
        assert!(
            pool.slump(
                &mut world,
                [view(&mut field)],
                |_, _| Vec2::new(0.0, -10.0),
                &mut next,
                0,
                DT
            )
            .unwrap()
            .is_none()
        );
    }
    assert!(pool.slumping_diagnostics().fragment_denials > 0);
    assert!(initial == field.terrain);
    assert_eq!(next, 2);
    assert_eq!(world.body_count(), 1);
    for _ in 0..60 {
        if let Some(commit) = pool
            .slump(
                &mut world,
                [view(&mut field)],
                |_, _| Vec2::new(0.0, -10.0),
                &mut next,
                1,
                DT,
            )
            .unwrap()
        {
            assert_eq!(commit.release.fragments.len(), 1);
            let actual = quantity(&field, &pool).0
                + commit.release.fragments[0]
                    .terrain
                    .cells()
                    .iter()
                    .filter(|c| c.material != MaterialId::VOID)
                    .count();
            assert_eq!(
                actual,
                initial
                    .cells()
                    .iter()
                    .filter(|c| c.material != MaterialId::VOID)
                    .count()
            );
            return;
        }
    }
    panic!("admitted corner should yield");
}
