use super::*;
use crate::rover::{PlanetAssembly, PlanetSpec};
use crate::world::{BodyKind, PhysicsWorldConfig};

const DT: f32 = 1.0 / 60.0;
const GRAVITY: Vec2 = Vec2::new(0.0, -18.0);
const FLOOR: PhysicsId = PhysicsId::new(1);
const SPACELING: PhysicsId = PhysicsId::new(2);

mod balance;
mod get_up;
mod jetpack;

fn tick(
    world: &mut PhysicsWorld,
    spaceling: &mut SpacelingAssembly,
    control: SpacelingControl,
    gravity: Vec2,
) {
    world.clear_forces();
    spaceling.apply_control(world, control, gravity, DT);
    world.apply_velocity_delta(spaceling.body(), gravity * DT, true);
    world.step(DT);
}

fn floor_fixture(angle: f32, velocity: Vec2) -> (PhysicsWorld, SpacelingAssembly) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let normal = Vec2::Y.rotate_radians(angle);
    assert!(world.insert_body(
        BodyId::new(FLOOR, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::KinematicVelocity,
            position: normal * -0.5,
            angle,
            linear_velocity: velocity,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(FLOOR, ColliderRole::PRIMARY, 0),
            100.0,
            0.5
        )],
    ));
    let spaceling = SpacelingAssembly::insert(
        &mut world,
        SPACELING,
        Vec2::Y * 1.2,
        0.0,
        SpacelingSpec::default(),
    )
    .unwrap();
    (world, spaceling)
}

fn settle(world: &mut PhysicsWorld, spaceling: &mut SpacelingAssembly) {
    for _ in 0..120 {
        tick(world, spaceling, SpacelingControl::default(), GRAVITY);
    }
    assert!(spaceling.snapshot(world).unwrap().grounded());
}

#[test]
fn single_body_walks_both_directions_and_stops_relative_to_ground() {
    for direction in [-1.0, 1.0] {
        let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
        assert_eq!(world.body_count(), 2);
        assert_eq!(world.collider_count(), 2);
        settle(&mut world, &mut spaceling);
        for _ in 0..120 {
            tick(
                &mut world,
                &mut spaceling,
                SpacelingControl {
                    walk: direction,
                    jump_held: false,
                },
                GRAVITY,
            );
        }
        let moving = spaceling.snapshot(&world).unwrap();
        assert!(moving.grounded());
        assert!(moving.motion.position.x * direction > 8.0, "{moving:?}");
        assert!((moving.relative_speed - direction * 5.0).abs() < 0.2);
        for _ in 0..60 {
            tick(
                &mut world,
                &mut spaceling,
                SpacelingControl::default(),
                GRAVITY,
            );
        }
        assert!(spaceling.snapshot(&world).unwrap().relative_speed.abs() < 0.05);
    }
}

#[test]
fn jump_requires_a_new_press_and_real_support() {
    let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
    settle(&mut world, &mut spaceling);
    let held = SpacelingControl {
        jump_held: true,
        ..SpacelingControl::default()
    };
    let mut peak: f32 = 0.0;
    for _ in 0..240 {
        tick(&mut world, &mut spaceling, held, GRAVITY);
        peak = peak.max(spaceling.snapshot(&world).unwrap().motion.position.y);
    }
    let landed = spaceling.snapshot(&world).unwrap();
    assert!(peak > 2.4);
    assert!(landed.grounded());
    assert_eq!(landed.jumps, 1);
    assert_eq!(landed.balance, SpacelingBalance::Balanced);
    assert_eq!(landed.knockdowns, 0, "ordinary jumps are not knockdowns");
    tick(
        &mut world,
        &mut spaceling,
        SpacelingControl::default(),
        GRAVITY,
    );
    tick(&mut world, &mut spaceling, held, GRAVITY);
    assert_eq!(spaceling.snapshot(&world).unwrap().jumps, 2);
    assert!(!spaceling.snapshot(&world).unwrap().grounded());
    tick(
        &mut world,
        &mut spaceling,
        SpacelingControl::default(),
        GRAVITY,
    );
    tick(&mut world, &mut spaceling, held, GRAVITY);
    assert_eq!(
        spaceling.snapshot(&world).unwrap().jumps,
        2,
        "no double jump"
    );
}

#[test]
fn walking_and_idling_use_translating_support_velocity() {
    for direction in [0.0, -1.0, 1.0] {
        let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::new(3.0, 0.0));
        settle(&mut world, &mut spaceling);
        for _ in 0..120 {
            tick(
                &mut world,
                &mut spaceling,
                SpacelingControl {
                    walk: direction,
                    jump_held: false,
                },
                GRAVITY,
            );
        }
        let snapshot = spaceling.snapshot(&world).unwrap();
        assert!(snapshot.grounded());
        assert!((snapshot.relative_speed - direction * 5.0).abs() < 0.2);
        assert!((snapshot.motion.linear_velocity.x - (3.0 + direction * 5.0)).abs() < 0.2);
    }
}

#[test]
fn walkable_slopes_support_motion_in_both_directions() {
    for angle in [-0.35, 0.35] {
        for direction in [-1.0, 1.0] {
            let (mut world, mut spaceling) = floor_fixture(angle, Vec2::ZERO);
            settle(&mut world, &mut spaceling);
            for _ in 0..90 {
                tick(
                    &mut world,
                    &mut spaceling,
                    SpacelingControl {
                        walk: direction,
                        jump_held: false,
                    },
                    GRAVITY,
                );
            }
            let snapshot = spaceling.snapshot(&world).unwrap();
            assert!(snapshot.grounded(), "{angle} {direction}: {snapshot:?}");
            assert!(snapshot.motion.position.x * direction > 5.0, "{snapshot:?}");
        }
    }
}

