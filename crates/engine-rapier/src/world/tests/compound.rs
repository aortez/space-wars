use super::*;

#[cfg(feature = "ccd-diagnostics")]
#[test]
fn ccd_shared_motion_rejects_before_contact_graph_lookup() {
    use rapier2d::counters::ccd_probe;

    for substeps in [1, 4] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            max_ccd_substeps: substeps,
            ..Default::default()
        });
        // Individually fast, close enough to share swept AABB candidates, but
        // translating together. Their relative motion cannot close the gap.
        for id in [1, 2] {
            let (_, body, collider) = ball_ids(id);
            assert!(world.insert_body(
                body,
                BodySpec {
                    position: Vec2::new(id as f32 * 0.3, 0.0),
                    linear_velocity: Vec2::new(100.0, 0.0),
                    ccd_enabled: true,
                    can_sleep: false,
                    ..Default::default()
                },
                &[ColliderSpec::ball(collider, 0.1)]
            ));
        }
        world.step(1.0 / 60.0);
        ccd_probe::begin();
        world.step(1.0 / 60.0);
        let counts = ccd_probe::finish();
        assert!(counts.toi_calls > 0, "substeps={substeps}: {counts:?}");
        assert_eq!(counts.thickness_rejected, counts.toi_calls);
        assert_eq!(counts.contact_lookup_calls, 0, "{counts:?}");
        assert!(world.contact_events().is_empty());
        for id in [1, 2] {
            let motion = world.motion(ball_ids(id).1).unwrap();
            assert_eq!(motion.linear_velocity, Vec2::new(100.0, 0.0));
            assert!((motion.position.x - (id as f32 * 0.3 + 100.0 / 30.0)).abs() < 1e-5);
        }
    }
}

fn thickness_wall(collider: ColliderId, parts: usize, half_width: f32) -> ColliderSpec {
    ColliderSpec {
        shape: ColliderShape::Compound {
            children: (0..parts)
                .map(|i| CompoundChild {
                    shape: ColliderShape::Cuboid {
                        half_width,
                        half_height: 1.0,
                    },
                    position: Vec2::new(0.0, i as f32 * 2.0),
                    angle: 0.0,
                })
                .collect(),
        },
        ..ColliderSpec::ball(collider, 1.0)
    }
}

fn thickness_test_world(substeps: usize) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        max_ccd_substeps: substeps,
        ..Default::default()
    });
    let (_, wall, wall_collider) = ball_ids(1);
    assert!(world.insert_body(
        wall,
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[thickness_wall(wall_collider, 2, 0.2)]
    ));
    let (_, ball, ball_collider) = ball_ids(2);
    assert!(world.insert_body(
        ball,
        BodySpec {
            position: Vec2::new(-1.0, 0.0),
            linear_velocity: Vec2::new(100.0, 0.0),
            ccd_enabled: true,
            can_sleep: false,
            ..Default::default()
        },
        &[ColliderSpec::ball(ball_collider, 0.1)]
    ));
    world
}

#[cfg(feature = "ccd-diagnostics")]
#[test]
fn ccd_compound_thickness_is_shared_across_passes_but_not_steps() {
    use rapier2d::counters::ccd_probe;

    for substeps in [0, 1, 4] {
        let mut world = thickness_test_world(substeps);
        // The engine normalizes its public setting to at least one. Exercise
        // Rapier's disabled-CCD branch directly as well.
        world.raw.integration_parameters.max_ccd_substeps = substeps;
        for tick in 0..3 {
            // Establish Rapier's integrated CCD velocity without an impact;
            // new bodies otherwise start with zero previous-step CCD velocity.
            world.set_pose(ball_ids(2).1, Vec2::new(-4.0, 0.0), 0.0, true);
            world.set_velocity(ball_ids(2).1, Vec2::new(100.0, 0.0), 0.0, true);
            world.step(1.0 / 60.0);
            // Re-arm the same collision each tick, including after the first
            // contact. A persistent cache would incorrectly report no misses.
            world.set_pose(ball_ids(2).1, Vec2::new(-1.0, 0.0), 0.0, true);
            world.set_velocity(ball_ids(2).1, Vec2::new(100.0, 0.0), 0.0, true);
            ccd_probe::begin();
            world.step(1.0 / 60.0);
            let counts = ccd_probe::finish();
            if substeps == 0 {
                assert_eq!(counts.compound_lookups, 0);
                assert_eq!(counts.compound_misses, 0);
            } else {
                assert_eq!(counts.compound_misses, 1, "tick={tick}: {counts:?}");
                assert_eq!(counts.compound_children, 2);
                assert!(counts.clamp_candidates > 0);
                if substeps > 1 {
                    assert!(world.raw.physics_pipeline.counters.ccd.num_substeps > 1);
                    assert!(counts.first_candidates > 0);
                    assert!(counts.compound_lookups > counts.compound_misses);
                }
            }
        }
    }
}

