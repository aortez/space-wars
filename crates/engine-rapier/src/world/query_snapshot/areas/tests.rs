use super::*;

#[test]
fn proposed_hull_bounds_use_the_rotated_shape_and_shape_identity_ignores_only_pose() {
    let (mut world, body, id) = world(Vec2::ZERO);
    let points = vec![
        Vec2::new(-8.0, -1.0),
        Vec2::new(6.0, -2.0),
        Vec2::new(1.0, 3.0),
    ];
    let shape = ColliderSpec::convex_polygon(id, points.clone());
    assert!(world.replace_colliders(body, id.role, std::slice::from_ref(&shape)));
    world.step(1.0 / 60.0);
    let snapshot = world.query_snapshot();
    let position = Vec2::new(5.0, 20.0);
    let anchor = Vec2::new(10.0, 3.0);
    let area = world
        .collider_query_area(id, position, 0.73, anchor, -0.41)
        .unwrap();
    for p in points {
        let p = (position + p.rotate_radians(0.73) - anchor).rotate_radians(0.41);
        assert!(p.x >= area.minimum.x - 0.0001 && p.x <= area.maximum.x + 0.0001);
        assert!(p.y >= area.minimum.y - 0.0001 && p.y <= area.maximum.y + 0.0001);
    }
    world.set_pose(body, Vec2::new(100.0, 50.0), 2.0, true);
    world.step(1.0 / 60.0);
    assert!(snapshot.collider_shape_matches(&world, id));
    assert!(world.replace_colliders(body, id.role, &[shape]));
    assert!(
        !snapshot.collider_shape_matches(&world, id),
        "same semantic ID is not shape identity"
    );
    assert!(
        world
            .collider_query_area(id, position, f32::NAN, anchor, 0.0)
            .is_none()
    );
}

fn frame() -> QueryFrame<'static> {
    QueryFrame {
        previous_position: Vec2::ZERO,
        previous_angle: 0.0,
        current_position: Vec2::ZERO,
        current_angle: 0.0,
        excluded: &[],
    }
}
fn region() -> QueryRegion<'static> {
    QueryRegion {
        previous_position: Vec2::ZERO,
        previous_angle: 0.0,
        current_position: Vec2::ZERO,
        current_angle: 0.0,
        radius: 10.0,
        groups: CollisionGroups::ALL,
        excluded: &[],
    }
}
fn area() -> QueryArea {
    QueryArea {
        minimum: Vec2::new(-5.0, 4.0),
        maximum: Vec2::new(5.0, 6.0),
        groups: CollisionGroups::ALL,
    }
}
fn world(position: Vec2) -> (PhysicsWorld, BodyId, ColliderId) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let entity = PhysicsId::new(1);
    let body = BodyId::new(entity, BodyRole::PRIMARY);
    let collider = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
    assert!(world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[ColliderSpec::ball(collider, 0.3)]
    ));
    world.step(1.0 / 60.0);
    (world, body, collider)
}

#[test]
fn unrelated_motion_is_allowed_but_crossing_the_middle_of_an_edge_is_not() {
    let (mut world, body, _) = world(Vec2::new(0.0, 20.0));
    let snapshot = world.query_snapshot();
    world.set_pose(body, Vec2::new(0.0, 18.0), 0.0, true);
    world.step(1.0 / 60.0);
    let check = snapshot.validate_areas(&world, frame(), &[area()]);
    assert!(check.valid);
    assert_eq!(check.unrelated_changes, 1);
    world.set_pose(body, Vec2::new(0.0, 5.0), 0.0, true);
    world.step(1.0 / 60.0);
    let check = snapshot.validate_areas(&world, frame(), &[area()]);
    assert!(!check.valid);
    assert!(check.area_tests > 0);
    let old = world.query_snapshot();
    world.set_pose(body, Vec2::Y * 20.0, 0.0, true);
    world.step(1.0 / 60.0);
    assert!(!old.validate_areas(&world, frame(), &[area()]).valid);
}

#[test]
fn new_colliders_and_rotation_about_a_distant_origin_revoke_local_evidence() {
    let (mut world, body, collider) = world(Vec2::new(0.0, -9995.0));
    let snapshot = world.query_snapshot();
    let added = BodyId::new(PhysicsId::new(2), BodyRole::PRIMARY);
    assert!(world.insert_body(
        added,
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::Y * 5.0,
            ..Default::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(added.entity, ColliderRole::PRIMARY, 0),
            0.3,
        )]
    ));
    world.step(1.0 / 60.0);
    assert!(!snapshot.validate_areas(&world, frame(), &[area()]).valid);

    world.remove_entity(added.entity);
    assert!(world.replace_colliders(
        body,
        collider.role,
        &[ColliderSpec::cuboid(collider, 0.3, 10000.0)]
    ));
    world.step(1.0 / 60.0);
    let snapshot = world.query_snapshot();
    // The origin stays still, but this tiny angle moves the route-facing tip
    // by about 0.1 units. Using only the route area's radius would miss it.
    world.set_pose(body, Vec2::new(0.0, -9995.0), 0.00001, true);
    world.step(1.0 / 60.0);
    let check = snapshot.validate_areas(&world, frame(), &[area()]);
    assert!(!check.valid);
    assert!(check.area_tests > 0);
    assert!(!snapshot.validate_region(&world, region()).valid);
}