#[test]
fn walls_and_ceilings_do_not_grant_grounded_jumps() {
    for angle in [std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
        let (mut world, mut spaceling) = floor_fixture(angle, Vec2::ZERO);
        let normal = Vec2::Y.rotate_radians(angle);
        world.set_pose(spaceling.body(), normal * 0.28, 0.0, true);
        // Maintain contact from the side/above, but define up from ordinary gravity.
        for _ in 0..20 {
            world.set_velocity(spaceling.body(), normal * -2.0, 0.0, true);
            tick(
                &mut world,
                &mut spaceling,
                SpacelingControl::default(),
                GRAVITY,
            );
        }
        let snapshot = spaceling.snapshot(&world).unwrap();
        assert!(
            snapshot.contacts > 0,
            "fixture must actually touch: {snapshot:?}"
        );
        assert!(!snapshot.grounded(), "{snapshot:?}");
        assert!(!spaceling.apply_control(
            &mut world,
            SpacelingControl {
                jump_held: true,
                walk: 0.0
            },
            GRAVITY,
            DT
        ));
    }
}

#[test]
fn removing_support_immediately_clears_ground_and_prevents_jump() {
    let (mut world, mut spaceling) = floor_fixture(0.0, Vec2::ZERO);
    settle(&mut world, &mut spaceling);
    assert!(world.remove_entity(FLOOR));
    assert!(!spaceling.snapshot(&world).unwrap().grounded());
    tick(
        &mut world,
        &mut spaceling,
        SpacelingControl {
            jump_held: true,
            walk: 0.0,
        },
        GRAVITY,
    );
    assert_eq!(spaceling.snapshot(&world).unwrap().jumps, 0);
    for _ in 0..30 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            GRAVITY,
        );
    }
    assert!(spaceling.snapshot(&world).unwrap().motion.position.y < 0.0);
}

#[test]
fn free_space_retains_up_and_momentum_without_ground_or_autobraking() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let mut spaceling = SpacelingAssembly::insert(
        &mut world,
        SPACELING,
        Vec2::ZERO,
        0.7,
        SpacelingSpec::default(),
    )
    .unwrap();
    world.set_velocity(spaceling.body(), Vec2::new(2.0, 1.0), 0.0, true);
    for _ in 0..60 {
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl {
                jump_held: true,
                walk: 0.0,
            },
            Vec2::ZERO,
        );
    }
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert!(!snapshot.grounded());
    assert_eq!(snapshot.jumps, 0);
    assert!((snapshot.motion.position - Vec2::new(2.0, 1.0)).length() < 0.001);
    assert!((snapshot.motion.angle - 0.7).abs() < 0.001);
}

#[test]
fn idle_character_tracks_rotating_planet_and_contact_point_velocity() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let planet = PlanetAssembly::insert(
        &mut world,
        FLOOR,
        PlanetSpec {
            center: Vec2::ZERO,
            radius: 20.0,
            angle: 0.0,
        },
        &[],
    )
    .unwrap();
    let mut spaceling = SpacelingAssembly::insert(
        &mut world,
        SPACELING,
        Vec2::new(0.0, 21.0),
        0.0,
        SpacelingSpec::default(),
    )
    .unwrap();
    let mut supported = 0;
    for index in 0..1200 {
        planet.set_next_pose(&mut world, Vec2::ZERO, (index + 1) as f32 * DT * 0.08);
        let position = world.motion(spaceling.body()).unwrap().position;
        tick(
            &mut world,
            &mut spaceling,
            SpacelingControl::default(),
            position.normalized() * -18.0,
        );
        let snapshot = spaceling.snapshot(&world).unwrap();
        supported += usize::from(snapshot.grounded());
        if let Some(support) = snapshot.support {
            let expected = Vec2::new(-support.position.y, support.position.x) * 0.08;
            assert!((support.velocity - expected).length() < 0.001);
            assert!((support.angular_velocity - 0.08).abs() < 0.001);
        }
    }
    let snapshot = spaceling.snapshot(&world).unwrap();
    assert!(supported > 1150, "grounded {supported}/1200");
    assert!((snapshot.motion.position.length() - 20.9).abs() < 0.1);
    assert!(snapshot.motion.position.x < -18.0, "{snapshot:?}");
    assert!(snapshot.relative_speed.abs() < 0.2);
}

#[test]
fn invalid_specs_do_not_insert_partial_entities_and_control_is_bounded() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    assert!(
        SpacelingAssembly::insert(
            &mut world,
            SPACELING,
            Vec2::ZERO,
            0.0,
            SpacelingSpec {
                radius: f32::NAN,
                ..SpacelingSpec::default()
            }
        )
        .is_none()
    );
    assert_eq!(world.entity_count(), 0);
    let mut spaceling = SpacelingAssembly::insert(
        &mut world,
        SPACELING,
        Vec2::ZERO,
        1.0,
        SpacelingSpec::default(),
    )
    .unwrap();
    spaceling.apply_control(
        &mut world,
        SpacelingControl {
            walk: 100.0,
            jump_held: false,
        },
        GRAVITY,
        DT,
    );
    let motion = world.motion(spaceling.body()).unwrap();
    assert!(motion.angular_velocity.abs() <= 60.0 * DT + 1e-5);
    assert!(motion.linear_velocity.length() <= 7.0 * DT + 1e-5);
    assert_eq!(
        motion.position,
        Vec2::ZERO,
        "controller must not integrate pose"
    );
    spaceling.apply_control(
        &mut world,
        SpacelingControl {
            walk: f32::NAN,
            jump_held: true,
        },
        GRAVITY,
        DT,
    );
    assert!(
        world
            .motion(spaceling.body())
            .unwrap()
            .linear_velocity
            .x
            .is_finite()
    );
}
