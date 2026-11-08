//! A carried body meets its whole weight at once in the step that follows,
//! and only in that step.
use super::*;

const DT: f32 = 1.0 / 60.0;
const BALL: PhysicsId = PhysicsId::new(1);

fn ball_id() -> BodyId {
    BodyId::new(BALL, BodyRole::PRIMARY)
}

/// A ball in the air under Clock's gravity, at the given gravity scale.
fn falling_ball(gravity_scale: f32) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -400.0),
        length_unit: 20.0,
        ..PhysicsWorldConfig::default()
    });
    insert_ball(&mut world, gravity_scale);
    world
}

fn insert_ball(world: &mut PhysicsWorld, gravity_scale: f32) {
    assert!(world.insert_body(
        ball_id(),
        BodySpec {
            position: Vec2::new(0.0, 100.0),
            gravity_scale,
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(BALL, ColliderRole::PRIMARY, 0),
            1.0
        )],
    ));
}

/// Hold the ball up against the weight its next step brings.
fn hold_up(world: &mut PhysicsWorld, gravity: Vec2) {
    let mass = world.body_mass(ball_id()).unwrap();
    assert!(world.apply_impulse(ball_id(), -gravity * (mass * DT), true));
}

#[test]
fn a_carried_body_meets_its_weight_at_once_for_one_step_only() {
    for scale in [1.0, 0.5] {
        let mut world = falling_ball(scale);
        let gravity = Vec2::new(0.0, -400.0) * scale;
        hold_up(&mut world, gravity);
        assert!(world.carry_weight(ball_id()));
        let before = world.motion(ball_id()).unwrap();
        world.step(DT);
        let after = world.motion(ball_id()).unwrap();
        // The hold and the weight cancel exactly, all through the step.
        assert!(
            after.position.distance_to(before.position) < 1e-4,
            "{scale}"
        );
        assert!(after.linear_velocity.length() < 1e-4, "{scale}");
        assert_eq!(world.gravity_scale(ball_id()), Some(scale));
        // Nobody carries it again, so it falls under its own gravity.
        world.step(DT);
        let next = world.motion(ball_id()).unwrap();
        assert!(
            (next.linear_velocity - gravity * DT).length() < 1e-3,
            "{scale}"
        );
        assert!(next.position.y < after.position.y);
    }
    // Held up the same way but not carried, the substeps lift it a little.
    let mut world = falling_ball(1.0);
    hold_up(&mut world, Vec2::new(0.0, -400.0));
    let before = world.motion(ball_id()).unwrap();
    world.step(DT);
    assert!(world.motion(ball_id()).unwrap().position.y > before.position.y + 1e-3);
}

#[test]
fn a_carry_waits_for_a_real_step_keeps_through_a_snapshot_and_ends_with_its_body() {
    let gravity = Vec2::new(0.0, -400.0);
    let mut world = falling_ball(1.0);
    hold_up(&mut world, gravity);
    assert!(world.carry_weight(ball_id()));
    assert!(world.carry_weight(ball_id()));
    world.step(0.0);
    world.step(f32::NAN);
    let restored = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    for (copy, mut world) in [
        ("clone", world.clone()),
        ("snapshot", restored),
        ("original", world),
    ] {
        let before = world.motion(ball_id()).unwrap();
        world.step(DT);
        let after = world.motion(ball_id()).unwrap();
        assert!(after.position.distance_to(before.position) < 1e-4, "{copy}");
        assert!(after.linear_velocity.length() < 1e-4, "{copy}");
    }
    // What has no weight of its own to carry is refused.
    let mut world = falling_ball(1.0);
    let post = PhysicsId::new(2);
    assert!(world.insert_body(
        BodyId::new(post, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(post, ColliderRole::PRIMARY, 0),
            1.0
        )],
    ));
    assert!(!world.carry_weight(BodyId::new(post, BodyRole::PRIMARY)));
    assert!(!world.carry_weight(BodyId::new(PhysicsId::new(3), BodyRole::PRIMARY)));
    // A body that leaves takes its carry with it, so one put in its place
    // falls as usual.
    assert!(world.carry_weight(ball_id()));
    assert!(world.remove_entity(BALL));
    insert_ball(&mut world, 1.0);
    world.step(DT);
    let velocity = world.motion(ball_id()).unwrap().linear_velocity;
    assert!((velocity - gravity * DT).length() < 1e-3, "{velocity:?}");
}
