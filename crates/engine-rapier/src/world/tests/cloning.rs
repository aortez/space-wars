use super::*;
use rapier2d::prelude::RevoluteJointBuilder;

const DT: f32 = 1.0 / 60.0;

pub(super) fn snapshot_clone(world: &PhysicsWorld) -> PhysicsWorld {
    let mut cloned = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    if cloned.collect_events {
        cloned.rebuild_events();
    }
    cloned
}

pub(super) fn assert_same(left: &PhysicsWorld, right: &PhysicsWorld) {
    assert!(left.snapshot_bytes().unwrap() == right.snapshot_bytes().unwrap());
    assert_eq!(left.contact_events(), right.contact_events());
    assert_eq!(left.sensor_intersections(), right.sensor_intersections());
    assert_eq!(left.body_indices, right.body_indices);
    assert_eq!(left.collider_indices, right.collider_indices);
    assert_eq!(left.joint_indices, right.joint_indices);
}

fn fixture(collect_events: bool) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -10.0),
        collect_events,
        max_ccd_substeps: 4,
        ..Default::default()
    });
    let (_, floor, collider) = ball_ids(1);
    assert!(
        world.insert_body(
            floor,
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &[ColliderSpec {
                shape: ColliderShape::Compound {
                    children: [-2.0, 2.0]
                        .map(|x| CompoundChild {
                            shape: ColliderShape::Cuboid {
                                half_width: 2.0,
                                half_height: 0.5,
                            },
                            position: Vec2::new(x, -0.5),
                            angle: 0.0,
                        })
                        .into(),
                },
                ..ColliderSpec::ball(collider, 1.0)
            }],
        )
    );
    insert_ball(&mut world, 2, Vec2::new(0.0, 0.24));
    let (_, sensor_body, sensor_id) = ball_ids(3);
    let mut sensor = ColliderSpec::cuboid(sensor_id, 5.0, 3.0);
    sensor.sensor = true;
    assert!(world.insert_body(
        sensor_body,
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[sensor],
    ));
    let (_, platform, collider) = ball_ids(4);
    assert!(world.insert_body(
        platform,
        BodySpec {
            kind: BodyKind::KinematicPosition,
            position: Vec2::new(2.0, 1.0),
            ..Default::default()
        },
        &[ColliderSpec::ball(collider, 0.1)],
    ));
    let bob = insert_ball(&mut world, 5, Vec2::new(2.0, 0.5));
    assert!(
        world.insert_raw_joint(
            JointId::new(platform.entity, JointRole::PRIMARY),
            platform,
            bob,
            RevoluteJointBuilder::new()
                .local_anchor1(Vector::new(0.0, -0.5))
                .build()
                .into(),
        )
    );
    let (_, sleeper, collider) = ball_ids(6);
    assert!(world.insert_body(
        sleeper,
        BodySpec {
            position: Vec2::new(-2.0, 0.25),
            ..Default::default()
        },
        &[ColliderSpec::ball(collider, 0.25)],
    ));
    world
}

#[test]
fn clone_matches_snapshot_with_contacts_joints_and_pending_edits() {
    for collect_events in [false, true] {
        for phase in 0..3 {
            let mut original = fixture(collect_events);
            if phase > 0 {
                for _ in 0..180 {
                    original.step(DT);
                }
                assert!(
                    original.raw.bodies[original.body_handle(ball_ids(6).1).unwrap()].is_sleeping()
                );
                if collect_events {
                    assert!(!original.contact_events().is_empty());
                    assert!(!original.sensor_intersections().is_empty());
                }
            }
            if phase == 2 {
                // Fork between edits and the next pipeline step, including an
                // arena slot reuse and removal of a connected joint.
                assert!(original.remove_entity(ball_ids(5).0));
                insert_ball(&mut original, 8, Vec2::new(-0.2, 0.25));
                let (_, floor, collider) = ball_ids(1);
                assert!(original.replace_colliders(
                    floor,
                    collider.role,
                    &[ColliderSpec::cuboid(collider, 4.0, 0.2)],
                ));
                assert!(original.set_next_kinematic_pose(ball_ids(4).1, Vec2::new(2.1, 1.1), 0.1));
                assert!(original.apply_impulse(ball_ids(6).1, Vec2::new(0.1, 0.2), true));
            }
            let before = original.snapshot_bytes().unwrap();
            let mut direct = original.clone();
            let mut restored = snapshot_clone(&original);
            assert_same(&direct, &restored);
            assert_eq!(original.snapshot_bytes().unwrap(), before);
            for tick in 0..120 {
                for world in [&mut original, &mut direct, &mut restored] {
                    if tick == 40 {
                        world.apply_impulse(ball_ids(2).1, Vec2::new(0.2, 0.3), true);
                    }
                    world.step(DT);
                }
                assert_same(&direct, &restored);
                assert_same(&direct, &original);
            }
        }
    }
}

#[test]
fn clone_keeps_shape_edits_and_event_collection_independent() {
    let mut original = fixture(true);
    original.step(DT);
    let before = original.snapshot_bytes().unwrap();
    let events = original.contact_events().to_vec();
    let sensors = original.sensor_intersections().to_vec();
    let mut cloned = original.clone();
    let handle = cloned.collider_handle(ball_ids(2).2).unwrap();
    // This edits through Rapier's copy-on-write API, not by replacing a shape.
    cloned.raw.colliders[handle]
        .shape_mut()
        .as_ball_mut()
        .unwrap()
        .radius = 0.5;
    assert!(cloned.remove_entity(ball_ids(5).0));
    assert_eq!(cloned.joint_count(), 0);
    assert_eq!(original.joint_count(), 1);
    cloned.step(DT);
    assert_eq!(original.snapshot_bytes().unwrap(), before);
    assert_eq!(original.contact_events(), events);
    assert_eq!(original.sensor_intersections(), sensors);

    let cloned_before = cloned.snapshot_bytes().unwrap();
    original.remove_entity(ball_ids(3).0);
    original.step(DT);
    assert_eq!(cloned.snapshot_bytes().unwrap(), cloned_before);
}
