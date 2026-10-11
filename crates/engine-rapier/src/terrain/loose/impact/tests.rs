use super::*;
use crate::world::{ColliderId, ColliderRole, ColliderSpec, PhysicsWorldConfig};
use engine_terrain::{CellCoord, EditMode, Material, TerrainEdit};

const DT: f32 = 1.0 / 60.0;
fn view(f: &mut TerrainFragment) -> TerrainBodyMut<'_> {
    TerrainBodyMut {
        terrain: &mut f.terrain,
        geometry: &mut f.geometry,
        assembly: &mut f.assembly,
    }
}
fn fixture(
    height: f32,
    angle: f32,
    config: LooseTerrainConfig,
) -> (PhysicsWorld, TerrainFragment, LooseTerrain) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -18.0).rotate_radians(angle),
        max_ccd_substeps: 4,
        ..Default::default()
    });
    let floor = PhysicsId::new(1);
    assert!(world.insert_body(
        BodyId::new(floor, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(0.0, -1.0).rotate_radians(angle),
            angle,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(floor, ColliderRole::PRIMARY, 0),
            20.0,
            1.0
        )]
    ));
    let mut terrain = Terrain::generate(
        12,
        8,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if (2..10).contains(&c.x) && (2..6).contains(&c.y) {
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
                start: CellCoord::new(2, 2),
                end: CellCoord::new(9, 2),
                radius: 0,
            },
            mode: EditMode::Damage(27),
        })
        .unwrap();
    let position = Vec2::new(0.0, 2.0 + height).rotate_radians(angle);
    let field = TerrainFragment::insert(
        &mut world,
        PhysicsId::new(2),
        DetachedTerrain {
            terrain,
            parent_offset: Vec2::ZERO,
        },
        BodyMotion {
            position,
            angle,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
        },
        position,
        TerrainSpec::default(),
    )
    .unwrap();
    (world, field, LooseTerrain::new(config).unwrap())
}
fn config() -> LooseTerrainConfig {
    LooseTerrainConfig {
        impact_fracture: Some(ImpactFractureConfig::default()),
        ..Default::default()
    }
}
fn solve(world: &mut PhysicsWorld, field: &TerrainFragment, pool: &mut LooseTerrain) {
    pool.capture_impact_motion(world, [(field.assembly.body(), &field.terrain)], DT);
    world.step(DT);
    pool.sample_impact_contacts(world);
}
fn quantity(terrain: &Terrain) -> (usize, u64) {
    terrain
        .cells()
        .iter()
        .filter(|c| c.material != MaterialId::VOID)
        .fold((0, 0), |(n, d), c| (n + 1, d + u64::from(c.durability)))
}

#[test]
fn hard_landings_chip_locally_preserve_damage_and_inherit_solved_motion() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for angle in [0.0, 0.7] {
            let (mut world, mut field, mut pool) =
                fixture(12.0, angle, LooseTerrainConfig { shape, ..config() });
            let initial = quantity(&field.terrain);
            let mut next = 100;
            for _ in 0..180 {
                solve(&mut world, &field, &mut pool);
                if pool.impact_diagnostics().pending > 0 {
                    break;
                }
            }
            assert!(
                pool.impact_diagnostics().pending > 0,
                "{:?}",
                pool.impact_diagnostics()
            );
            let hit = *pool.impact.pending.values().next().unwrap();
            assert!(hit.contact.position.y < -1.9);
            let before = world.motion(field.assembly.body()).unwrap();
            let center = world.center_of_mass(field.assembly.body()).unwrap();
            let commit = pool
                .fracture_impacts(&mut world, [view(&mut field)], &mut next, 64)
                .unwrap()
                .unwrap();
            let mut after = quantity(&field.terrain);
            for fragment in &commit.release.fragments {
                let q = quantity(&fragment.terrain);
                after.0 += q.0;
                after.1 += q.1;
            }
            for g in pool.iter() {
                after.0 += 1;
                after.1 += u64::from(g.cell().durability);
                let m = world.motion(g.body()).unwrap();
                let offset = world.center_of_mass(g.body()).unwrap() - center;
                let expected = before.linear_velocity
                    + Vec2::new(-offset.y, offset.x) * before.angular_velocity;
                assert!(
                    m.linear_velocity.distance_to(expected) < 1e-4,
                    "no added kick"
                );
                let local = (m.position - before.position).rotate_radians(-before.angle);
                assert!(
                    local.distance_to(hit.contact.position) < 4.6,
                    "bounded patch at the contact: {local:?}"
                );
            }
            assert_eq!(after, initial);
            assert!((1..=16).contains(&pool.len()));
            assert!(quantity(&field.terrain).0 > 0, "retain a clump");
            assert_eq!(pool.impact_diagnostics().failures, 1);
            pool.audit(&world).unwrap();
        }
    }
}

