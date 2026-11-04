use super::*;

#[test]
fn hit_velocities_include_rotation_and_preserve_misses_and_inside_hits() {
    let mut world = PhysicsWorld::new(Default::default());
    let id = BodyId::new(PhysicsId::new(7), BodyRole::PRIMARY);
    let collider = ColliderId::new(id.entity, ColliderRole::PRIMARY, 0);
    assert!(world.insert_body(
        id,
        BodySpec {
            position: Vec2::new(15.0, 0.0),
            linear_velocity: Vec2::new(2.0, -1.0),
            angular_velocity: 0.4,
            can_sleep: false,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(collider, 2.0, 3.0)]
    ));
    world.step(1.0 / 60.0);
    let observation = read(&sensor(), &world);
    let motion = world.motion(id).unwrap();
    let com = world.center_of_mass(id).unwrap();
    let expected = |point: Vec2| {
        let r = point - com;
        motion.linear_velocity + Vec2::new(-r.y, r.x) * motion.angular_velocity
    };
    assert_eq!(observation.nearest_collider, Some(collider));
    assert!(
        (observation.nearest_surface_velocity.unwrap()
            - expected(observation.nearest_point.unwrap()))
        .length()
            < 0.00001
    );
    let mut hits = 0;
    let mut misses = 0;
    for ray in &observation.rays {
        if let Some(d) = ray.distance {
            let point = Vec2::new(ray.bearing.cos(), ray.bearing.sin()) * d;
            assert!((ray.surface_velocity.unwrap() - expected(point)).length() < 0.00001);
            hits += 1;
        } else {
            assert!(ray.collider.is_none() && ray.surface_velocity.is_none());
            misses += 1;
        }
    }
    assert!(hits > 0 && misses > 0);
    let inside = sensor()
        .observe(&world, motion.position, 0.0, Default::default())
        .unwrap();
    assert_eq!(inside.nearest_distance, Some(0.0));
    assert_eq!(inside.nearest_collider, Some(collider));
    let empty = sensor()
        .observe(&world, Vec2::new(200.0, 200.0), 0.0, Default::default())
        .unwrap();
    assert!(empty.nearest_collider.is_none() && empty.nearest_surface_velocity.is_none());
}

#[test]
fn exact_hit_ties_use_stable_ids_not_insertion_order() {
    let mut observations = Vec::new();
    for order in [[2, 1], [1, 2]] {
        let mut world = PhysicsWorld::new(Default::default());
        for id in order {
            block(&mut world, id, Vec2::new(15.0, 0.0), Vec2::new(1.0, 3.0));
        }
        world.step(1.0 / 60.0);
        let observation = read(&sensor(), &world);
        assert_eq!(
            observation.nearest_collider.unwrap().entity,
            PhysicsId::new(1)
        );
        assert!(
            observation
                .rays
                .iter()
                .filter_map(|r| r.collider)
                .all(|c| c.entity == PhysicsId::new(1))
        );
        assert_eq!(read(&sensor(), &world.clone()), observation);
        observations.push(observation);
    }
    assert_eq!(observations[0], observations[1]);
}

#[test]
fn solid_radius_uses_local_shapes_and_ignores_sensor_bounds() {
    let mut world = PhysicsWorld::new(Default::default());
    let body = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    let mut solid = ColliderSpec::ball(ColliderId::new(body.entity, ColliderRole::PRIMARY, 0), 2.0);
    solid.local_position = Vec2::new(3.0, 0.0);
    let mut sensor = ColliderSpec::ball(
        ColliderId::new(body.entity, ColliderRole::PRIMARY, 1),
        1000.0,
    );
    sensor.sensor = true;
    assert!(world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[solid, sensor]
    ));
    assert_eq!(world.body_solid_radius(body), Some(5.0));
    assert_eq!(
        world.body_solid_radius_about(body, Vec2::new(3.0, 0.0)),
        Some(2.0)
    );
    assert_eq!(
        world.body_solid_radius_about(body, Vec2::new(f32::NAN, 0.0)),
        None
    );
    world.set_pose(body, Vec2::new(100.0, 200.0), 1.2, false);
    assert_eq!(world.body_solid_radius(body), Some(5.0));
}

