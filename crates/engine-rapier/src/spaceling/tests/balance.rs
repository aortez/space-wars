use super::*;

fn shove(world: &mut PhysicsWorld, spaceling: &SpacelingAssembly, strength: f32) {
    let motion = world.motion(spaceling.body()).unwrap();
    let mass = world.body_mass(spaceling.body()).unwrap();
    assert!(world.apply_impulse_at_point(
        spaceling.body(),
        Vec2::new(4.0, 6.0) * (mass * strength),
        motion.position + Vec2::new(0.0, 0.6),
        true,
    ));
}

#[test]
fn small_shove_preserves_balance_but_large_shove_disables_control() {
    let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
    settle(&mut world, &mut spaceling);
    shove(&mut world, &spaceling, 0.05);
    tick(
        &mut world,
        &mut spaceling,
        SpacelingControl::default(),
        GRAVITY,
    );
    assert_eq!(
        spaceling.snapshot(&world).unwrap().balance,
        SpacelingBalance::Balanced
    );
    settle(&mut world, &mut spaceling);
    shove(&mut world, &spaceling, 1.0);
    let before = world.motion(spaceling.body()).unwrap();
    assert!(!spaceling.apply_control(
        &mut world,
        SpacelingControl {
            walk: -1.0,
            jump_held: true
        },
        GRAVITY,
        DT,
    ));
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert_eq!(snapshot.balance, SpacelingBalance::KnockedDown);
    assert_eq!(
        snapshot.motion, before,
        "no countersteering, self-righting, or pose snap"
    );
    assert_eq!(snapshot.knockdowns, 1);
    assert!(snapshot.last_knockdown.unwrap().angular_speed >= 8.0);
}

#[test]
fn off_center_shove_tumbles_lands_and_recovers_without_buffering_jump() {
    for support_velocity in [Vec2::ZERO, Vec2::new(3.0, 0.0)] {
        let (mut world, mut spaceling) = floor_fixture(0.0, support_velocity);
        settle(&mut world, &mut spaceling);
        shove(&mut world, &spaceling, 1.0);
        let mut saw_airborne = false;
        let mut saw_recovering = false;
        let mut max_tilt = 0.0_f32;
        for _ in 0..900 {
            tick(
                &mut world,
                &mut spaceling,
                SpacelingControl {
                    walk: -1.0,
                    jump_held: true,
                },
                GRAVITY,
            );
            let snapshot = spaceling.snapshot(&world).unwrap();
            saw_airborne |= !snapshot.grounded();
            saw_recovering |= snapshot.balance == SpacelingBalance::Recovering;
            max_tilt = max_tilt.max(angle_error(snapshot.up, snapshot.motion.angle).abs());
            assert_eq!(snapshot.jumps, 0, "held jump must not fire on recovery");
            if snapshot.recoveries > 0 {
                assert!(snapshot.grounded());
                assert_eq!(snapshot.balance, SpacelingBalance::Balanced);
                assert!(angle_error(snapshot.up, snapshot.motion.angle).abs() < 0.2);
                break;
            }
        }
        let snapshot = spaceling.snapshot(&world).unwrap();
        assert!(
            saw_airborne && saw_recovering && max_tilt > 1.0,
            "{snapshot:?}"
        );
        assert_eq!(snapshot.recoveries, 1, "{snapshot:?}");
        assert_eq!(snapshot.knockdowns, 1);
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl {
                walk: 0.0,
                jump_held: true,
            },
            GRAVITY,
        );
        assert_eq!(spaceling.snapshot(&world).unwrap().jumps, 1);
    }
}

#[test]
fn zero_gravity_preserves_linear_and_angular_momentum_even_with_held_controls() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let mut spaceling = SpacelingAssembly::insert(
        &mut world,
        SPACELING,
        Vec2::ZERO,
        0.0,
        SpacelingSpec::default(),
    )
    .unwrap();
    shove(&mut world, &spaceling, 1.0);
    let before = world.motion(spaceling.body()).unwrap();
    for _ in 0..180 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl {
                walk: -1.0,
                jump_held: true,
            },
            Vec2::ZERO,
        );
    }
    let after = spaceling.snapshot(&world).unwrap();
    assert_eq!(after.balance, SpacelingBalance::KnockedDown);
    assert_eq!(after.recoveries, 0);
    assert_eq!(after.jumps, 0);
    assert!((after.motion.linear_velocity - before.linear_velocity).length() < 1e-5);
    assert!((after.motion.angular_velocity - before.angular_velocity).abs() < 1e-5);
    assert!((after.motion.position - before.linear_velocity * (180.0 * DT)).length() < 0.01);
}

