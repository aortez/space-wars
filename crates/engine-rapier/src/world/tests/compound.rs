use super::*;

#[test]
fn ccd_substeps_keep_prescribed_kinematic_motion_uniform() {
    prescribed_motion_substeps(0.0, Vec2::ZERO);
    prescribed_motion_substeps(0.04, Vec2::new(1.5, 0.2));
}

fn prescribed_motion_substeps(turn: f32, offset: Vec2) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        max_ccd_substeps: 4,
        ..Default::default()
    });
    let (_, platform, platform_collider) = ball_ids(3);
    assert!(world.insert_body(
        platform,
        BodySpec {
            kind: BodyKind::KinematicPosition,
            position: Vec2::new(0.0, -4.0),
            ..Default::default()
        },
        &[ColliderSpec {
            local_position: offset,
            ..ColliderSpec::cuboid(platform_collider, 3.0, 0.2)
        }]
    ));
    let (_, ball, ball_collider) = ball_ids(1);
    assert!(world.insert_body(
        ball,
        BodySpec {
            position: Vec2::new(-1.0, 0.0),
            linear_velocity: Vec2::new(100.0, 0.0),
            ccd_enabled: true,
            ..Default::default()
        },
        &[ColliderSpec {
            restitution: 1.0,
            ..ColliderSpec::ball(ball_collider, 0.25)
        }]
    ));
    let (_, obstacle, obstacle_collider) = ball_ids(2);
    assert!(world.insert_body(
        obstacle,
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[ColliderSpec {
            restitution: 1.0,
            ..ColliderSpec::ball(obstacle_collider, 0.25)
        }]
    ));
    world.refresh_mass_properties(platform);
    let mut subdivided = false;
    for tick in 0..4 {
        let target = Vec2::new((tick + 1) as f32 * 0.4, -4.0);
        let angle = (tick + 1) as f32 * turn;
        let expected_velocity = (target + offset.rotate_radians(angle)
            - world.center_of_mass(platform).unwrap())
            * 60.0;
        world.set_next_kinematic_pose(platform, target, angle);
        let mut restored =
            PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
        world.step(1.0 / 60.0);
        restored.step(1.0 / 60.0);
        assert_eq!(world.motion(platform), restored.motion(platform));
        assert_eq!(
            world.raw.bodies[world.body_handle(platform).unwrap()].body_type(),
            RigidBodyType::KinematicPositionBased
        );
        let substeps = world.raw.physics_pipeline.counters.ccd.num_substeps;
        subdivided |= substeps > 1;
        let motion = world.motion(platform).unwrap();
        assert!(motion.position.distance_to(target) < 0.00001);
        assert!((motion.angle - angle).abs() < 0.00001);
        assert!(
            (motion.angular_velocity - turn * 60.0).abs() < 0.00001,
            "spin {} expected {}",
            motion.angular_velocity,
            turn * 60.0
        );
        assert!(
            motion.linear_velocity.distance_to(expected_velocity) < 0.001,
            "tick {tick}, CCD substeps {substeps}: {motion:?}"
        );
    }
    assert!(subdivided, "exercise a resolving CCD subdivision");
}

fn child(x: f32) -> CompoundChild {
    CompoundChild {
        shape: ColliderShape::Cuboid {
            half_width: 0.25,
            half_height: 0.1,
        },
        position: Vec2::new(x, 0.0),
        angle: 0.0,
    }
}

#[test]
fn compound_reports_each_child_impact_and_restores_contacts() {
    let mut results = Vec::new();
    for compound in [false, true] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            ..Default::default()
        });
        let (entity, body, collider) = ball_ids(1);
        let children = vec![child(-0.3), child(0.3)];
        let colliders = if compound {
            vec![ColliderSpec {
                shape: ColliderShape::Compound { children },
                ..ColliderSpec::ball(collider, 1.0)
            }]
        } else {
            children
                .into_iter()
                .enumerate()
                .map(|(i, c)| ColliderSpec {
                    shape: c.shape,
                    local_position: c.position,
                    ..ColliderSpec::ball(ColliderId::new(entity, BALL_COLLIDER, i as u16), 1.0)
                })
                .collect()
        };
        assert!(world.insert_body(
            body,
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &colliders
        ));
        let (_, ball, ball_collider) = ball_ids(2);
        assert!(world.insert_body(
            ball,
            BodySpec {
                position: Vec2::new(0.0, 0.55),
                linear_velocity: Vec2::new(0.0, -3.0),
                ..Default::default()
            },
            &[ColliderSpec::ball(ball_collider, 0.5)]
        ));
        let mut restored =
            PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
        world.step(1.0 / 60.0);
        restored.step(1.0 / 60.0);
        assert_eq!(world.motion(ball), restored.motion(ball));
        assert_eq!(world.contact_events(), restored.contact_events());
        let events = world.contact_events();
        assert_eq!(
            events.len(),
            2,
            "two child hits must survive event deduplication"
        );
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.collider_a.entity, entity);
            assert_eq!(event.subshape_a, compound.then_some(i as u32));
            assert_eq!(event.subshape_b, None);
            assert!(event.impulse_magnitude > 0.0);
            assert!(event.local_contact_a.unwrap().position.y > 0.09);
            assert!(world.collider_handle(event.collider_a).is_some());
        }
        assert_eq!(world.surface_contacts(ball_collider).count(), 2);
        results.push(events.iter().map(|e| e.impulse_magnitude).sum::<f32>());
    }
    assert!((results[0] - results[1]).abs() < 0.0001);
}

