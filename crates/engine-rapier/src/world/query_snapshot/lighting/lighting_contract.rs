use super::*;
use engine_core::lighting::{EmissionPoint, Receiver};
fn ray(start: Vec2, end: Vec2) -> LightRay {
    LightRay {
        receiver: Receiver {
            position: start,
            surface: None,
        },
        endpoint: EmissionPoint {
            position: end,
            surface: None,
        },
    }
}
fn add(world: &mut PhysicsWorld, entity: u64, position: Vec2, sensor: bool) -> ColliderId {
    let id = ColliderId::new(PhysicsId::new(entity), ColliderRole::PRIMARY, 0);
    let mut collider = ColliderSpec::cuboid(id, 0.5, 0.5);
    collider.sensor = sensor;
    assert!(world.insert_body(
        BodyId::new(id.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[collider]
    ));
    id
}
#[test]
fn canonical_solids_sensors_water_removal_and_immutable_copy() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    add(&mut world, 1, Vec2::new(2.0, 0.0), true);
    let water = add(&mut world, 2, Vec2::new(3.0, 0.0), false);
    let solid = add(&mut world, 3, Vec2::new(5.0, 0.0), false);
    world.step(1.0 / 60.0);
    let before = world.snapshot_bytes().unwrap();
    let snapshot = world.query_snapshot();
    let r = ray(Vec2::ZERO, Vec2::new(10.0, 0.0));
    let transparent = [water];
    let view = snapshot.lighting_visibility(&[], &transparent).unwrap();
    assert_eq!(view.blocker(r), Some(light_collider_id(solid)));
    assert_eq!(world.snapshot_bytes().unwrap(), before);
    world.remove_entity(solid.entity);
    world.step(1.0 / 60.0);
    assert_eq!(
        world
            .query_snapshot()
            .lighting_visibility(&[], &transparent)
            .unwrap()
            .blocker(r),
        None
    );
    assert_eq!(view.blocker(r), Some(light_collider_id(solid)));
}
#[test]
fn attached_origin_emitting_endpoint_and_same_host_intervening_part() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let origin = add(&mut world, 1, Vec2::ZERO, false);
    let emitter = add(&mut world, 2, Vec2::new(4.0, 0.0), false);
    world.step(1.0 / 60.0);
    let snapshot = world.query_snapshot();
    let view = snapshot.lighting_visibility(&[], &[]).unwrap();
    let mut r = ray(Vec2::new(0.5, 0.0), Vec2::new(3.5, 0.0));
    r.receiver.surface = Some(light_collider_id(origin));
    r.endpoint.surface = Some(light_collider_id(emitter));
    assert_eq!(view.blocker(r), None);
    let intervening = ColliderId::new(emitter.entity, ColliderRole::PRIMARY, 1);
    let mut c = ColliderSpec::cuboid(intervening, 0.2, 0.2);
    c.local_position = Vec2::new(-2.0, 0.0);
    assert!(world.insert_collider(BodyId::new(emitter.entity, BodyRole::PRIMARY), &c));
    world.step(1.0 / 60.0);
    assert_eq!(
        world
            .query_snapshot()
            .lighting_visibility(&[], &[])
            .unwrap()
            .blocker(r),
        Some(light_collider_id(intervening))
    );
    // Moving the origin inside its support is not covered by the local tolerance.
    r.receiver.position = Vec2::ZERO;
    assert_eq!(view.blocker(r), Some(light_collider_id(origin)));
}
#[test]
fn lightweight_tissue_endpoints_overlap_and_capacity() {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    let tissue = OpaqueFootprint {
        id: LightId(u64::MAX, 1),
        center: Vec2::ZERO,
        radius: 1.0,
    };
    let footprints = [tissue];
    let view = snapshot.lighting_visibility(&footprints, &[]).unwrap();
    let mut r = ray(Vec2::X, Vec2::new(2.0, 0.0));
    r.receiver.surface = Some(tissue.id);
    assert_eq!(view.blocker(r), None);
    r.endpoint.position = -Vec2::X;
    r.endpoint.surface = Some(tissue.id);
    assert_eq!(view.blocker(r), Some(tissue.id));
    let too_many = vec![tissue; MAX_LIGHT_FOOTPRINTS + 1];
    assert!(matches!(
        snapshot.lighting_visibility(&too_many, &[]),
        Err(LightingError::Capacity)
    ));
}
#[test]
fn moving_rotating_support_restores_visibility_without_query_steps() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let id = add(&mut world, 1, Vec2::new(2.0, 0.0), false);
    let body = BodyId::new(id.entity, BodyRole::PRIMARY);
    world.step(1.0 / 60.0);
    let r = ray(Vec2::ZERO, Vec2::new(4.0, 0.0));
    assert!(
        world
            .query_snapshot()
            .lighting_visibility(&[], &[])
            .unwrap()
            .blocker(r)
            .is_some()
    );
    assert!(world.set_pose(
        body,
        Vec2::new(2.0, 2.0),
        std::f32::consts::FRAC_PI_4,
        false
    ));
    world.step(1.0 / 60.0);
    let activity = physics_activity();
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(
        world
            .query_snapshot()
            .lighting_visibility(&[], &[])
            .unwrap()
            .blocker(r),
        None
    );
    assert_eq!(physics_activity(), activity);
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
}
#[test]
fn maximum_footprints_meter_work_and_expose_overflow() {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    let footprints: Vec<_> = (0..MAX_LIGHT_FOOTPRINTS)
        .map(|i| OpaqueFootprint {
            id: LightId(u64::MAX, i as u64),
            center: Vec2::new(i as f32, 10.0),
            radius: 0.1,
        })
        .collect();
    let view = snapshot.lighting_visibility(&footprints, &[]).unwrap();
    let r = ray(Vec2::ZERO, Vec2::X);
    for _ in 0..64 {
        assert_eq!(view.blocker(r), None);
    }
    assert_eq!(
        view.counters(),
        VisibilityCounters {
            rays: 64,
            collider_records: 0,
            footprint_tests: 64 * MAX_LIGHT_FOOTPRINTS as u64
        }
    );
    let exclusions = vec![
        ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
        MAX_OPTICAL_EXCLUSIONS + 1
    ];
    assert!(matches!(
        snapshot.lighting_visibility(&[], &exclusions),
        Err(LightingError::Capacity)
    ));
}
#[test]
fn coincident_emitting_receiver_is_local_but_other_geometry_blocks() {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    let footprint = OpaqueFootprint {
        id: LightId(u64::MAX, 1),
        center: Vec2::ZERO,
        radius: 1.0,
    };
    let footprints = [footprint];
    let view = snapshot.lighting_visibility(&footprints, &[]).unwrap();
    let mut r = ray(Vec2::X, Vec2::X);
    r.receiver.surface = Some(footprint.id);
    r.endpoint.surface = Some(footprint.id);
    assert_eq!(view.blocker(r), None);
    r.receiver.surface = None;
    r.endpoint.surface = None;
    assert_eq!(view.blocker(r), Some(footprint.id));
}
#[test]
fn collider_capacity_is_explicit() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    for i in 0..MAX_LIGHT_COLLIDERS {
        add(&mut world, i as u64, Vec2::new(i as f32, 20.0), false);
    }
    assert!(world.query_snapshot().lighting_visibility(&[], &[]).is_ok());
    add(&mut world, MAX_LIGHT_COLLIDERS as u64, Vec2::ZERO, false);
    assert!(matches!(
        world.query_snapshot().lighting_visibility(&[], &[]),
        Err(LightingError::Capacity)
    ));
}
#[test]
fn compound_shape_complexity_is_bounded_before_queries() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let id = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
    let mut collider = ColliderSpec::ball(id, 1.0);
    collider.shape = ColliderShape::Compound {
        children: vec![
            CompoundChild {
                shape: ColliderShape::Ball { radius: 0.1 },
                position: Vec2::ZERO,
                angle: 0.0,
            };
            MAX_LIGHT_COLLIDERS
        ],
    };
    assert!(world.insert_body(
        BodyId::new(id.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[collider]
    ));
    assert!(matches!(
        world.query_snapshot().lighting_visibility(&[], &[]),
        Err(LightingError::Capacity)
    ));
}
#[test]
fn attached_samples_follow_translated_rotated_emitting_support() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let receiver_id = add(&mut world, 1, Vec2::ZERO, false);
    let emitter_id = add(&mut world, 2, Vec2::new(4.0, 0.0), false);
    for (offset, angle) in [(Vec2::ZERO, 0.0), (Vec2::new(10.0, 5.0), 1.3)] {
        for (id, local) in [(receiver_id, Vec2::ZERO), (emitter_id, Vec2::new(4.0, 0.0))] {
            assert!(world.set_pose(
                BodyId::new(id.entity, BodyRole::PRIMARY),
                offset + local.rotate_radians(angle),
                angle,
                false
            ));
        }
        world.step(1.0 / 60.0);
        let snapshot = world.query_snapshot();
        let view = snapshot.lighting_visibility(&[], &[]).unwrap();
        let mut r = ray(
            offset + Vec2::new(0.5, 0.0).rotate_radians(angle),
            offset + Vec2::new(3.5, 0.0).rotate_radians(angle),
        );
        r.receiver.surface = Some(light_collider_id(receiver_id));
        r.endpoint.surface = Some(light_collider_id(emitter_id));
        assert_eq!(view.blocker(r), None);
    }
}

