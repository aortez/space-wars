//! Entities whose colliders pass through each other's, as a round leaving the
//! tank that fired it does.
use super::*;
use crate::world::loading::CaptureConfig;

const DT: f32 = 1.0 / 60.0;

fn collider(entity: u64) -> ColliderId {
    ColliderId::new(PhysicsId::new(entity), ColliderRole::PRIMARY, 0)
}

fn body(entity: u64) -> BodyId {
    BodyId::new(PhysicsId::new(entity), BodyRole::PRIMARY)
}

fn entity(value: u64) -> PhysicsId {
    PhysicsId::new(value)
}

/// A fixed floor, 20 wide and 2 deep, with its top at `top`.
fn insert_floor(world: &mut PhysicsWorld, floor: u64, top: f32) {
    assert!(world.insert_body(
        body(floor),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(0.0, top - 1.0),
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(collider(floor), 10.0, 1.0)],
    ));
}

/// A ball, entity 2, of radius 0.5 at height 3.
fn insert_ball(world: &mut PhysicsWorld) {
    assert!(world.insert_body(
        body(2),
        BodySpec {
            position: Vec2::new(0.0, 3.0),
            can_sleep: false,
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(collider(2), 0.5)],
    ));
}

/// Two floors under gravity, entity 1 with its top at height 1 and entity 3
/// with its top at -5.
fn floors(collect_events: bool) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -10.0),
        collect_events,
        ..PhysicsWorldConfig::default()
    });
    insert_floor(&mut world, 1, 1.0);
    insert_floor(&mut world, 3, -5.0);
    world
}

/// The two floors, with the ball above them, falling.
fn floors_and_ball(collect_events: bool) -> PhysicsWorld {
    let mut world = floors(collect_events);
    insert_ball(&mut world);
    world
}

/// The ball's height after `steps` more steps.
fn fall(world: &mut PhysicsWorld, steps: usize) -> f32 {
    for _ in 0..steps {
        world.step(DT);
    }
    world.motion(body(2)).unwrap().position.y
}

/// The ball lands on the upper floor, unless it passes through it, when it
/// lands on the lower one. That holds whether or not the world collects
/// contact events, and whichever of the pair is named first.
#[test]
fn a_ball_passes_through_one_floor_and_lands_on_the_next() {
    let mut world = floors_and_ball(true);
    let resting = fall(&mut world, 180);
    assert!((resting - 1.5).abs() < 0.05, "{resting}");
    for collect_events in [true, false] {
        for (passing, other) in [(2, 1), (1, 2)] {
            let mut world = floors_and_ball(collect_events);
            assert!(world.pass_through(entity(passing), entity(other)));
            assert_eq!(world.passes_through(entity(passing)), Some(entity(other)));
            let mut touched = false;
            for _ in 0..180 {
                world.step(DT);
                touched |= world
                    .contact_events()
                    .iter()
                    .any(|event| [event.collider_a, event.collider_b].contains(&collider(1)));
            }
            let landed = world.motion(body(2)).unwrap().position.y;
            assert!((landed + 4.5).abs() < 0.05, "{landed}");
            assert!(!touched);
        }
    }
}

/// From the step after a pass-through ends, the two meet again: a ball
/// halfway into the floor it was passing through is pushed back out on top.
#[test]
fn ending_a_pass_through_lets_the_two_meet_again() {
    let mut world = floors_and_ball(true);
    assert!(!world.pass_through(entity(2), entity(2)));
    assert!(!world.pass_through(entity(9), entity(1)));
    assert!(!world.pass_through(entity(2), entity(9)));
    assert!(!world.end_pass_through(entity(2)));
    assert!(world.pass_through(entity(2), entity(1)));
    let mut steps = 0;
    while world.motion(body(2)).unwrap().position.y > 0.6 {
        world.step(DT);
        steps += 1;
        assert!(steps < 120, "the ball never reached the floor");
    }
    assert!(world.end_pass_through(entity(2)));
    assert_eq!(world.passes_through(entity(2)), None);
    assert!(!world.end_pass_through(entity(2)));
    let resting = fall(&mut world, 180);
    assert!((resting - 1.5).abs() < 0.05, "{resting}");
}