#[test]
fn circular_validation_ignores_unmeasured_box_corners() {
    let (mut world, body, _) = world(Vec2::new(9.0, 9.0));
    let snapshot = world.query_snapshot();
    world.set_pose(body, Vec2::new(8.0, 8.0), 0.0, true);
    world.step(1.0 / 60.0);
    assert!(snapshot.validate_region(&world, region()).valid);
    assert!(
        !snapshot
            .validate_areas(
                &world,
                frame(),
                &[QueryArea {
                    minimum: Vec2::new(-10.0, -10.0),
                    maximum: Vec2::new(10.0, 10.0),
                    groups: CollisionGroups::ALL,
                }]
            )
            .valid
    );
}

#[test]
fn rigid_frame_clone_filters_removal_and_shape_replacement_keep_their_contracts() {
    let (world, body, collider) = world(Vec2::Y * 5.0);
    let snapshot = world.query_snapshot();
    let mut clone = world.clone();
    assert!(snapshot.validate_areas(&clone, frame(), &[area()]).valid);
    let position = Vec2::new(100.0, -20.0);
    let angle = 0.3;
    clone.set_pose(
        body,
        position + (Vec2::Y * 5.0).rotate_radians(angle),
        angle,
        true,
    );
    clone.step(1.0 / 60.0);
    assert!(
        snapshot
            .validate_areas(
                &clone,
                QueryFrame {
                    current_position: position,
                    current_angle: angle,
                    ..frame()
                },
                &[area()]
            )
            .valid
    );
    for fault in 0..4 {
        let mut clone = world.clone();
        if fault == 0 {
            clone.remove_entity(body.entity);
        } else {
            let mut shape = ColliderSpec::ball(collider, if fault == 1 { 0.4 } else { 0.3 });
            if fault == 2 {
                shape.sensor = true;
            }
            if fault == 3 {
                shape.collision_groups = CollisionGroups::new(1, 0);
            }
            assert!(clone.replace_colliders(body, collider.role, &[shape]));
        }
        clone.step(1.0 / 60.0);
        assert!(
            !snapshot.validate_areas(&clone, frame(), &[area()]).valid,
            "fault {fault}"
        );
        assert!(
            snapshot
                .validate_areas(
                    &clone,
                    QueryFrame {
                        excluded: &[body.entity],
                        ..frame()
                    },
                    &[area()]
                )
                .valid
        );
    }
    assert!(!snapshot.validate_areas(&world, frame(), &[]).valid);
    assert!(
        !snapshot
            .validate_areas(
                &world,
                frame(),
                &[QueryArea {
                    minimum: Vec2::new(f32::NAN, 0.0),
                    ..area()
                }]
            )
            .valid
    );
}

#[test]
fn hollow_boundary_and_query_groups_do_not_fill_unmeasured_space() {
    let (mut world, body, collider) = world(Vec2::ZERO);
    let mut boundary = ColliderSpec::polyline(
        collider,
        vec![
            Vec2::new(-20.0, -20.0),
            Vec2::new(-20.0, 20.0),
            Vec2::new(20.0, 20.0),
            Vec2::new(20.0, -20.0),
            Vec2::new(-20.0, -20.0),
        ],
    );
    boundary.collision_groups = CollisionGroups::new(2, 1);
    assert!(world.replace_colliders(body, collider.role, &[boundary]));
    world.step(1.0 / 60.0);
    let snapshot = world.query_snapshot();
    world.set_pose(body, Vec2::X, 0.01, true);
    world.step(1.0 / 60.0);
    let check = snapshot.validate_areas(&world, frame(), &[area()]);
    assert!(check.valid);
    assert!(
        check.area_tests > 0,
        "exercise actual hollow geometry, not only its AABB"
    );
    assert!(snapshot.validate_region(&world, region()).valid);
    let separate = QueryArea {
        minimum: Vec2::new(-30.0, -30.0),
        maximum: Vec2::new(30.0, 30.0),
        groups: CollisionGroups::new(1, 4),
    };
    assert!(snapshot.validate_areas(&world, frame(), &[separate]).valid);
}

