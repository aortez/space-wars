use super::*;

#[test]
fn compound_mass_rays_and_clearance_match_separate_pieces_after_cuts() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        let mut terrain = Terrain::generate(
            97,
            97,
            0.5,
            vec![Material {
                id: MaterialId(1),
                hardness: 100,
            }],
            |c| {
                if Vec2::new((c.x - 48) as f32, (c.y - 48) as f32).length() <= 44.4 {
                    MaterialId(1)
                } else {
                    MaterialId::VOID
                }
            },
        )
        .unwrap()
        .with_surface_distances(|p| 22.2 - p.length())
        .unwrap();
        let mut geometry = TerrainGeometry::with_surface(&terrain, surface);
        let mut worlds =
            [TerrainColliders::Separate, TerrainColliders::ChunkCompound].map(|colliders| {
                let mut world = PhysicsWorld::new(PhysicsWorldConfig {
                    gravity: Vec2::ZERO,
                    ..Default::default()
                });
                let assembly = TerrainAssembly::insert(
                    &mut world,
                    PhysicsId::new(1),
                    BodySpec {
                        position: Vec2::new(13.0, -8.0),
                        angle: 0.73,
                        ..Default::default()
                    },
                    &terrain,
                    &geometry,
                    TerrainSpec {
                        surface,
                        colliders,
                        ..Default::default()
                    },
                )
                .unwrap();
                (world, assembly)
            });
        for cut in 0..4 {
            if cut > 0 {
                terrain
                    .apply(TerrainEdit {
                        brush: Brush::Circle {
                            center: [
                                CellCoord::new(48, 48),
                                CellCoord::new(63, 48),
                                CellCoord::new(32, 32),
                            ][cut - 1],
                            radius: 5,
                        },
                        mode: EditMode::Remove,
                    })
                    .unwrap();
                geometry.refresh(&terrain);
            }
            let mut measurements = Vec::new();
            for (world, assembly) in &mut worlds {
                assembly.synchronize(world, &terrain, &geometry).unwrap();
                let body = assembly.body();
                world.set_pose(body, Vec2::new(13.0, -8.0), 0.73, true);
                world.set_velocity(body, Vec2::ZERO, 0.0, true);
                world.step(1.0 / 60.0);
                let mut rays = Vec::new();
                let mut clearance = Vec::new();
                for i in 0..72 {
                    // Avoid exact shared vertices: either adjacent face may
                    // legitimately supply the normal there in a different BVH.
                    let up = Vec2::from_radians((i as f32 + 0.317) * std::f32::consts::TAU / 72.0);
                    for origin in [Vec2::ZERO, up * 25.0] {
                        let direction = if origin == Vec2::ZERO { up } else { -up };
                        rays.push(
                            world
                                .cast_ray(
                                    Vec2::new(13.0, -8.0) + origin.rotate_radians(0.73),
                                    direction.rotate_radians(0.73),
                                    RayCastOptions {
                                        max_distance: 50.0,
                                        ..Default::default()
                                    },
                                )
                                .map(|hit| (hit.distance, hit.normal)),
                        );
                    }
                    for distance in [20.0, 24.0] {
                        clearance.push(world.capsule_is_clear(
                            Vec2::new(13.0, -8.0) + up * distance,
                            0.35,
                            0.3,
                            0.2,
                            CollisionGroups::ALL,
                        ));
                    }
                }
                world.apply_impulse_at_point(
                    body,
                    Vec2::new(3.0, 7.0),
                    Vec2::new(16.0, -5.0),
                    true,
                );
                measurements.push((
                    world.body_mass(body).unwrap(),
                    world.center_of_mass(body).unwrap(),
                    world.motion(body).unwrap(),
                    rays,
                    clearance,
                ));
            }
            let (a, b) = (&measurements[0], &measurements[1]);
            assert!(
                (a.0 - b.0).abs() < a.0 * 0.00001,
                "mass {surface:?} cut {cut}"
            );
            assert!(a.1.distance_to(b.1) < 0.0001);
            assert!(a.2.linear_velocity.distance_to(b.2.linear_velocity) < 0.00001);
            assert!((a.2.angular_velocity - b.2.angular_velocity).abs() < 0.00001);
            for (x, y) in a.3.iter().zip(&b.3) {
                match (x, y) {
                    (Some(x), Some(y)) => {
                        assert!(
                            (x.0 - y.0).abs() < 0.0001,
                            "ray {surface:?} cut {cut}: {x:?} {y:?}"
                        );
                        // At a shared vertex either adjacent face can win the tie.
                        // A solid ray starting inside material has distance and
                        // normal zero in both layouts; it has no boundary normal.
                        if surface == TerrainSurface::Interpolated && x.0 > 0.0001 {
                            assert!(
                                x.1.dot(y.1) > 0.99,
                                "normal {surface:?} cut {cut}: {x:?} {y:?}"
                            );
                        }
                    }
                    (None, None) => {}
                    _ => panic!("ray coverage differs {surface:?} cut {cut}: {x:?} {y:?}"),
                }
            }
            assert_eq!(a.4, b.4);
            assert!(worlds[1].0.collider_count() < worlds[0].0.collider_count());
        }
        // Damage without removal must keep every compound's existing handle.
        let (world, assembly) = &mut worlds[1];
        let handles: Vec<_> = world
            .collider_ids()
            .map(|id| (id, world.collider_handle(id)))
            .collect();
        terrain
            .apply(TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(48, 10),
                    radius: 2,
                },
                mode: EditMode::Damage(1),
            })
            .unwrap();
        geometry.refresh(&terrain);
        assert_eq!(assembly.synchronize(world, &terrain, &geometry), Some(0));
        for (id, handle) in handles {
            assert_eq!(world.collider_handle(id), handle);
        }
    }
}
