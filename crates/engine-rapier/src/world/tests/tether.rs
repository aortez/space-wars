//! A rope holds its length against a load, lets its ends close freely, and
//! leaves with its own identity, whichever end goes first.
use super::*;

const DT: f32 = 1.0 / 60.0;
const POST: PhysicsId = PhysicsId::new(1);
const BOB: PhysicsId = PhysicsId::new(2);
const OTHER: PhysicsId = PhysicsId::new(3);

fn post_id() -> BodyId {
    BodyId::new(POST, BodyRole::PRIMARY)
}

fn bob_id() -> BodyId {
    BodyId::new(BOB, BodyRole::PRIMARY)
}

fn rope_id() -> JointId {
    JointId::new(BOB, JointRole::new(7))
}

/// A fixed post and a ball of radius `r` at `bob`, under gravity `g` at the
/// given scale.
fn fixture(g: f32, r: f32, length_unit: f32, bob: Vec2, velocity: Vec2) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -g),
        length_unit,
        ..PhysicsWorldConfig::default()
    });
    assert!(world.insert_body(
        post_id(),
        BodySpec {
            kind: BodyKind::Fixed,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(POST, ColliderRole::PRIMARY, 0),
            r,
            r
        )],
    ));
    assert!(world.insert_body(
        bob_id(),
        BodySpec {
            position: bob,
            linear_velocity: velocity,
            lock_rotation: true,
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(BOB, ColliderRole::PRIMARY, 0),
            r
        )],
    ));
    world
}

fn tie(world: &mut PhysicsWorld, r: f32, length: f32) {
    assert!(world.insert_tether(
        rope_id(),
        TetherSpec {
            anchor_body: post_id(),
            anchor: Vec2::new(0.0, -r),
            body: bob_id(),
            attachment: Vec2::ZERO,
            length,
        },
    ));
}

#[test]
fn a_pendulum_keeps_the_textbook_period_at_both_scales() {
    // Clock's scale, and metres.
    for (g, r, unit) in [(400.0_f32, 6.0_f32, 20.0_f32), (9.81, 0.05, 1.0)] {
        let length = 20.0 * r;
        let theta = 0.3_f32;
        let start = Vec2::new(theta.sin(), -theta.cos()) * length - Vec2::new(0.0, r);
        let mut world = fixture(g, r, unit, start, Vec2::ZERO);
        tie(&mut world, r, length);
        let mass = world.body_mass(bob_id()).unwrap();
        let mut last = start.x;
        let mut crossings = Vec::new();
        for tick in 0..400 {
            world.step(DT);
            let x = world.motion(bob_id()).unwrap().position.x;
            if (last > 0.0) != (x > 0.0) {
                crossings.push(tick);
            }
            last = x;
            let rope = world.tether(rope_id()).unwrap();
            assert!((rope.distance - length).abs() < 1e-3 * length, "{rope:?}");
            // Between the weight's share along the rope at the ends of the
            // swing and that plus the centripetal pull at the bottom.
            let weights = rope.tension / (mass * g);
            assert!((0.9..1.15).contains(&weights), "g {g}: {weights}");
        }
        let expected =
            2.0 * std::f32::consts::PI * (length / g).sqrt() * (1.0 + theta * theta / 16.0);
        for pair in crossings.windows(3) {
            let period = (pair[2] - pair[0]) as f32 * DT;
            assert!(
                (period - expected).abs() < 1.5 * DT,
                "g {g}: {period} for {expected}"
            );
        }
        assert!(crossings.len() >= 4, "g {g}: {crossings:?}");
    }
}

#[test]
fn a_slack_rope_neither_pulls_nor_pushes_until_it_goes_taut() {
    let (g, r) = (400.0, 6.0);
    let mut world = fixture(
        g,
        r,
        20.0,
        Vec2::new(0.0, -5.0 * r),
        Vec2::new(10.0 * r, 0.0),
    );
    let length = 12.0 * r;
    tie(&mut world, r, length);
    let mut taut = None;
    for tick in 0..120 {
        let before = world.motion(bob_id()).unwrap().linear_velocity;
        world.step(DT);
        let after = world.motion(bob_id()).unwrap().linear_velocity;
        let rope = world.tether(rope_id()).unwrap();
        if rope.slack() > 1e-3 * r {
            // Ballistic: gravity alone, though the ends close and part.
            assert_eq!(rope.tension, 0.0, "tick {tick}");
            assert!(((after - before) / DT - Vec2::new(0.0, -g)).length() < 1e-2 * g);
        } else if taut.is_none() {
            taut = Some(tick);
        }
        assert!(
            rope.distance <= length * (1.0 + 1e-3),
            "tick {tick}: {rope:?}"
        );
    }
    assert!(taut.is_some_and(|tick| tick > 10), "{taut:?}");
}