#[test]
fn compound_ccd_keeps_the_child_hit_after_separation() {
    let mut impact_ticks = Vec::new();
    for compound in [false, true] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            max_ccd_substeps: 4,
            ..Default::default()
        });
        let (_, ground, collider) = ball_ids(1);
        let shape = if compound {
            ColliderShape::Compound {
                children: vec![child(0.0), child(10.0)],
            }
        } else {
            child(0.0).shape
        };
        assert!(world.insert_body(
            ground,
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &[ColliderSpec {
                restitution: 1.0,
                shape,
                ..ColliderSpec::ball(collider, 1.0)
            }]
        ));
        let (_, ball, ball_collider) = ball_ids(2);
        assert!(world.insert_body(
            ball,
            BodySpec {
                position: Vec2::new(0.0, 1.0),
                linear_velocity: Vec2::new(0.0, -200.0),
                ccd_enabled: true,
                ..Default::default()
            },
            &[ColliderSpec {
                restitution: 1.0,
                ..ColliderSpec::ball(ball_collider, 0.1)
            }]
        ));
        // CCD can clamp at the end of one step and resolve at the next. Use
        // the same four-tick contract as the existing transient-impact test.
        let mut hit = None;
        for tick in 0..4 {
            let metrics = world.step(1.0 / 60.0);
            assert!(
                world.motion(ball).unwrap().position.y >= 0.19,
                "no tunneling"
            );
            if metrics.contact_pairs == 0
                && let Some(event) = world.contact_events().first().copied()
            {
                hit = Some(event);
                impact_ticks.push(tick);
                break;
            }
        }
        let event = hit.expect("retain the callback hit after separation");
        assert_eq!(event.subshape_a, compound.then_some(0));
        assert!(event.impulse_magnitude > 0.0);
        assert!(event.local_contact_a.unwrap().position.y > 0.09);
        assert!(world.motion(ball).unwrap().linear_velocity.y > 0.0);
    }
    assert_eq!(impact_ticks[0], impact_ticks[1]);
}

#[test]
fn compound_against_compound_preserves_both_child_identities() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        ..Default::default()
    });
    for (id, fixed) in [(1, true), (2, false)] {
        let (_, body, collider) = ball_ids(id);
        assert!(world.insert_body(
            body,
            BodySpec {
                kind: if fixed {
                    BodyKind::Fixed
                } else {
                    BodyKind::Dynamic
                },
                position: Vec2::new(0.0, if fixed { 0.0 } else { 0.19 }),
                linear_velocity: if fixed {
                    Vec2::ZERO
                } else {
                    Vec2::new(0.0, -3.0)
                },
                ..Default::default()
            },
            &[ColliderSpec {
                shape: ColliderShape::Compound {
                    children: vec![child(-0.5), child(0.5)]
                },
                ..ColliderSpec::ball(collider, 1.0)
            }]
        ));
    }
    world.step(1.0 / 60.0);
    let keys: Vec<_> = world
        .contact_events()
        .iter()
        .map(|e| (e.subshape_a, e.subshape_b))
        .collect();
    assert_eq!(keys, vec![(Some(0), Some(0)), (Some(1), Some(1))]);
    assert!(
        world
            .contact_events()
            .iter()
            .all(|e| e.impulse_magnitude > 0.0)
    );
}

#[test]
fn invalid_compound_replacements_are_atomic() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let body = insert_ball(&mut world, 1, Vec2::ZERO);
    let (_, _, id) = ball_ids(1);
    let before = world.snapshot_bytes().unwrap();
    for children in [
        vec![],
        vec![CompoundChild {
            position: Vec2::new(f32::NAN, 0.0),
            ..child(0.0)
        }],
        vec![CompoundChild {
            shape: ColliderShape::Compound {
                children: vec![child(0.0)],
            },
            ..child(0.0)
        }],
        vec![CompoundChild {
            shape: ColliderShape::Polyline {
                vertices: vec![Vec2::ZERO, Vec2::X],
            },
            ..child(0.0)
        }],
    ] {
        assert!(!world.replace_colliders(
            body,
            BALL_COLLIDER,
            &[ColliderSpec {
                shape: ColliderShape::Compound { children },
                ..ColliderSpec::ball(id, 1.0)
            }]
        ));
        assert_eq!(world.snapshot_bytes().unwrap(), before);
    }
}