#[test]
fn ccd_thickness_shape_edits_and_handle_reuse_keep_snapshot_continuation() {
    for substeps in [1, 4] {
        let mut world = thickness_test_world(substeps);
        let (_, wall, collider) = ball_ids(1);
        let original_handle = world.collider_handle(collider).unwrap();
        for stage in 0..5 {
            let spec = match stage {
                0 => thickness_wall(collider, 2, 0.2),
                1 => thickness_wall(collider, 1, 0.01),
                2 => ColliderSpec::ball(collider, 0.3),
                3 => thickness_wall(collider, 3, 0.05),
                _ => thickness_wall(collider, 4, 0.1),
            };
            if stage == 4 {
                assert!(world.replace_colliders(wall, collider.role, &[spec]));
                let new_handle = world.collider_handle(collider).unwrap();
                assert_eq!(
                    new_handle.into_raw_parts().0,
                    original_handle.into_raw_parts().0
                );
                assert_ne!(new_handle, original_handle, "exercise a reused arena slot");
            } else if stage > 0 {
                // Exercise an edit under the same raw handle as well as
                // compound -> primitive -> compound transitions.
                let shape = build_collider(&spec, true).unwrap().shared_shape().clone();
                world.raw.colliders[original_handle].set_shape(shape);
            }
            world.set_pose(ball_ids(2).1, Vec2::new(-1.0, 0.0), 0.0, true);
            world.set_velocity(ball_ids(2).1, Vec2::new(100.0, 0.0), 0.0, true);
            let mut restored =
                PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
            #[cfg(feature = "ccd-diagnostics")]
            rapier2d::counters::ccd_probe::begin();
            world.step(1.0 / 60.0);
            #[cfg(feature = "ccd-diagnostics")]
            {
                let counts = rapier2d::counters::ccd_probe::finish();
                let children = match stage {
                    0 => 2,
                    1 => 1,
                    2 => 0,
                    3 => 3,
                    _ => 4,
                };
                assert_eq!(counts.compound_misses, u64::from(children > 0));
                assert_eq!(counts.compound_children, children);
            }
            restored.step(1.0 / 60.0);
            assert_eq!(world.contact_events(), restored.contact_events());
            assert_eq!(
                world.snapshot_bytes().unwrap(),
                restored.snapshot_bytes().unwrap(),
                "substeps={substeps}, stage={stage}"
            );
            assert!(
                world.motion(ball_ids(2).1).unwrap().position.x < 0.0,
                "CCD must stop the ball before it passes through the edited wall"
            );
        }
    }
}