fn coincident(point: Vec2, id: LightId, naming: u8) -> LightRay {
    let mut r = ray(point, point);
    r.receiver.surface = (naming & 1 != 0).then_some(id);
    r.endpoint.surface = (naming & 2 != 0).then_some(id);
    r
}

#[test]
fn coincident_footprint_boundary_tolerance_and_embedded_endpoints() {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    let id = LightId(u64::MAX, 1);
    let footprints = [OpaqueFootprint {
        id,
        center: Vec2::ZERO,
        radius: 1.0,
    }];
    let view = snapshot.lighting_visibility(&footprints, &[]).unwrap();
    for naming in 0..=3 {
        for (x, at_boundary) in [
            (1.0, true),
            (1.0 - ENDPOINT_TOLERANCE * 0.5, true),
            (1.0 - ENDPOINT_TOLERANCE * 2.0, false),
            (0.0, false),
        ] {
            let expected = (!(at_boundary && naming != 0)).then_some(id);
            assert_eq!(
                view.blocker(coincident(Vec2::new(x, 0.0), id, naming)),
                expected,
                "footprint x={x}, endpoint naming mask={naming}"
            );
        }
    }
}

#[test]
fn coincident_canonical_boundary_tolerance_and_embedded_endpoints() {
    for shape in [
        ColliderShape::Ball { radius: 0.5 },
        ColliderShape::Cuboid {
            half_width: 0.5,
            half_height: 0.5,
        },
    ] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let collider_id = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
        let mut collider = ColliderSpec::ball(collider_id, 0.5);
        collider.shape = shape;
        assert!(world.insert_body(
            BodyId::new(collider_id.entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(2.0, 3.0),
                ..Default::default()
            },
            &[collider]
        ));
        world.step(1.0 / 60.0);
        let snapshot = world.query_snapshot();
        let view = snapshot.lighting_visibility(&[], &[]).unwrap();
        let id = light_collider_id(collider_id);
        for naming in 0..=3 {
            for (x, at_boundary) in [
                (0.5, true),
                (0.5 - ENDPOINT_TOLERANCE * 0.5, true),
                (0.5 - ENDPOINT_TOLERANCE * 2.0, false),
                (0.0, false),
            ] {
                let expected = (!(at_boundary && naming != 0)).then_some(id);
                assert_eq!(
                    view.blocker(coincident(Vec2::new(2.0 + x, 3.0), id, naming)),
                    expected,
                    "canonical x={x}, endpoint naming mask={naming}"
                );
            }
        }
    }
}