fn sensor() -> PolarSensor {
    PolarSensor::new(PolarConfig {
        sectors: 16,
        radial_edges: vec![0., 10., 20., 40., 80.],
        angular_samples: 2,
        radial_samples: 2,
    })
    .unwrap()
}

fn block(world: &mut PhysicsWorld, id: u64, position: Vec2, half: Vec2) -> PhysicsId {
    let entity = PhysicsId::new(id);
    assert!(world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            position,
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            half.x,
            half.y
        )]
    ));
    entity
}

fn read(sensor: &PolarSensor, world: &PhysicsWorld) -> PolarObservation {
    sensor
        .observe(world, Vec2::ZERO, 0., PolarFilter::default())
        .unwrap()
}

#[test]
fn farther_geometry_survives_a_nearer_occluder() {
    let mut w = PhysicsWorld::new(Default::default());
    block(&mut w, 1, Vec2::new(15., 2.), Vec2::new(1., 4.));
    block(&mut w, 2, Vec2::new(55., 5.), Vec2::new(2., 8.));
    w.step(1. / 60.);
    let before = w.snapshot_bytes().unwrap();
    let o = read(&sensor(), &w);
    assert!(o.cells[16].overlap_coverage > 0.);
    assert_eq!(o.cells[32].overlap_coverage, 0.);
    assert!(o.cells[48].overlap_coverage > 0.);
    assert!(o.rays[0].distance.unwrap() < 20.);
    assert!((o.nearest_distance.unwrap() - 14.).abs() < 0.001);
    assert_eq!(before, w.snapshot_bytes().unwrap());
    assert_eq!(o, read(&sensor(), &w));
}

#[test]
fn thin_wall_between_samples_is_visible_to_overlap_channel() {
    let mut w = PhysicsWorld::new(Default::default());
    block(&mut w, 1, Vec2::new(15., 0.), Vec2::new(1., 0.01));
    w.step(1. / 60.);
    let o = read(&sensor(), &w);
    assert!(o.cells.iter().all(|c| c.sampled_occupancy == 0.));
    assert!(o.rays.iter().all(|r| r.distance.is_none()));
    assert!(o.cells[16].overlap_coverage > 0.);
    assert!(
        o.cells[31].overlap_coverage > 0.,
        "angular seam must touch both sectors"
    );
    assert!((o.nearest_distance.unwrap() - 14.).abs() < 0.001);
}

#[test]
fn frame_rotation_is_equivariant_and_edits_refresh_after_step() {
    let mut w = PhysicsWorld::new(Default::default());
    let entity = block(&mut w, 1, Vec2::new(15., 3.), Vec2::new(1., 2.));
    w.step(1. / 60.);
    let s = sensor();
    let initial = read(&s, &w);
    let angle = 1.17;
    let translation = Vec2::new(80., -12.);
    w.set_pose(
        BodyId::new(entity, BodyRole::PRIMARY),
        translation + Vec2::new(15., 3.).rotate_radians(angle),
        angle,
        false,
    );
    w.step(1. / 60.);
    let rotated = s
        .observe(&w, translation, angle, PolarFilter::default())
        .unwrap();
    assert_eq!(initial.cells, rotated.cells);
    assert!((initial.nearest_distance.unwrap() - rotated.nearest_distance.unwrap()).abs() < 0.001);
    for (a, b) in initial.rays.iter().zip(&rotated.rays) {
        assert_eq!(a.distance.is_some(), b.distance.is_some());
        if let (Some(a), Some(b)) = (a.distance, b.distance) {
            assert!((a - b).abs() < 0.001);
        }
    }
    w.remove_entity(entity);
    w.step(1. / 60.);
    let empty = read(&s, &w);
    assert!(empty.cells.iter().all(|c| c.overlap_coverage == 0.));
    block(&mut w, 2, Vec2::new(30., 5.), Vec2::new(1., 4.));
    w.step(1. / 60.);
    assert!(read(&s, &w).cells.iter().any(|c| c.overlap_coverage > 0.));
}

