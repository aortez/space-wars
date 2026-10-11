use super::*;

fn room(gap: f32, angle: f32, sensor: bool, groups: CollisionGroups) -> (PhysicsWorld, ColliderId) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let origin = Vec2::new(37.0, -19.0);
    let actor = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);
    let mut spec = ColliderSpec::capsule(actor, 0.3, 0.2);
    spec.collision_groups = CollisionGroups::new(1, 2);
    assert!(world.insert_body(
        BodyId::new(actor.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: origin,
            angle: 1.1,
            ..Default::default()
        },
        &[spec],
    ));
    let wall = |position, half_width, half_height| CompoundChild {
        position,
        angle: 0.0,
        shape: ColliderShape::Cuboid {
            half_width,
            half_height,
        },
    };
    let top_half = (2.1 - gap / 2.0) / 2.0;
    let top_center = gap / 2.0 + top_half;
    let id = ColliderId::new(PhysicsId::new(2), ColliderRole::PRIMARY, 0);
    assert!(world.insert_body(
        BodyId::new(id.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: origin,
            angle,
            ..Default::default()
        },
        &[ColliderSpec {
            shape: ColliderShape::Compound {
                children: vec![
                    wall(Vec2::new(-2.0, 0.0), 0.1, 2.1),
                    wall(Vec2::new(2.0, 0.0), 0.1, 2.1),
                    wall(Vec2::new(0.0, -2.0), 2.1, 0.1),
                    wall(Vec2::new(-top_center, 2.0), top_half, 0.1),
                    wall(Vec2::new(top_center, 2.0), top_half, 0.1),
                ]
            },
            sensor,
            collision_groups: groups,
            ..ColliderSpec::ball(id, 1.0)
        }],
    ));
    world.step(1.0 / 60.0);
    (world, actor)
}

fn grid(spacing: f32) -> ConnectivityGrid {
    ConnectivityGrid {
        half_cells: (3.0 / spacing) as u16,
        spacing,
        angle: 0.0,
        penetration_allowance: 0.025,
    }
}

#[test]
fn closed_and_too_narrow_rooms_have_a_witnessed_barrier_at_each_resolution() {
    for gap in [0.0, 0.12] {
        for angle in [0.0, 0.67] {
            let (world, actor) = room(gap, angle, false, CollisionGroups::new(2, 1));
            let before = world.snapshot_bytes().unwrap();
            for spacing in [0.1, 0.05, 0.025] {
                let result = world
                    .diagnose_capsule_connectivity(actor, grid(spacing), |_| true)
                    .unwrap();
                assert!(
                    !result.source_blocked && !result.reaches_boundary,
                    "gap={gap}, angle={angle}, spacing={spacing}"
                );
                assert!(result.reachable_cells > 0 && !result.barrier.is_empty());
                // Recheck connectivity using ONLY the retained witnesses. All
                // other cells, even measured solids, are deliberately open.
                let side = result.rows.len();
                let mut blocked = vec![false; side * side];
                for w in &result.barrier {
                    assert_eq!(w.collider.entity, PhysicsId::new(2));
                    assert!((w.center.distance_to(w.solid_point) - w.distance).abs() < 1e-6);
                    assert!(
                        w.distance + result.cell_circumradius + result.numerical_margin
                            < result.effective_radius
                    );
                    blocked[w.cell[1] * side + w.cell[0]] = true;
                }
                let source = side * (side / 2) + side / 2;
                let mut seen = vec![false; side * side];
                seen[source] = true;
                let mut queue = VecDeque::from([source]);
                while let Some(i) = queue.pop_front() {
                    assert!(
                        i % side > 0 && i % side + 1 < side && i / side > 0 && i / side + 1 < side
                    );
                    for n in neighbors(i, side) {
                        if !blocked[n] && !seen[n] {
                            seen[n] = true;
                            queue.push_back(n);
                        }
                    }
                }
            }
            assert_eq!(world.snapshot_bytes().unwrap(), before);
        }
    }
}

#[test]
fn unresolved_coarse_cells_remain_open_and_source_overlap_is_not_a_certificate() {
    let (mut world, actor) = room(0.25, 0.0, false, CollisionGroups::new(2, 1));
    let coarse = world
        .diagnose_capsule_connectivity(actor, grid(0.1), |_| true)
        .unwrap();
    let fine = world
        .diagnose_capsule_connectivity(actor, grid(0.025), |_| true)
        .unwrap();
    assert!(coarse.reaches_boundary && !fine.reaches_boundary);
    let solid = ColliderId::new(PhysicsId::new(3), ColliderRole::PRIMARY, 0);
    let mut spec = ColliderSpec::ball(solid, 1.0);
    spec.collision_groups = CollisionGroups::new(2, 1);
    assert!(world.insert_body(
        BodyId::new(solid.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(37.0, -19.0),
            ..Default::default()
        },
        &[spec]
    ));
    world.step(1.0 / 60.0);
    let result = world
        .diagnose_capsule_connectivity(actor, grid(0.05), |_| true)
        .unwrap();
    assert!(result.source_blocked);
    assert_eq!(result.reachable_cells, 0);
    assert!(result.barrier.is_empty());
}

#[test]
fn open_door_sensors_groups_and_excluded_solids_do_not_certify_enclosure() {
    for (gap, sensor, groups, include) in [
        (1.0, false, CollisionGroups::new(2, 1), true),
        (0.0, true, CollisionGroups::new(2, 1), true),
        (0.0, false, CollisionGroups::new(4, 4), true),
        (0.0, false, CollisionGroups::new(2, 1), false),
    ] {
        let (world, actor) = room(gap, 0.37, sensor, groups);
        let before = world.snapshot_bytes().unwrap();
        let result = world
            .diagnose_capsule_connectivity(actor, grid(0.05), |_| include)
            .unwrap();
        assert!(!result.source_blocked && result.reaches_boundary);
        assert_eq!(world.snapshot_bytes().unwrap(), before);
    }
}

#[test]
fn invalid_grids_and_non_capsules_are_rejected() {
    let (world, actor) = room(0.0, 0.0, false, CollisionGroups::ALL);
    for invalid in [
        ConnectivityGrid {
            half_cells: 0,
            ..grid(0.05)
        },
        ConnectivityGrid {
            half_cells: 257,
            ..grid(0.05)
        },
        ConnectivityGrid {
            spacing: f32::NAN,
            ..grid(0.05)
        },
        ConnectivityGrid {
            spacing: 0.0,
            ..grid(0.05)
        },
        ConnectivityGrid {
            spacing: 2.0,
            ..grid(0.05)
        },
        ConnectivityGrid {
            angle: f32::INFINITY,
            ..grid(0.05)
        },
        ConnectivityGrid {
            penetration_allowance: -1.0,
            ..grid(0.05)
        },
        ConnectivityGrid {
            penetration_allowance: 0.2,
            ..grid(0.05)
        },
    ] {
        assert!(
            world
                .diagnose_capsule_connectivity(actor, invalid, |_| true)
                .is_none()
        );
    }
    let wall = ColliderId::new(PhysicsId::new(2), ColliderRole::PRIMARY, 0);
    assert!(
        world
            .diagnose_capsule_connectivity(wall, grid(0.05), |_| true)
            .is_none()
    );
}