#[test]
fn coincident_overlapping_geometry_keeps_its_own_opaque_identity() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let surface = add(&mut world, 1, Vec2::ZERO, false);
    let overlapping = add(&mut world, 2, Vec2::new(0.5, 0.0), false);
    world.step(1.0 / 60.0);
    let snapshot = world.query_snapshot();
    let view = snapshot.lighting_visibility(&[], &[]).unwrap();
    for naming in 1..=3 {
        assert_eq!(
            view.blocker(coincident(
                Vec2::new(0.5, 0.0),
                light_collider_id(surface),
                naming
            )),
            Some(light_collider_id(overlapping))
        );
    }
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    let local = LightId(u64::MAX, 1);
    let other = LightId(u64::MAX, 2);
    let footprints = [
        OpaqueFootprint {
            id: local,
            center: Vec2::ZERO,
            radius: 1.0,
        },
        OpaqueFootprint {
            id: other,
            center: Vec2::X,
            radius: 1.0,
        },
    ];
    let view = snapshot.lighting_visibility(&footprints, &[]).unwrap();
    for naming in 1..=3 {
        assert_eq!(
            view.blocker(coincident(Vec2::X, local, naming)),
            Some(other)
        );
    }
}

#[test]
fn coincident_compound_contact_does_not_exempt_another_childs_interior() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let collider_id = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
    let mut collider = ColliderSpec::ball(collider_id, 1.0);
    collider.shape = ColliderShape::Compound {
        children: [Vec2::ZERO, Vec2::X]
            .into_iter()
            .map(|position| CompoundChild {
                shape: ColliderShape::Ball { radius: 1.0 },
                position,
                angle: 0.0,
            })
            .collect(),
    };
    assert!(world.insert_body(
        BodyId::new(collider_id.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[collider]
    ));
    world.step(1.0 / 60.0);
    let snapshot = world.query_snapshot();
    let view = snapshot.lighting_visibility(&[], &[]).unwrap();
    let id = light_collider_id(collider_id);
    for naming in 1..=3 {
        assert_eq!(view.blocker(coincident(Vec2::X, id, naming)), Some(id));
        assert_eq!(
            view.blocker(coincident(Vec2::new(2.0, 0.0), id, naming)),
            None
        );
    }
    assert_eq!(view.counters().rays, 6);
    assert_eq!(view.counters().collider_records, 6);
}