/// Ending a pass-through takes effect from the next step even where nothing
/// moves: a ball asleep halfway into a floor, with no gravity, meets the floor
/// at once and is pushed back out on top.
#[test]
fn ending_a_pass_through_at_rest_pushes_the_ball_out() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        collect_events: true,
        ..PhysicsWorldConfig::default()
    });
    insert_floor(&mut world, 1, 1.0);
    assert!(world.insert_body(
        body(2),
        BodySpec {
            position: Vec2::new(0.0, 0.5),
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(collider(2), 0.5)],
    ));
    assert!(world.pass_through(entity(2), entity(1)));
    let asleep =
        |world: &PhysicsWorld| world.raw.bodies[world.body_handle(body(2)).unwrap()].is_sleeping();
    let mut steps = 0;
    while !asleep(&world) {
        world.step(DT);
        steps += 1;
        assert!(steps < 600, "the ball never slept");
    }
    assert!(world.end_pass_through(entity(2)));
    world.step(DT);
    assert!(
        world
            .contact_events()
            .iter()
            .any(|event| [event.collider_a, event.collider_b].contains(&collider(1)))
    );
    let height = fall(&mut world, 120);
    assert!(height > 1.45, "{height}");
}

/// A pass-through takes effect from the next step even where the pair rests
/// on each other: a ball asleep on the upper floor falls through it at once.
#[test]
fn a_ball_asleep_on_a_floor_falls_through_it_at_once() {
    let mut world = floors(true);
    assert!(world.insert_body(
        body(2),
        BodySpec {
            position: Vec2::new(0.0, 1.5),
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(collider(2), 0.5)],
    ));
    let asleep =
        |world: &PhysicsWorld| world.raw.bodies[world.body_handle(body(2)).unwrap()].is_sleeping();
    let mut steps = 0;
    while !asleep(&world) {
        world.step(DT);
        steps += 1;
        assert!(steps < 600, "the ball never slept");
    }
    assert!(world.pass_through(entity(1), entity(2)));
    world.step(DT);
    assert!(
        !world
            .contact_events()
            .iter()
            .any(|event| [event.collider_a, event.collider_b].contains(&collider(1)))
    );
    let landed = fall(&mut world, 180);
    assert!((landed + 4.5).abs() < 0.05, "{landed}");
}

/// A round born inside its own tank, in a world set up as Spacewars sets up
/// its own, falls through untouched while it passes through, and is pushed
/// out otherwise.
#[test]
fn a_ball_born_inside_a_floor_passes_through_it_with_spacewars_settings() {
    for passing in [false, true] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -10.0),
            max_ccd_substeps: 4,
            ..PhysicsWorldConfig::default()
        });
        world
            .enable_loading_capture(CaptureConfig::default())
            .unwrap();
        world.enable_speculative_arrivals().unwrap();
        insert_floor(&mut world, 1, 1.0);
        insert_floor(&mut world, 3, -5.0);
        assert!(world.insert_body(
            body(2),
            BodySpec {
                position: Vec2::new(0.0, 0.4),
                linear_velocity: Vec2::new(0.0, -10.0),
                can_sleep: false,
                ccd_enabled: true,
                ..BodySpec::default()
            },
            &[ColliderSpec::ball(collider(2), 0.25)],
        ));
        if passing {
            assert!(world.pass_through(entity(2), entity(1)));
        }
        let height = fall(&mut world, 180);
        if passing {
            assert!((height + 4.75).abs() < 0.05, "{height}");
        } else {
            assert!((height - 1.25).abs() < 0.05, "{height}");
        }
    }
}

