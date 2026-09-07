use super::*;

fn prone_notch() -> (PhysicsWorld, SpacelingAssembly) {
    prone_notch_at(-std::f32::consts::FRAC_PI_2)
}

fn prone_notch_at(angle: f32) -> (PhysicsWorld, SpacelingAssembly) {
    let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
    for (id, x) in [(3, -1.5), (4, 1.5)] {
        let id = PhysicsId::new(id);
        assert!(world.insert_body(
            BodyId::new(id, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(x, 0.4),
                ..Default::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(id, ColliderRole::PRIMARY, 0),
                0.5,
                0.4
            )]
        ));
    }
    world.set_pose(spaceling.body(), Vec2::new(0.0, 0.31), angle, true);
    world.set_velocity(spaceling.body(), Vec2::ZERO, 0.0, true);
    spaceling.set_balance(&mut world, SpacelingBalance::KnockedDown);
    for _ in 0..8 {
        world.apply_velocity_delta(spaceling.body(), GRAVITY * DT, true);
        world.step(DT);
    }
    assert!(spaceling.snapshot(&world).unwrap().grounded());
    (world, spaceling)
}

#[test]
fn jump_requests_a_physical_get_up_from_a_prone_excavation() {
    for angle in [-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2] {
        for balance in [
            SpacelingBalance::KnockedDown,
            SpacelingBalance::Recovering,
            SpacelingBalance::Balanced,
        ] {
            let (mut world, mut spaceling) = prone_notch_at(angle);
            spaceling.set_balance(&mut world, balance);
            let pose = world.motion(spaceling.body()).unwrap();
            assert!(!spaceling.apply_control(
                &mut world,
                SpacelingControl {
                    walk: 0.0,
                    jump_held: true
                },
                GRAVITY,
                DT
            ));
            let motion = world.motion(spaceling.body()).unwrap();
            assert_eq!(motion.position, pose.position, "get-up must not teleport");
            assert_eq!(motion.angle, pose.angle, "get-up must not snap rotation");
            assert!(
                motion.linear_velocity.y > 1.0,
                "jump should begin a physical get-up: {motion:?}"
            );
            for _ in 0..180 {
                tick(
                    &mut world,
                    &mut spaceling,
                    SpacelingControl {
                        walk: 0.0,
                        jump_held: true,
                    },
                    GRAVITY,
                );
            }
            let snapshot = spaceling.snapshot(&world).unwrap();
            assert_eq!(snapshot.balance, SpacelingBalance::Balanced, "{snapshot:?}");
            assert!(snapshot.grounded(), "{snapshot:?}");
            assert!(angle_error(snapshot.up, snapshot.motion.angle).abs() < 0.15);
            assert_eq!(
                snapshot.jumps, 0,
                "held get-up must not become a buffered jump"
            );
            assert_eq!(snapshot.get_up_attempts, 1);
            assert_eq!(snapshot.get_up_result, SpacelingGetUpResult::Succeeded);
        }
    }
}

#[test]
fn low_ceiling_blocks_standing_but_allows_crawling_to_open_ground() {
    let (mut world, mut spaceling) = prone_notch();
    world.remove_entity(PhysicsId::new(3));
    world.remove_entity(PhysicsId::new(4));
    let roof = PhysicsId::new(5);
    assert!(world.insert_body(
        BodyId::new(roof, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(0.0, 1.35),
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(roof, ColliderRole::PRIMARY, 0),
            2.5,
            0.25
        )],
    ));
    world.step(DT);
    let held = SpacelingControl {
        walk: 0.0,
        jump_held: true,
    };
    tick(&mut world, &mut spaceling, held, GRAVITY);
    assert_eq!(
        spaceling.snapshot(&world).unwrap().get_up_result,
        SpacelingGetUpResult::Blocked
    );
    for _ in 0..120 {
        tick(&mut world, &mut spaceling, held, GRAVITY);
        let motion = world.motion(spaceling.body()).unwrap();
        let half_height =
            spaceling.spec.radius + spaceling.spec.half_segment * motion.angle.cos().abs();
        assert!(
            motion.position.y + half_height <= 1.14,
            "must remain below the roof: {motion:?}"
        );
        assert!(
            motion.position.y - half_height >= -0.04,
            "must remain above the floor: {motion:?}"
        );
    }
    assert_eq!(spaceling.snapshot(&world).unwrap().get_up_attempts, 1);
    for _ in 0..480 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl { walk: 1.0, ..held },
            GRAVITY,
        );
    }
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert!(
        snapshot.motion.position.x > 4.0,
        "must be able to crawl out: {snapshot:?}"
    );
    assert_eq!(snapshot.balance, SpacelingBalance::Balanced, "{snapshot:?}");
    assert!(snapshot.grounded());
    assert_eq!(snapshot.jumps, 0);
}