#[test]
fn coincident_polyline_contact_requires_its_own_surface_identity() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let collider_id = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
    let mut collider = ColliderSpec::ball(collider_id, 1.0);
    collider.shape = ColliderShape::Polyline {
        vertices: vec![Vec2::new(0.5, -1.0), Vec2::new(0.5, 1.0)],
    };
    assert!(world.insert_body(
        BodyId::new(collider_id.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[collider]
    ));
    world.step(1.0 / 60.0);
    let snapshot = world.query_snapshot();
    let view = snapshot.lighting_visibility(&[], &[]).unwrap();
    let id = light_collider_id(collider_id);
    for naming in 0..=3 {
        assert_eq!(
            view.blocker(coincident(Vec2::new(0.5, 0.0), id, naming)),
            (naming == 0).then_some(id)
        );
    }
}

#[test]
fn coordinate_limits_canonical_receiver_and_emitter_trims() {
    for sign in [-1.0, 1.0] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let center = Vec2::new(sign * 99999.0, 0.0);
        let collider = add(&mut world, 1, center, false);
        world.step(1.0 / 60.0);
        let snapshot = world.query_snapshot();
        let view = snapshot.lighting_visibility(&[], &[]).unwrap();
        let id = light_collider_id(collider);
        let surface = center + Vec2::new(sign * 0.5, 0.0);
        let outside = center + Vec2::new(sign, 0.0);
        let mut r = ray(surface, outside);
        r.receiver.surface = Some(id);
        assert_eq!(view.blocker(r), None, "attached receiver sign={sign}");
        let mut r = ray(outside, surface);
        r.endpoint.surface = Some(id);
        assert_eq!(view.blocker(r), None, "emitter endpoint sign={sign}");
    }
}

