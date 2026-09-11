use super::*;

#[test]
fn contour_mass_center_and_rotational_response_match_the_full_material() {
    let terrain = Terrain::generate(
        8,
        7,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |p| {
            if p.x + p.y < 8 && p != CellCoord::new(2, 2) {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let mut results = Vec::new();
    for surface in [TerrainSurface::Blocks, TerrainSurface::Contour] {
        let geometry = TerrainGeometry::with_surface(&terrain, surface);
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            ..Default::default()
        });
        let assembly = TerrainAssembly::insert(
            &mut world,
            PhysicsId::new(1),
            BodySpec {
                position: Vec2::new(30.0, 10.0),
                angle: 0.7,
                ..Default::default()
            },
            &terrain,
            &geometry,
            TerrainSpec {
                surface,
                ..Default::default()
            },
        )
        .unwrap();
        let body = assembly.body();
        world.apply_impulse_at_point(body, Vec2::new(3.0, 7.0), Vec2::new(33.0, 11.0), true);
        results.push((
            world.body_mass(body).unwrap(),
            world.center_of_mass(body).unwrap(),
            world.motion(body).unwrap(),
        ));
        let restored = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(restored.body_mass(body), world.body_mass(body));
        assert_eq!(restored.motion(body), world.motion(body));
    }
    assert!((results[0].0 - results[1].0).abs() < 0.0001);
    assert!(results[0].1.distance_to(results[1].1) < 0.0001);
    assert!(
        results[0]
            .2
            .linear_velocity
            .distance_to(results[1].2.linear_velocity)
            < 0.0001
    );
    assert!((results[0].2.angular_velocity - results[1].2.angular_velocity).abs() < 0.0001);
}

#[test]
fn sloped_queries_map_concave_fill_to_material_and_rebuild_neighbour_colliders() {
    let mut terrain = Terrain::generate(
        65,
        65,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |p| {
            if p.x + p.y <= 64 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let mut geometry = TerrainGeometry::with_surface(&terrain, TerrainSurface::Contour);
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        ..Default::default()
    });
    let mut assembly = TerrainAssembly::insert(
        &mut world,
        PhysicsId::new(1),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &terrain,
        &geometry,
        TerrainSpec {
            surface: TerrainSurface::Contour,
            ..Default::default()
        },
    )
    .unwrap();
    world.step(1.0 / 60.0);
    let hit = world
        .cast_ray(
            Vec2::new(2.0, 2.0),
            Vec2::new(-1.0, -1.0).normalized(),
            RayCastOptions::default(),
        )
        .unwrap();
    assert!(hit.normal.x > 0.6 && hit.normal.y > 0.6, "{hit:?}");
    let owner = geometry
        .source_cell(&terrain, hit.point - hit.normal * 0.01)
        .unwrap();
    terrain
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: owner,
                radius: 1,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    // Reject stale neighbour geometry before any collider mutation.
    assert!(
        assembly
            .synchronize(&mut world, &terrain, &geometry)
            .is_none()
    );
    geometry.refresh(&terrain);
    assert!(
        assembly
            .synchronize(&mut world, &terrain, &geometry)
            .unwrap()
            > 1
    );
    world.step(1.0 / 60.0);
    let next = world
        .cast_ray(
            Vec2::new(2.0, 2.0),
            Vec2::new(-1.0, -1.0).normalized(),
            RayCastOptions::default(),
        )
        .unwrap();
    assert!(next.point.distance_to(hit.point) > 0.4);
}