#[test]
fn get_up_follows_rotating_moving_support_under_strong_gravity() {
    let (mut world, mut spaceling) = floor_fixture(-0.35, Vec2::new(3.0, 1.0));
    let floor = BodyId::new(FLOOR, BodyRole::PRIMARY);
    let parent = world.motion(floor).unwrap();
    world.set_velocity(floor, parent.linear_velocity, 0.25, true);
    let position = parent.position + Vec2::Y.rotate_radians(parent.angle) * 0.81;
    world.set_pose(
        spaceling.body(),
        position,
        parent.angle - std::f32::consts::FRAC_PI_2,
        true,
    );
    world.set_velocity(
        spaceling.body(),
        world.velocity_at_point(floor, position).unwrap(),
        0.25,
        true,
    );
    spaceling.set_balance(&mut world, SpacelingBalance::KnockedDown);
    for _ in 0..8 {
        let gravity = Vec2::new(0.0, -180.0).rotate_radians(world.motion(floor).unwrap().angle);
        spaceling.up = gravity.normalized() * -1.0;
        world.apply_velocity_delta(spaceling.body(), gravity * DT, true);
        world.step(DT);
    }
    assert!(spaceling.snapshot(&world).unwrap().grounded());
    for _ in 0..180 {
        let gravity = Vec2::new(0.0, -180.0).rotate_radians(world.motion(floor).unwrap().angle);
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl {
                walk: 0.0,
                jump_held: true,
            },
            gravity,
        );
    }
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert_eq!(
        snapshot.get_up_result,
        SpacelingGetUpResult::Succeeded,
        "{snapshot:?}"
    );
    assert_eq!(snapshot.balance, SpacelingBalance::Balanced, "{snapshot:?}");
    assert!(snapshot.grounded(), "{snapshot:?}");
    assert!(angle_error(snapshot.up, snapshot.motion.angle).abs() < 0.15);
    assert!(snapshot.relative_speed.abs() < 0.15);
    assert_eq!(snapshot.jumps, 0);
}

#[test]
fn remeshing_a_supporting_body_does_not_restart_recovery() {
    let (mut world, mut spaceling) = prone_notch();
    spaceling.set_balance(&mut world, SpacelingBalance::Recovering);
    spaceling.recovery_support = Some(BodyId::new(FLOOR, BodyRole::PRIMARY));
    for _ in 0..6 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
    }
    let before = spaceling.snapshot(&world).unwrap();
    assert_eq!(before.balance, SpacelingBalance::Recovering);
    let replacement = ColliderId::new(FLOOR, ColliderRole::PRIMARY, 7);
    assert!(world.replace_colliders(
        BodyId::new(FLOOR, BodyRole::PRIMARY),
        ColliderRole::PRIMARY,
        &[ColliderSpec::cuboid(replacement, 100.0, 0.5)]
    ));
    for _ in 0..2 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
    }
    let after = spaceling.snapshot(&world).unwrap();
    assert_eq!(after.support.unwrap().collider, replacement);
    assert_eq!(after.balance, SpacelingBalance::Recovering);
    assert!(
        after.recovery_progress >= before.recovery_progress,
        "{before:?} -> {after:?}"
    );
}

#[test]
fn get_up_does_not_provide_an_airborne_or_zero_gravity_boost() {
    for gravity in [GRAVITY, Vec2::ZERO] {
        let (mut world, mut spaceling) = prone_notch();
        if gravity != Vec2::ZERO {
            world.remove_entity(FLOOR);
            world.remove_entity(PhysicsId::new(3));
            world.remove_entity(PhysicsId::new(4));
            world.step(DT);
        }
        let before = world.motion(spaceling.body()).unwrap();
        spaceling.apply_control(
            &mut world,
            SpacelingControl {
                walk: 0.0,
                jump_held: true,
            },
            gravity,
            DT,
        );
        let after = spaceling.snapshot(&world).unwrap();
        assert_eq!(after.motion, before);
        assert_eq!(
            after.get_up_result,
            if gravity == Vec2::ZERO {
                SpacelingGetUpResult::NoGravity
            } else {
                SpacelingGetUpResult::NoSupport
            }
        );
        assert_eq!(after.jumps, 0);
    }
}

#[test]
fn removing_support_gravity_or_balance_interrupts_an_active_get_up() {
    for expected in [
        SpacelingGetUpResult::NoSupport,
        SpacelingGetUpResult::NoGravity,
        SpacelingGetUpResult::Unsettled,
    ] {
        let (mut world, mut spaceling) = prone_notch();
        let held = SpacelingControl {
            walk: 0.0,
            jump_held: true,
        };
        tick(&mut world, &mut spaceling, held, GRAVITY);
        assert_eq!(
            spaceling.snapshot(&world).unwrap().get_up_result,
            SpacelingGetUpResult::Started
        );
        let mut gravity = GRAVITY;
        match expected {
            SpacelingGetUpResult::NoSupport => {
                world.remove_entity(FLOOR);
            }
            SpacelingGetUpResult::NoGravity => {
                gravity = Vec2::ZERO;
            }
            SpacelingGetUpResult::Unsettled => {
                world.set_velocity(spaceling.body(), Vec2::new(20.0, 0.0), 20.0, true);
            }
            _ => unreachable!(),
        }
        let before = world.motion(spaceling.body()).unwrap();
        spaceling.apply_control(&mut world, held, gravity, DT);
        let after = spaceling.snapshot(&world).unwrap();
        assert_eq!(after.get_up_result, expected, "{after:?}");
        assert_eq!(after.balance, SpacelingBalance::KnockedDown);
        assert_eq!(
            after.motion, before,
            "an interrupted get-up must stop driving"
        );
        assert!(spaceling.get_up_assist.is_none());
    }
}