/// Continuous collision detection keeps the pair apart too, with one CCD
/// substep or the four Spacewars takes: a fast ball that would stop at a thin
/// wall passes through it.
#[test]
fn a_fast_ball_passes_through_a_thin_wall_under_ccd() {
    for (passing, substeps) in [(false, 1), (true, 1), (false, 4), (true, 4)] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            max_ccd_substeps: substeps,
            ..PhysicsWorldConfig::default()
        });
        assert!(world.insert_body(
            body(1),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(5.0, 0.0),
                ..BodySpec::default()
            },
            &[ColliderSpec::cuboid(collider(1), 0.05, 2.0)],
        ));
        assert!(world.insert_body(
            body(2),
            BodySpec {
                linear_velocity: Vec2::new(300.0, 0.0),
                can_sleep: false,
                ccd_enabled: true,
                ..BodySpec::default()
            },
            &[ColliderSpec::ball(collider(2), 0.1)],
        ));
        if passing {
            assert!(world.pass_through(entity(2), entity(1)));
        }
        for _ in 0..10 {
            world.step(DT);
        }
        let x = world.motion(body(2)).unwrap().position.x;
        if passing {
            assert!(x > 40.0, "{x} with {substeps} substeps");
        } else {
            assert!(x < 5.0, "{x} with {substeps} substeps");
        }
    }
}

/// A clone and a world rebuilt from a snapshot keep the pass-through, and let
/// the ball through as the original does. A world with a loading capture, as
/// Spacewars runs, clones through its snapshot. The snapshot also carries a
/// weight, whose section comes before the pass-through's.
#[test]
fn clones_and_snapshots_keep_a_pass_through() {
    for capture in [false, true] {
        let mut world = floors_and_ball(true);
        if capture {
            world
                .enable_loading_capture(CaptureConfig::default())
                .unwrap();
            world.enable_speculative_arrivals().unwrap();
        }
        assert!(world.pass_through(entity(2), entity(1)));
        world.step(DT);
        assert!(world.carry_weight(body(2)));
        let snapshot = world.snapshot_bytes().unwrap();
        let restored = PhysicsWorld::from_snapshot_bytes(&snapshot).unwrap();
        for mut copy in [world.clone(), restored] {
            assert_eq!(copy.passes_through(entity(2)), Some(entity(1)));
            let landed = fall(&mut copy, 180);
            assert!((landed + 4.5).abs() < 0.05, "{landed}");
        }
    }
}

/// Colliders an entity gains while it passes through pass through too, and
/// an entity removed and inserted again meets everything.
#[test]
fn new_colliders_pass_through_and_a_reinserted_entity_meets_everything() {
    let mut world = floors_and_ball(true);
    assert!(world.pass_through(entity(2), entity(1)));
    assert!(world.replace_colliders(
        body(2),
        ColliderRole::PRIMARY,
        &[ColliderSpec::ball(collider(2), 0.6)],
    ));
    let landed = fall(&mut world, 180);
    assert!((landed + 4.4).abs() < 0.05, "{landed}");
    assert!(world.remove_entity(entity(2)));
    assert_eq!(world.passes_through(entity(2)), None);
    insert_ball(&mut world);
    let resting = fall(&mut world, 180);
    assert!((resting - 1.5).abs() < 0.05, "{resting}");
}

/// Removing the entity passed through, or moving it to another world, ends
/// the pass-through too, so one inserted again under its identity meets the
/// ball. An entity moved to another world passes through there what it passed
/// through here.
#[test]
fn removing_the_other_ends_a_pass_through_and_moving_carries_it() {
    let mut world = floors_and_ball(true);
    assert!(world.pass_through(entity(2), entity(1)));
    assert!(world.remove_entity(entity(1)));
    assert_eq!(world.passes_through(entity(2)), None);
    insert_floor(&mut world, 1, 1.0);
    let resting = fall(&mut world, 180);
    assert!((resting - 1.5).abs() < 0.05, "{resting}");

    let mut world = floors_and_ball(true);
    let mut target = PhysicsWorld::new(PhysicsWorldConfig::default());
    assert!(world.pass_through(entity(2), entity(1)));
    assert!(world.transfer_entity(entity(1), &mut target));
    assert_eq!(world.passes_through(entity(2)), None);

    let mut world = floors_and_ball(true);
    let mut target = floors(true);
    assert!(world.pass_through(entity(2), entity(1)));
    assert!(world.transfer_entity(entity(2), &mut target));
    assert_eq!(world.passes_through(entity(2)), None);
    assert_eq!(target.passes_through(entity(2)), Some(entity(1)));
    let landed = fall(&mut target, 180);
    assert!((landed + 4.5).abs() < 0.05, "{landed}");
}