#[test]
fn soft_landings_strong_material_and_sustained_support_do_not_erode() {
    for (height, strength) in [(0.3, 6.0), (12.0, 24.0), (-0.02, 6.0)] {
        let (mut world, mut field, mut pool) = fixture(
            height,
            0.0,
            LooseTerrainConfig {
                impact_fracture: Some(ImpactFractureConfig {
                    yield_speed: strength,
                    ..Default::default()
                }),
                ..config()
            },
        );
        let initial = field.terrain.clone();
        let mut next = 100;
        for _ in 0..600 {
            solve(&mut world, &field, &mut pool);
            assert!(
                pool.fracture_impacts(&mut world, [view(&mut field)], &mut next, 64)
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(field.terrain, initial);
        assert_eq!(next, 100);
        assert!(pool.impact_diagnostics().held_checks > 0);
    }
}

#[test]
fn queued_impacts_clone_replay_and_stale_edits_are_discarded() {
    let (mut world, mut field, mut pool) = fixture(12.0, 0.0, config());
    for _ in 0..180 {
        solve(&mut world, &field, &mut pool);
        if !pool.impact.pending.is_empty() {
            break;
        }
    }
    assert!(!pool.impact.pending.is_empty());
    let (mut clone_world, mut clone_field, mut clone_pool) =
        (world.clone(), field.clone(), pool.clone());
    let (mut next, mut clone_next) = (100, 100);
    let a = pool
        .fracture_impacts(&mut world, [view(&mut field)], &mut next, 64)
        .unwrap()
        .unwrap();
    let b = clone_pool
        .fracture_impacts(
            &mut clone_world,
            [view(&mut clone_field)],
            &mut clone_next,
            64,
        )
        .unwrap()
        .unwrap();
    assert_eq!(a.release.new_grains, b.release.new_grains);
    assert_eq!(field.terrain, clone_field.terrain);
    assert_eq!(pool.settling_hash(), clone_pool.settling_hash());
    assert_eq!(world.snapshot_bytes(), clone_world.snapshot_bytes());
    assert_eq!(next, clone_next);
    // An intervening blast/mining edit must not fracture changed geometry from
    // an old local contact point, even if the source identity still exists.
    let (mut world, mut field, mut pool) = fixture(12.0, 0.0, config());
    for _ in 0..180 {
        solve(&mut world, &field, &mut pool);
        if !pool.impact.pending.is_empty() {
            break;
        }
    }
    field
        .terrain
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(2, 2),
                radius: 0,
            },
            mode: EditMode::Damage(1),
        })
        .unwrap();
    let edited = field.terrain.clone();
    assert!(
        pool.fracture_impacts(&mut world, [view(&mut field)], &mut next, 64)
            .unwrap()
            .is_none()
    );
    assert_eq!(field.terrain, edited);
    assert_eq!(pool.impact_diagnostics().stale_hits, 1);
}

#[test]
fn refused_hits_are_atomic_and_never_retried_after_capacity_returns() {
    for grain_limit in [1, 192] {
        let (mut world, mut field, mut pool) = fixture(
            0.0,
            0.0,
            LooseTerrainConfig {
                max_grains: grain_limit,
                ..config()
            },
        );
        // A central, deep strike splits this clump. Both budgets must admit
        // the entire transfer before any field, ID, body or velocity changes.
        let body = field.assembly.body();
        pool.impact.pending.insert(
            body,
            Hit {
                revision: field.terrain.revision(),
                contact: ContactPoint {
                    position: Vec2::new(0.0, -2.0),
                    normal: -Vec2::Y,
                },
                impulse: 640.0,
                strongest: 640.0,
                speed: 20.0,
                mass: 32.0,
            },
        );
        let before = field.terrain.clone();
        let physics = world.snapshot_bytes();
        let mut next = 100;
        assert!(
            pool.fracture_impacts(&mut world, [view(&mut field)], &mut next, 0)
                .unwrap()
                .is_none()
        );
        assert_eq!(field.terrain, before);
        assert_eq!(world.snapshot_bytes(), physics);
        assert_eq!(next, 100);
        let d = pool.impact_diagnostics();
        if grain_limit == 1 {
            assert_eq!(d.capacity_denials, 1);
        } else {
            assert_eq!(d.fragment_denials, 1);
        }
        pool.config.max_grains = 192;
        assert!(
            pool.fracture_impacts(&mut world, [view(&mut field)], &mut next, 64)
                .unwrap()
                .is_none()
        );
        for _ in 0..120 {
            solve(&mut world, &field, &mut pool);
            assert!(
                pool.fracture_impacts(&mut world, [view(&mut field)], &mut next, 64)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn translating_support_uses_prescribed_velocity_and_contact_work_is_bounded() {
    let (mut world, field, mut pool) = fixture(
        0.0,
        0.0,
        LooseTerrainConfig {
            impact_fracture: Some(ImpactFractureConfig {
                max_contacts: 1,
                ..Default::default()
            }),
            ..config()
        },
    );
    let floor = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    world.set_body_kind(floor, BodyKind::KinematicPosition, true);
    world.set_velocity(field.assembly.body(), Vec2::new(0.0, -20.0), 0.0, true);
    for i in 1..=10 {
        // The clump and support descend together. Absolute speed is above the
        // yield threshold; their relative contact speed is close to zero.
        world.set_next_kinematic_pose(floor, Vec2::new(0.0, -1.0 - i as f32 * 20.0 * DT), 0.0);
        solve(&mut world, &field, &mut pool);
        assert_eq!(pool.impact_diagnostics().pending, 0);
        assert!(pool.impact_diagnostics().scanned <= 1);
    }
    // A prescribed stationary endpoint overrides the previous solved velocity.
    let position = world.motion(floor).unwrap().position;
    world.set_next_kinematic_pose(floor, position, 0.0);
    assert_eq!(
        world.incoming_motion(floor, DT).unwrap().linear_velocity,
        Vec2::ZERO
    );
    world.set_next_kinematic_pose(floor, position, 0.1);
    assert!((world.incoming_motion(floor, DT).unwrap().angular_velocity - 6.0).abs() < 1e-4);
}