#[test]
fn filters_exclude_actor_and_sensors_without_hiding_other_solids() {
    let mut w = PhysicsWorld::new(Default::default());
    let actor = block(&mut w, 1, Vec2::ZERO, Vec2::new(2., 3.));
    let id = PhysicsId::new(2);
    w.insert_body(
        BodyId::new(id, BodyRole::PRIMARY),
        BodySpec::default(),
        &[ColliderSpec {
            sensor: true,
            ..ColliderSpec::ball(ColliderId::new(id, ColliderRole::PRIMARY, 0), 1.)
        }],
    );
    w.step(1. / 60.);
    let s = sensor();
    assert_eq!(read(&s, &w).nearest_distance, Some(0.));
    let excluded = [actor];
    let filter = PolarFilter {
        excluded: &excluded,
        ..Default::default()
    };
    assert!(
        s.observe(&w, Vec2::ZERO, 0., filter)
            .unwrap()
            .nearest_distance
            .is_none()
    );
    assert_eq!(
        s.observe(
            &w,
            Vec2::ZERO,
            0.,
            PolarFilter {
                include_sensors: true,
                ..filter
            }
        )
        .unwrap()
        .nearest_distance,
        Some(0.)
    );
    assert!(w.polar_geometry_outlines(filter).unwrap().is_empty());
    assert!(
        s.observe(
            &w,
            Vec2::ZERO,
            0.,
            PolarFilter {
                groups: CollisionGroups::NONE,
                ..Default::default()
            }
        )
        .unwrap()
        .nearest_distance
        .is_none()
    );
}

#[test]
fn occupied_points_are_never_in_cells_marked_clear() {
    let mut w = PhysicsWorld::new(Default::default());
    for i in 0..40 {
        let a = i as f32 * 2.399_963_1;
        let r = 2. + (i % 7) as f32 * 10.;
        block(
            &mut w,
            i + 1,
            Vec2::new(a.cos(), a.sin()) * r,
            Vec2::new(0.02, 0.03),
        );
    }
    w.step(1. / 60.);
    let s = sensor();
    let o = read(&s, &w);
    for motion in w.motions() {
        let p = motion.motion.position;
        let r = p.length();
        let a = p.y.atan2(p.x).rem_euclid(TAU);
        let sector = (a / TAU * s.config.sectors as f32) as usize;
        let band = s
            .config
            .radial_edges
            .windows(2)
            .position(|b| r >= b[0] && r < b[1])
            .unwrap();
        assert!(
            o.cells[band * s.config.sectors + sector].overlap_coverage > 0.,
            "{p:?}"
        );
    }
}

#[test]
fn bad_configurations_and_poses_are_errors_not_empty_space() {
    for config in [
        PolarConfig {
            sectors: 0,
            ..Default::default()
        },
        PolarConfig {
            radial_edges: vec![0., f32::NAN],
            ..Default::default()
        },
        PolarConfig {
            radial_edges: vec![0., 2., 1.],
            ..Default::default()
        },
        PolarConfig {
            angular_samples: 0,
            ..Default::default()
        },
        PolarConfig {
            radial_edges: vec![],
            ..Default::default()
        },
    ] {
        assert!(PolarSensor::new(config).is_err());
    }
    assert!(
        sensor()
            .observe(
                &PhysicsWorld::new(Default::default()),
                Vec2::ZERO,
                f32::NAN,
                Default::default()
            )
            .is_err()
    );
}