#[test]
fn coordinate_limits_footprint_receiver_and_emitter_trims() {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    for sign in [-1.0, 1.0] {
        let center = Vec2::new(sign * 99999.0, 0.0);
        let id = LightId(u64::MAX, 1);
        let footprints = [OpaqueFootprint {
            id,
            center,
            radius: 0.5,
        }];
        let view = snapshot.lighting_visibility(&footprints, &[]).unwrap();
        let surface = center + Vec2::new(sign * 0.5, 0.0);
        let outside = center + Vec2::new(sign, 0.0);
        let mut r = ray(surface, outside);
        r.receiver.surface = Some(id);
        assert_eq!(view.blocker(r), None, "attached footprint sign={sign}");
        let mut r = ray(outside, surface);
        r.endpoint.surface = Some(id);
        assert_eq!(view.blocker(r), None, "emitting footprint sign={sign}");
    }
}

#[test]
fn coordinate_limits_long_endpoint_trims_and_same_host_intervening_parts() {
    for sign in [-1.0, 1.0] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let center = Vec2::new(sign * 99999.0, 0.0);
        let source = add(&mut world, 1, center, false);
        world.step(1.0 / 60.0);
        let mut r = ray(
            Vec2::new(-sign * 100000.0, 0.0),
            center - Vec2::new(sign * 0.5, 0.0),
        );
        r.endpoint.surface = Some(light_collider_id(source));
        assert_eq!(
            world
                .query_snapshot()
                .lighting_visibility(&[], &[])
                .unwrap()
                .blocker(r),
            None
        );
        let part = ColliderId::new(source.entity, ColliderRole::PRIMARY, 1);
        let mut collider = ColliderSpec::cuboid(part, 0.1, 0.1);
        collider.local_position = Vec2::new(-sign * 0.75, 0.0);
        assert!(world.insert_collider(BodyId::new(source.entity, BodyRole::PRIMARY), &collider));
        world.step(1.0 / 60.0);
        assert_eq!(
            world
                .query_snapshot()
                .lighting_visibility(&[], &[])
                .unwrap()
                .blocker(r),
            Some(light_collider_id(part))
        );
        // The opposite direction needs a local receiver exemption, too.
        let mut reversed = ray(r.endpoint.position, r.receiver.position);
        reversed.receiver.surface = r.endpoint.surface;
        assert_eq!(
            world
                .query_snapshot()
                .lighting_visibility(&[], &[])
                .unwrap()
                .blocker(reversed),
            Some(light_collider_id(part))
        );
    }
}

#[test]
fn coordinate_limits_long_footprint_endpoint_and_overlapping_blocker() {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    for sign in [-1.0, 1.0] {
        let center = Vec2::new(sign * 99999.0, 0.0);
        let id = LightId(u64::MAX, 1);
        let local = OpaqueFootprint {
            id,
            center,
            radius: 0.5,
        };
        let mut r = ray(
            Vec2::new(-sign * 100000.0, 0.0),
            center - Vec2::new(sign * 0.5, 0.0),
        );
        r.endpoint.surface = Some(id);
        let local_only = [local];
        assert_eq!(
            snapshot
                .lighting_visibility(&local_only, &[])
                .unwrap()
                .blocker(r),
            None
        );
        let other = OpaqueFootprint {
            id: LightId(u64::MAX, 2),
            center: center - Vec2::new(sign * 0.75, 0.0),
            radius: 0.1,
        };
        let overlapping = [local, other];
        assert_eq!(
            snapshot
                .lighting_visibility(&overlapping, &[])
                .unwrap()
                .blocker(r),
            Some(other.id)
        );
    }
}