#[test]
fn a_rope_changes_length_only_when_asked_and_refuses_what_it_cannot_be() {
    let r = 6.0;
    let mut world = fixture(400.0, r, 20.0, Vec2::new(0.0, -11.0 * r), Vec2::ZERO);
    tie(&mut world, r, 10.0 * r);
    assert!(world.set_tether_length(rope_id(), 14.0 * r));
    // Paying out lets the bob fall to the new length, no further.
    for _ in 0..120 {
        world.step(DT);
    }
    let rope = world.tether(rope_id()).unwrap();
    assert!((rope.distance - 14.0 * r).abs() < 1e-2 * r, "{rope:?}");
    for refused in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(!world.set_tether_length(rope_id(), refused));
    }
    assert_eq!(world.tether(rope_id()).unwrap().length, 14.0 * r);
    // A rope needs two distinct, present bodies and a fresh identity.
    let spec = TetherSpec {
        anchor_body: post_id(),
        anchor: Vec2::ZERO,
        body: bob_id(),
        attachment: Vec2::ZERO,
        length: r,
    };
    assert!(!world.insert_tether(rope_id(), spec));
    let other = JointId::new(BOB, JointRole::new(8));
    assert!(!world.insert_tether(
        other,
        TetherSpec {
            anchor_body: bob_id(),
            ..spec
        }
    ));
    assert!(!world.insert_tether(
        other,
        TetherSpec {
            anchor_body: BodyId::new(OTHER, BodyRole::PRIMARY),
            ..spec
        }
    ));
    assert!(!world.insert_tether(
        other,
        TetherSpec {
            length: 0.0,
            ..spec
        }
    ));
    assert!(!world.insert_tether(
        other,
        TetherSpec {
            anchor: Vec2::new(f32::NAN, 0.0),
            ..spec
        }
    ));
    assert!(!world.contains_joint(other));
    assert!(!world.set_tether_length(other, r));
    assert_eq!(world.tether(other), None);
    assert_eq!(world.identity_fault(), None);
}

#[test]
fn a_rope_leaves_with_either_end_and_alone_when_removed() {
    let r = 6.0;
    // Removing the rope keeps both bodies; removing either body takes it.
    for removed in [None, Some(POST), Some(BOB)] {
        let mut world = fixture(400.0, r, 20.0, Vec2::new(0.0, -11.0 * r), Vec2::ZERO);
        tie(&mut world, r, 10.0 * r);
        world.step(DT);
        let bodies = world.body_count();
        match removed {
            None => {
                assert!(world.remove_joint(rope_id()));
                assert!(!world.remove_joint(rope_id()));
                assert_eq!(world.body_count(), bodies);
            }
            Some(entity) => assert!(world.remove_entity(entity)),
        }
        assert!(!world.contains_joint(rope_id()));
        assert_eq!(world.joint_count(), 0);
        assert_eq!(world.identity_fault(), None, "{removed:?}");
        // The bob falls freely once the rope is gone.
        if removed != Some(BOB) {
            let before = world.motion(bob_id()).unwrap().linear_velocity;
            world.step(DT);
            let after = world.motion(bob_id()).unwrap().linear_velocity;
            assert!((after - before - Vec2::new(0.0, -400.0) * DT).length() < 1e-3);
        }
    }
}

#[test]
fn a_rope_survives_a_snapshot_with_its_identity_length_and_load() {
    let r = 6.0;
    let mut world = fixture(400.0, r, 20.0, Vec2::new(3.0 * r, -9.0 * r), Vec2::ZERO);
    tie(&mut world, r, 10.0 * r);
    for _ in 0..30 {
        world.step(DT);
    }
    let restored = world.clone();
    assert_eq!(restored.tether(rope_id()), world.tether(rope_id()));
    let mut restored = restored;
    for _ in 0..30 {
        world.step(DT);
        restored.step(DT);
    }
    assert_eq!(restored.tether(rope_id()), world.tether(rope_id()));
    assert_eq!(restored.motion(bob_id()), world.motion(bob_id()));
}