#[test]
fn compound_sweep_pruning_preserves_physics_and_snapshot_continuation() {
    use rapier2d::parry::query::DefaultQueryDispatcher;

    for kind in [
        BodyKind::Fixed,
        BodyKind::KinematicPosition,
        BodyKind::KinematicVelocity,
    ] {
        for substeps in [1, 4] {
            let mut filtered = PhysicsWorld::new(PhysicsWorldConfig {
                gravity: Vec2::new(0.0, -50.0),
                max_ccd_substeps: substeps,
                ..Default::default()
            });
            let (_, ground, ground_collider) = ball_ids(1);
            assert!(
                filtered.insert_body(
                    ground,
                    BodySpec {
                        kind,
                        ..Default::default()
                    },
                    &[ColliderSpec {
                        shape: ColliderShape::Compound {
                            children: (-12..=12)
                                .map(|x| CompoundChild {
                                    shape: ColliderShape::Cuboid {
                                        half_width: 0.5,
                                        half_height: 0.1
                                    },
                                    position: Vec2::new(x as f32, 0.0),
                                    angle: 0.0,
                                })
                                .collect(),
                        },
                        ..ColliderSpec::ball(ground_collider, 1.0)
                    }]
                )
            );
            let (_, clump, clump_collider) = ball_ids(2);
            assert!(
                filtered.insert_body(
                    clump,
                    BodySpec {
                        position: Vec2::new(0.0, 6.0),
                        linear_velocity: Vec2::new(3.0, -20.0),
                        angular_velocity: 2.0,
                        ccd_enabled: true,
                        ..Default::default()
                    },
                    &[ColliderSpec {
                        shape: ColliderShape::Compound {
                            children: (0..4)
                                .map(|x| CompoundChild {
                                    shape: ColliderShape::Cuboid {
                                        half_width: 0.5,
                                        half_height: 0.3
                                    },
                                    position: Vec2::new(x as f32 - 1.5, (x % 2) as f32 * 0.2),
                                    angle: 0.0,
                                })
                                .collect(),
                        },
                        ..ColliderSpec::ball(clump_collider, 1.0)
                    }]
                )
            );
            let (_, ball, ball_collider) = ball_ids(3);
            assert!(filtered.insert_body(
                ball,
                BodySpec {
                    position: Vec2::new(-4.0, 14.0),
                    linear_velocity: Vec2::new(1.0, -100.0),
                    ccd_enabled: true,
                    ..Default::default()
                },
                &[ColliderSpec::ball(ball_collider, 0.15)]
            ));
            let mut reference = filtered.clone();
            reference
                .raw
                .narrow_phase
                .set_query_dispatcher(DefaultQueryDispatcher);
            let mut impacted = false;
            for tick in 1..=180 {
                for world in [&mut filtered, &mut reference] {
                    if kind == BodyKind::KinematicPosition {
                        let t = tick as f32 / 60.0;
                        world.set_next_kinematic_pose(ground, Vec2::new(t * 0.5, 0.0), t * 0.02);
                    } else if kind == BodyKind::KinematicVelocity {
                        world.set_velocity(ground, Vec2::new(0.5, 0.0), 0.02, true);
                    }
                    world.step(1.0 / 60.0);
                }
                impacted |= !filtered.contact_events().is_empty();
                assert_eq!(
                    filtered.contact_events(),
                    reference.contact_events(),
                    "kind={kind:?}, substeps={substeps}, tick={tick}"
                );
                assert_eq!(
                    filtered.snapshot_bytes().unwrap(),
                    reference.snapshot_bytes().unwrap(),
                    "kind={kind:?}, substeps={substeps}, tick={tick}"
                );
                if tick == 90 {
                    filtered =
                        PhysicsWorld::from_snapshot_bytes(&filtered.snapshot_bytes().unwrap())
                            .unwrap();
                    reference =
                        PhysicsWorld::from_snapshot_bytes(&reference.snapshot_bytes().unwrap())
                            .unwrap();
                    reference
                        .raw
                        .narrow_phase
                        .set_query_dispatcher(DefaultQueryDispatcher);
                }
            }
            assert!(impacted);
        }
    }
}

#[test]
fn ccd_uses_ground_velocity_when_rejecting_slow_relative_motion() {
    for kind in [BodyKind::KinematicPosition, BodyKind::KinematicVelocity] {
        for compound in [false, true] {
            for substeps in [1, 4] {
                for speed in [2.5, 5.0] {
                    ccd_passenger_clearance(kind, compound, substeps, speed);
                }
            }
        }
    }
}