#[test]
fn coordinate_limits_large_local_shape_keeps_sub_ulp_trim() {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let snapshot = world.query_snapshot();
    let id = LightId(u64::MAX, 1);
    let footprint = [OpaqueFootprint {
        id,
        center: Vec2::ZERO,
        radius: 99999.5,
    }];
    let view = snapshot.lighting_visibility(&footprint, &[]).unwrap();
    for sign in [-1.0, 1.0] {
        let a = Vec2::new(sign * 99999.5, 0.0);
        let b = Vec2::new(sign * 100000.0, 0.0);
        let mut r = ray(a, b);
        r.receiver.surface = Some(id);
        assert_eq!(view.blocker(r), None);
        let mut r = ray(b, a);
        r.endpoint.surface = Some(id);
        assert_eq!(view.blocker(r), None);
    }
}

#[test]
fn coordinate_limits_rotated_compound_preserves_child_local_geometry() {
    for sign in [-1.0, 1.0] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let id = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
        let mut collider = ColliderSpec::ball(id, 0.5);
        collider.shape = ColliderShape::Compound {
            children: vec![
                CompoundChild {
                    shape: ColliderShape::Cuboid {
                        half_width: 0.5,
                        half_height: 0.5,
                    },
                    position: Vec2::new(0.0, 0.5),
                    angle: 0.0,
                },
                CompoundChild {
                    shape: ColliderShape::Cuboid {
                        half_width: 0.1,
                        half_height: 0.1,
                    },
                    position: Vec2::new(0.0, -0.25),
                    angle: 0.0,
                },
            ],
        };
        let center = Vec2::new(sign * 99998.0, sign * 99998.0);
        // Exact quarter turn keeps the input surface representable at the limit.
        assert!(world.insert_body(
            BodyId::new(id.entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: center,
                angle: std::f32::consts::FRAC_PI_2,
                ..Default::default()
            },
            &[collider]
        ));
        world.step(1.0 / 60.0);
        let view = world.query_snapshot();
        let view = view.lighting_visibility(&[], &[]).unwrap();
        // Left outer face of the first rotated child is clear when leaving it.
        let surface = center - Vec2::X;
        let mut r = ray(surface, surface - Vec2::X);
        r.receiver.surface = Some(light_collider_id(id));
        assert_eq!(view.blocker(r), None);
        // A different child of the very same collider must still block.
        let mut r = ray(center, center + Vec2::X);
        r.receiver.surface = Some(light_collider_id(id));
        assert_eq!(view.blocker(r), Some(light_collider_id(id)));
    }
}