#[test]
fn rejection_details_preserve_old_and_new_envelope_overlaps_and_replacements() {
    let (mut world, body, collider) = world(Vec2::Y * 5.0);
    let snapshot = world.query_snapshot();
    let right = QueryArea {
        minimum: Vec2::new(4.0, -1.0),
        maximum: Vec2::new(6.0, 1.0),
        ..area()
    };
    world.set_pose(body, Vec2::X * 5.0, 0.0, true);
    world.step(1.0 / 60.0);
    let before = world.snapshot_bytes().unwrap();
    let report = snapshot.diagnose_region(&world, region(), &[area(), right]);
    assert_eq!(world.snapshot_bytes().unwrap(), before);
    assert!(report.complete);
    assert!(!snapshot.validate_region(&world, region()).valid);
    assert_eq!(report.region_changes, 1);
    assert_eq!(report.area_changes, vec![1, 1]);
    assert_eq!(report.changes[0].areas, vec![0, 1]);
    assert_eq!(
        report.changes[0].previous.as_ref().unwrap().position,
        Vec2::Y * 5.0
    );
    assert_eq!(
        report.changes[0].current.as_ref().unwrap().position,
        Vec2::X * 5.0
    );
    assert!(report.changes[0].motion_bound.unwrap() > 7.0);
    assert_eq!(report.unsupported_tests, 0);
    assert_eq!(
        report,
        snapshot.diagnose_region(&world.clone(), region(), &[area(), right])
    );

    assert!(world.replace_colliders(body, collider.role, &[ColliderSpec::ball(collider, 0.4)]));
    world.step(1.0 / 60.0);
    let report = snapshot.diagnose_region(&world, region(), &[area(), right]);
    assert_eq!(report.region_changes, 2);
    assert_eq!(
        report
            .changes
            .iter()
            .filter(|c| c.previous.is_none())
            .count(),
        1
    );
    assert_eq!(
        report
            .changes
            .iter()
            .filter(|c| c.current.is_none())
            .count(),
        1
    );
    assert!(
        report.changes.iter().all(
            |c| c.previous.as_ref().or(c.current.as_ref()).unwrap().collider == Some(collider)
        )
    );
}

#[test]
fn rejection_details_are_bounded_without_truncating_totals() {
    let (mut world, _, _) = world(Vec2::Y * 20.0);
    let snapshot = world.query_snapshot();
    for id in 2..14 {
        let body = BodyId::new(PhysicsId::new(id), BodyRole::PRIMARY);
        assert!(world.insert_body(
            body,
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::Y * 5.0,
                ..Default::default()
            },
            &[ColliderSpec::ball(
                ColliderId::new(body.entity, ColliderRole::PRIMARY, 0),
                0.3
            )]
        ));
    }
    world.step(1.0 / 60.0);
    let report = snapshot.diagnose_region(&world, region(), &[area()]);
    assert!(report.complete);
    assert_eq!(report.changed_colliders, 12);
    assert_eq!(report.region_changes, 12);
    assert_eq!(report.area_changes, vec![12]);
    assert_eq!(report.changes.len(), 8);
    assert_eq!(report.omitted_changes, 4);
    assert_eq!(report.area_tests, 24);

    let invalid = snapshot.diagnose_region(
        &world,
        QueryRegion {
            current_angle: f32::NAN,
            ..region()
        },
        &[area()],
    );
    assert!(!invalid.complete);
    assert_eq!(invalid.unavailable, Some("invalid frame or bounds"));
    assert!(invalid.changes.is_empty());
    let full = snapshot.diagnose_region(&world, region(), &[area(); 9]);
    assert!(!full.complete);
    assert_eq!(full.unavailable, Some("area or exclusion capacity"));
}

#[test]
fn diagnostic_scan_and_acceptance_agree_on_motion_filters_and_lifecycle() {
    let (source, body, collider) = world(Vec2::Y * 5.0);
    let snapshot = source.query_snapshot();
    for case in 0..9 {
        let mut world = source.clone();
        let mut region = region();
        match case {
            0 => {}
            1 => {
                world.remove_entity(body.entity);
            }
            2..=4 => {
                let mut spec = ColliderSpec::ball(collider, 0.4);
                if case == 3 {
                    spec.sensor = true;
                }
                if case == 4 {
                    spec.collision_groups = CollisionGroups::NONE;
                }
                world.replace_colliders(body, collider.role, &[spec]);
            }
            5 => {
                world.set_pose(body, Vec2::Y * 20.0, 0.0, true);
            }
            6 => {
                region.current_position = Vec2::new(100.0, 20.0);
                region.current_angle = 0.3;
                world.set_pose(
                    body,
                    region.current_position + (Vec2::Y * 5.0).rotate_radians(0.3),
                    0.3,
                    true,
                );
            }
            7 => {
                for (_, collider) in world.raw.colliders.iter_mut() {
                    collider.set_enabled(false);
                }
            }
            _ => {
                world.set_pose(body, Vec2::Y * 6.0, 0.0, true);
            }
        }
        world.step(1.0 / 60.0);
        for exclude in [false, true] {
            let ids = [body.entity];
            let region = QueryRegion {
                excluded: if exclude { &ids } else { &[] },
                ..region
            };
            let report = snapshot.diagnose_region(&world, region, &[area()]);
            assert!(report.complete);
            assert_eq!(
                snapshot.validate_region(&world, region).valid,
                report.region_changes == 0,
                "case {case}, exclude {exclude}"
            );
        }
    }
}
