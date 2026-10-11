use super::*;
use engine_terrain::{Brush, CellCoord, EditMode, Material};

fn fixture(surface: TerrainSurface) -> (PhysicsWorld, TerrainFragment) {
    let terrain = Terrain::generate(
        13,
        13,
        0.5,
        vec![
            Material {
                id: MaterialId(1),
                hardness: 100,
            },
            Material {
                id: MaterialId(2),
                hardness: 180,
            },
        ],
        |c| {
            if c.y > 8 {
                MaterialId::VOID
            } else if c.x < 6 {
                MaterialId(1)
            } else {
                MaterialId(2)
            }
        },
    )
    .unwrap();
    let mut world = PhysicsWorld::new(Default::default());
    let fragment = TerrainFragment::insert(
        &mut world,
        PhysicsId::new(1),
        DetachedTerrain {
            terrain,
            parent_offset: Vec2::ZERO,
        },
        BodyMotion {
            position: Vec2::new(7.0, -3.0),
            angle: 0.6,
            linear_velocity: Vec2::new(2.0, 1.0),
            angular_velocity: 0.4,
        },
        Vec2::new(7.0, -3.0),
        TerrainSpec {
            surface,
            ..Default::default()
        },
    )
    .unwrap();
    (world, fragment)
}
fn body(f: &mut TerrainFragment) -> TerrainBodyMut<'_> {
    TerrainBodyMut {
        terrain: &mut f.terrain,
        geometry: &mut f.geometry,
        assembly: &mut f.assembly,
    }
}
fn edit(work: u8) -> TerrainEdit {
    TerrainEdit {
        brush: Brush::Circle {
            center: CellCoord::new(6, 8),
            radius: 2,
        },
        mode: EditMode::Damage(work),
    }
}

#[test]
fn damage_releases_only_broken_material_with_source_motion_and_matching_surfaces() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        let (mut world, mut source) = fixture(surface);
        let initial = source
            .terrain
            .cells()
            .iter()
            .filter(|c| c.material != MaterialId::VOID)
            .count();
        let motion = world.motion(source.assembly.body()).unwrap();
        let center = world.center_of_mass(source.assembly.body()).unwrap();
        let mut loose = LooseTerrain::new(Default::default()).unwrap();
        let mut next = 2;
        let plan = PreparedRelease::new(&source.terrain, &[edit(160)]).unwrap();
        assert!(plan.grain_count() > 0);
        let committed = loose
            .commit(&mut world, body(&mut source), plan, &mut next)
            .unwrap();
        assert!(committed.fragments.is_empty());
        for grain in loose.iter() {
            assert_eq!(grain.cell().material, MaterialId(1));
            let actual = world.motion(grain.body()).unwrap();
            let offset = actual.position - center;
            assert!(
                actual.linear_velocity.distance_to(
                    motion.linear_velocity
                        + Vec2::new(-offset.y, offset.x) * motion.angular_velocity
                ) < 0.00001
            );
        }
        assert!(
            source
                .terrain
                .cells()
                .iter()
                .any(|c| c.material == MaterialId(2) && c.durability == 20)
        );
        assert!(source.geometry.is_current(&source.terrain));
        let plan = PreparedRelease::new(&source.terrain, &[edit(20)]).unwrap();
        loose
            .commit(&mut world, body(&mut source), plan, &mut next)
            .unwrap();
        assert!(
            loose
                .iter()
                .any(|g| g.cell().material == MaterialId(2) && g.cell().durability == 20)
        );
        assert_eq!(
            initial,
            source
                .terrain
                .cells()
                .iter()
                .filter(|c| c.material != MaterialId::VOID)
                .count()
                + loose.len()
        );
        loose.audit(&world).unwrap();
    }
}

#[test]
fn capacity_stale_plans_and_duplicate_identities_cannot_publish_half_an_edit() {
    let (mut world, mut source) = fixture(TerrainSurface::Blocks);
    let initial = source.terrain.clone();
    let motion = world.motion(source.assembly.body());
    let mut loose = LooseTerrain::new(LooseTerrainConfig {
        max_grains: 1,
        ..Default::default()
    })
    .unwrap();
    let mut next = 2;
    let plan = PreparedRelease::new(&source.terrain, &[edit(160)]).unwrap();
    assert_eq!(
        loose
            .commit(&mut world, body(&mut source), plan, &mut next)
            .unwrap_err(),
        ReleaseError::Capacity
    );
    assert_eq!(source.terrain, initial);
    assert_eq!(world.motion(source.assembly.body()), motion);
    assert_eq!(world.body_count(), 1);
    assert_eq!(next, 2);
    assert!(loose.is_empty());
    let mut loose = LooseTerrain::new(Default::default()).unwrap();
    let plan = PreparedRelease::new(&source.terrain, &[edit(160)]).unwrap();
    let mut duplicate = 1;
    assert!(
        loose
            .commit(&mut world, body(&mut source), plan.clone(), &mut duplicate)
            .is_err()
    );
    assert_eq!(source.terrain, initial);
    source.terrain.apply(edit(1)).unwrap();
    source.geometry.refresh(&source.terrain);
    let damaged = source.terrain.clone();
    assert!(
        loose
            .commit(&mut world, body(&mut source), plan, &mut next)
            .is_err()
    );
    assert_eq!(source.terrain, damaged);
    assert_eq!(world.body_count(), 1);
}