#[test]
fn canonical_shape_clipping_matches_untrimmed_query_oracle() {
    let shapes = [
        ColliderShape::Ball { radius: 0.5 },
        ColliderShape::Cuboid {
            half_width: 0.5,
            half_height: 0.5,
        },
        ColliderShape::Capsule {
            half_segment: 0.5,
            radius: 0.25,
        },
        ColliderShape::ConvexPolygon {
            vertices: vec![
                Vec2::new(-0.5, -0.5),
                Vec2::new(0.5, -0.5),
                Vec2::new(0.5, 0.5),
                Vec2::new(-0.5, 0.5),
            ],
        },
        ColliderShape::Polyline {
            vertices: vec![
                Vec2::new(-0.5, -0.5),
                Vec2::new(0.0, 0.5),
                Vec2::new(0.5, -0.5),
            ],
        },
    ];
    for shape in shapes {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let id = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
        let mut collider = ColliderSpec::ball(id, 0.5);
        collider.shape = shape;
        assert!(world.insert_body(
            BodyId::new(id.entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &[collider]
        ));
        world.step(1.0 / 60.0);
        let snapshot = world.query_snapshot();
        let view = snapshot.lighting_visibility(&[], &[]).unwrap();
        for y in [-1.25, -0.375, 0.0, 0.375, 1.25] {
            for (a, b) in [
                (Vec2::new(-2.0, y), Vec2::new(2.0, y)),
                (Vec2::new(y, -2.0), Vec2::new(y, 2.0)),
                (Vec2::ZERO, Vec2::new(2.0, y)),
            ] {
                let r = ray(a, b);
                let oracle = snapshot.cast_ray(
                    a,
                    b - a,
                    RayCastOptions {
                        max_distance: (b - a).length(),
                        ..Default::default()
                    },
                );
                assert_eq!(
                    view.blocker(r).is_some(),
                    oracle.is_some(),
                    "ray {a:?} -> {b:?}"
                );
            }
        }
    }
}

#[test]
fn ceiling_light_contract_moving_solids_tissue_and_source_boundaries() {
    use engine_core::lighting::{ApertureSource, DirectionalSource, LightingSnapshot};
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let solid = add(&mut world, 991, Vec2::new(0.0, 5.0), false);
    world.step(1.0 / 60.0);
    let publication = LightingSnapshot::new(
        1,
        1,
        DirectionalSource {
            toward_source: Vec2::Y,
            strength: 50.0,
            color: engine_core::Color::WHITE,
        },
        &[],
    )
    .unwrap()
    .with_aperture(ApertureSource {
        id: LightId(992, 1),
        left: -2.0,
        right: 2.0,
        ceiling_y: 10.0,
        strength: 950.0,
    })
    .unwrap();
    let receivers = [
        Receiver {
            position: Vec2::ZERO,
            surface: None,
        },
        Receiver {
            position: Vec2::new(1.0, 0.0),
            surface: None,
        },
        Receiver {
            position: Vec2::new(2.1, 0.0),
            surface: None,
        },
    ];
    let old = world.query_snapshot();
    let visible = old.lighting_visibility(&[], &[]).unwrap();
    let exposure = publication.query(1, 1, &receivers, &visible).unwrap();
    assert_eq!(
        exposure.iter().map(|e| e.intensity).collect::<Vec<_>>(),
        [0.0, 1000.0, 50.0]
    );
    assert_eq!(exposure[0].first_blocker, Some(light_collider_id(solid)));
    let silhouettes = old.lighting_occluder_silhouettes(&[]).unwrap();
    assert!(
        silhouettes
            .iter()
            .any(|p| p.iter().any(|v| v.x < 0.0) && p.iter().any(|v| v.x > 0.0))
    );
    world.set_pose(
        BodyId::new(solid.entity, BodyRole::PRIMARY),
        Vec2::new(8.0, 5.0),
        0.0,
        true,
    );
    world.step(1.0 / 60.0);
    let fresh = world.query_snapshot();
    let clear = fresh.lighting_visibility(&[], &[]).unwrap();
    assert_eq!(
        publication.query(1, 1, &receivers, &clear).unwrap()[0].intensity,
        1000.0
    );
    assert_eq!(
        publication.query(1, 1, &receivers, &visible).unwrap()[0].intensity,
        0.0,
        "old geometry stays immutable"
    );
    let tissue = [OpaqueFootprint {
        id: LightId(993, 1),
        center: Vec2::new(1.0, 4.0),
        radius: 0.4,
    }];
    let shade = fresh.lighting_visibility(&tissue, &[]).unwrap();
    let samples = publication.query(1, 1, &receivers, &shade).unwrap();
    assert_eq!(samples[0].intensity, 1000.0);
    assert_eq!(samples[1].intensity, 0.0);
    assert_eq!(samples[1].first_blocker, Some(tissue[0].id));
    world.remove_entity(solid.entity);
    world.step(1.0 / 60.0);
    assert!(
        world
            .query_snapshot()
            .lighting_occluder_silhouettes(&[])
            .unwrap()
            .is_empty()
    );
}