#[test]
fn a_real_high_speed_landing_causes_knockdown() {
    let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
    world.set_pose(spaceling.body(), Vec2::new(0.0, 4.0), 0.0, true);
    world.set_velocity(spaceling.body(), Vec2::new(0.0, -20.0), 0.0, true);
    for _ in 0..30 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
        if spaceling.snapshot(&world).unwrap().knockdowns > 0 {
            break;
        }
    }
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert_eq!(snapshot.knockdowns, 1, "{snapshot:?}");
    assert!(snapshot.last_knockdown.unwrap().velocity_change >= 12.0);
}

#[test]
fn loss_of_support_or_gravity_aborts_recovery() {
    for remove_support in [true, false] {
        let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
        settle(&mut world, &mut spaceling);
        shove(&mut world, &spaceling, 1.0);
        for _ in 0..900 {
            tick(
                &mut world,
                &mut spaceling,
                SpacelingControl::default(),
                GRAVITY,
            );
            if spaceling.snapshot(&world).unwrap().balance == SpacelingBalance::Recovering {
                break;
            }
        }
        assert_eq!(
            spaceling.snapshot(&world).unwrap().balance,
            SpacelingBalance::Recovering
        );
        if remove_support {
            assert!(world.remove_entity(FLOOR));
        }
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            if remove_support { GRAVITY } else { Vec2::ZERO },
        );
        let snapshot = spaceling.snapshot(&world).unwrap();
        assert_eq!(snapshot.balance, SpacelingBalance::KnockedDown);
        assert_eq!(snapshot.recovery_progress, 0.0);
        assert_eq!(snapshot.recoveries, 0);
    }
}

#[test]
fn another_strong_shove_interrupts_recovery() {
    let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
    settle(&mut world, &mut spaceling);
    shove(&mut world, &spaceling, 1.0);
    for _ in 0..900 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
        if spaceling.snapshot(&world).unwrap().balance == SpacelingBalance::Recovering {
            break;
        }
    }
    assert_eq!(
        spaceling.snapshot(&world).unwrap().balance,
        SpacelingBalance::Recovering
    );
    let mass = world.body_mass(spaceling.body()).unwrap();
    world.apply_impulse(spaceling.body(), Vec2::Y * (mass * 20.0), true);
    tick(
        &mut world,
        &mut spaceling,
        SpacelingControl::default(),
        GRAVITY,
    );
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert_eq!(snapshot.balance, SpacelingBalance::KnockedDown);
    assert_eq!(snapshot.knockdowns, 2);
    assert_eq!(snapshot.recovery_progress, 0.0);
}

#[test]
fn sustained_airborne_recovery_is_cancelled_even_if_support_still_exists() {
    let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
    settle(&mut world, &mut spaceling);
    shove(&mut world, &spaceling, 1.0);
    for _ in 0..900 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
        if spaceling.snapshot(&world).unwrap().balance == SpacelingBalance::Recovering {
            break;
        }
    }
    assert_eq!(
        spaceling.snapshot(&world).unwrap().balance,
        SpacelingBalance::Recovering
    );
    // Remove contact, not the supporting entity. The narrow phase observes it
    // on the following step; the bounded grace period must then expire.
    world.set_pose(spaceling.body(), Vec2::new(0.0, 20.0), 0.5, true);
    world.set_velocity(spaceling.body(), Vec2::ZERO, 0.0, true);
    for _ in 0..12 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
    }
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert_eq!(snapshot.balance, SpacelingBalance::KnockedDown);
    assert_eq!(snapshot.recovery_progress, 0.0);
    assert!(!snapshot.grounded());
}

#[test]
fn invalid_balance_tuning_does_not_insert_a_partial_body() {
    for balance in [
        SpacelingBalanceSpec {
            settle_seconds: f32::NAN,
            ..SpacelingBalanceSpec::default()
        },
        SpacelingBalanceSpec {
            recovery_seconds: 0.0,
            ..SpacelingBalanceSpec::default()
        },
        SpacelingBalanceSpec {
            knockdown_angular_speed: 1.0,
            ..SpacelingBalanceSpec::default()
        },
        SpacelingBalanceSpec {
            support_grace_seconds: f32::INFINITY,
            ..SpacelingBalanceSpec::default()
        },
    ] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        assert!(
            SpacelingAssembly::insert(
                &mut world,
                SPACELING,
                Vec2::ZERO,
                0.0,
                SpacelingSpec {
                    balance,
                    ..SpacelingSpec::default()
                },
            )
            .is_none()
        );
        assert_eq!(world.body_count(), 0);
        assert_eq!(world.collider_count(), 0);
    }
}