fn ccd_passenger_clearance(kind: BodyKind, compound: bool, substeps: usize, speed: f32) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        max_ccd_substeps: substeps,
        ..Default::default()
    });
    let (_, platform, floor) = ball_ids(3);
    let children = [-20.0, 20.0].map(|x| CompoundChild {
        shape: ColliderShape::Cuboid {
            half_width: 20.0,
            half_height: 0.2,
        },
        position: Vec2::new(x, -0.2),
        angle: 0.0,
    });
    let colliders = if compound {
        vec![ColliderSpec {
            shape: ColliderShape::Compound {
                children: children.to_vec(),
            },
            ..ColliderSpec::ball(floor, 1.0)
        }]
    } else {
        children
            .into_iter()
            .enumerate()
            .map(|(i, child)| ColliderSpec {
                shape: child.shape,
                local_position: child.position,
                ..ColliderSpec::ball(ColliderId::new(platform.entity, floor.role, i as u16), 1.0)
            })
            .collect()
    };
    assert!(world.insert_body(
        platform,
        BodySpec {
            kind,
            linear_velocity: Vec2::new(0.0, 24.0),
            ..Default::default()
        },
        &colliders
    ));
    let (_, passenger, passenger_collider) = ball_ids(1);
    let start = Vec2::new(-0.2, 0.16);
    let velocity = Vec2::new(speed, 24.0);
    assert!(world.insert_body(
        passenger,
        BodySpec {
            position: start,
            linear_velocity: velocity,
            ccd_enabled: true,
            ..Default::default()
        },
        &[ColliderSpec {
            friction: 0.0,
            ..ColliderSpec::ball(passenger_collider, 0.15)
        }]
    ));
    // Both bodies have the same normal velocity and a 0.01-unit gap. Their
    // relative motion is too slow to need CCD. Zeroing the floor's CCD velocity
    // instead counts their shared transport as impact speed and can clamp the
    // passenger against ground already placed at its future pose.
    for tick in 1..=30 {
        let time = tick as f32 / 60.0;
        let target = Vec2::new(0.0, 24.0 * time);
        if kind == BodyKind::KinematicPosition {
            world.set_next_kinematic_pose(platform, target, 0.0);
        }
        world.step(1.0 / 60.0);
        let pose = world.motion(passenger).unwrap();
        assert!(
            pose.position.distance_to(start + velocity * time) < 0.002,
            "{kind:?}, compound={compound}, substeps={substeps}, speed={speed}, tick={tick}: {pose:?}"
        );
        assert!(world.motion(platform).unwrap().position.distance_to(target) < 0.0001);
    }
}

#[test]
fn ccd_still_stops_at_a_moving_kinematic_wall() {
    for kind in [BodyKind::KinematicPosition, BodyKind::KinematicVelocity] {
        for substeps in [1, 4] {
            let mut world = PhysicsWorld::new(PhysicsWorldConfig {
                max_ccd_substeps: substeps,
                ..Default::default()
            });
            let (_, wall, wall_collider) = ball_ids(3);
            assert!(world.insert_body(
                wall,
                BodySpec {
                    kind,
                    linear_velocity: Vec2::new(24.0, 0.0),
                    ..Default::default()
                },
                &[ColliderSpec::cuboid(wall_collider, 0.2, 20.0)]
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
                &[ColliderSpec::ball(ball_collider, 0.15)]
            ));
            let mut impacted = false;
            for tick in 1..=30 {
                let target = Vec2::new(24.0 * (tick as f32 / 60.0), 0.0);
                if kind == BodyKind::KinematicPosition {
                    world.set_next_kinematic_pose(wall, target, 0.0);
                }
                world.step(1.0 / 60.0);
                impacted |= world
                    .contact_events()
                    .iter()
                    .any(|event| event.impulse_magnitude > 0.0);
                assert!(world.motion(wall).unwrap().position.distance_to(target) < 0.0001);
                let motion = world.motion(ball).unwrap();
                // Allow transient penetration before the next contact solve.
                assert!(
                    motion.position.x < target.x - 0.30,
                    "{kind:?}, substeps={substeps}, tick={tick}: {motion:?}"
                );
            }
            assert!(impacted, "report the real impact while the wall moves");
            assert!((world.motion(ball).unwrap().position.x - 11.65).abs() < 0.01);
            assert!((world.motion(ball).unwrap().linear_velocity.x - 24.0).abs() < 0.01);
        }
    }
}

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
