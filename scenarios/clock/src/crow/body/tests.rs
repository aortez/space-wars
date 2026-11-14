use super::*;
use crate::{arena::mechanics_config, crow::flight::DT};

fn world(layout: Layout) -> PhysicsWorld {
    PhysicsWorld::new(mechanics_config(layout))
}

/// One perch, in the first column, whose top is at `feet`.
fn one_perch(feet: Vec2) -> [Option<Perch>; PERCH_COLUMNS] {
    let mut perches = [None; PERCH_COLUMNS];
    perches[0] = Some(Perch {
        key: [0, 0, 0],
        feet,
    });
    perches
}

fn step(world: &mut PhysicsWorld, velocity: Vec2) -> (Vec2, Vec2) {
    CROW.drive(world, velocity);
    world.step(DT);
    CROW.motion(world).expect("the bird")
}

/// The world's gravity does not act on a bird: its controller brings its
/// own, and a bird given no velocity keeps none.
#[test]
fn a_bird_feels_only_the_pull_its_controller_gives_it() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = world(layout);
    let start = Vec2::new(0.0, layout.pitch * 3.0);
    CROW.insert(&mut world, start, Vec2::ZERO, layout);
    for _ in 0..30 {
        assert_eq!(step(&mut world, Vec2::ZERO), (start, Vec2::ZERO));
    }
}

/// Faster than a bird ever comes down, in pitches a second. Falling, the
/// drag its controller gives it balances the pull at a little over 11, and a
/// hop comes down slower. A dive this fast covers a fifth of a pitch in a
/// step.
const DIVE: f32 = 12.0;

/// A bird diving onto a perch faster than it ever comes down is stopped on
/// top of it: what its feet are about to meet is seen ahead of them, so they
/// do not sink in by most of a step's travel first. It then stands there.
#[test]
fn a_hard_touchdown_stops_on_top_of_the_perch() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = world(layout);
    let feet = Vec2::ZERO;
    // Just above the perch, so that the first step would take the feet
    // nearly a step's travel into it.
    let start = feet + Vec2::new(0.0, layout.pitch * 0.02);
    let mut perches = Perches::default();
    perches.place(&mut world, &one_perch(feet), start, layout);
    CROW.insert(&mut world, start, Vec2::ZERO, layout);
    let dive = Vec2::new(0.0, -layout.pitch * DIVE);
    for tick in 0..20 {
        let (position, _) = step(&mut world, dive);
        assert!(
            feet.y - position.y < layout.pitch * 0.02,
            "tick {tick}: the feet {} pitches into the perch",
            (feet.y - position.y) / layout.pitch
        );
    }
    let mut velocity = Vec2::ZERO;
    for _ in 0..20 {
        velocity.y -= crate::crow::flight::gravity(layout) * DT;
        velocity = step(&mut world, velocity).1;
    }
    let (position, _) = CROW.motion(&world).unwrap();
    assert!(
        (position.y - feet.y).abs() < layout.pitch * 0.02,
        "{position:?}"
    );
    assert!(CROW.footing(&world, layout));
}

/// A fixed box, `half` across each way, its center at `center`.
fn solid(world: &mut PhysicsWorld, entity: u64, center: Vec2, half: Vec2) {
    let entity = PhysicsId::new(entity);
    assert!(world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: center,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            half.x,
            half.y,
        )],
    ));
}

/// What holds a bird up is a surface under its feet that it is not leaving:
/// standing on a perch, but not flying above it or taking off from it, and
/// not a wall it presses against or a ceiling it flies into, which push it
/// as hard.
#[test]
fn footing_is_a_surface_under_the_feet_that_the_bird_is_not_leaving() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = world(layout);
    let feet = Vec2::ZERO;
    let mut perches = Perches::default();
    perches.place(&mut world, &one_perch(feet), feet, layout);
    CROW.insert(&mut world, feet, Vec2::ZERO, layout);
    let pull = Vec2::new(0.0, -crate::crow::flight::gravity(layout) * DT);
    for _ in 0..3 {
        step(&mut world, pull);
    }
    assert!(CROW.footing(&world, layout), "standing");
    step(&mut world, Vec2::new(0.0, layout.pitch * 2.0));
    assert!(!CROW.footing(&world, layout), "taking off");
    for _ in 0..30 {
        step(&mut world, Vec2::new(0.0, layout.pitch * 2.0));
    }
    assert!(!CROW.footing(&world, layout), "flying");

    // Pressed against a wall beside it, or flying into a ceiling over it,
    // the bird is pushed as hard as a perch holds it up, but not up.
    let radius = layout.pitch * RADIUS_PITCH;
    let half = Vec2::new(layout.pitch, layout.pitch);
    let gap = layout.pitch * 0.01;
    let start = Vec2::new(0.0, layout.pitch * 5.0);
    let body = start + Vec2::new(0.0, radius);
    for (name, center, velocity) in [
        (
            "pressing on a wall",
            body + Vec2::new(radius + half.x + gap, 0.0),
            Vec2::new(layout.pitch * 2.0, 0.0),
        ),
        (
            "flying into a ceiling",
            body + Vec2::new(0.0, radius + half.y + gap),
            Vec2::new(0.0, layout.pitch * 2.0),
        ),
    ] {
        let mut world = PhysicsWorld::new(mechanics_config(layout));
        solid(&mut world, 1, center, half);
        CROW.insert(&mut world, start, Vec2::ZERO, layout);
        for tick in 0..3 {
            step(&mut world, velocity);
            let push = world
                .body_contact_push(CROW.id)
                .unwrap_or_else(|| panic!("{name}, tick {tick}: nothing pushes the bird"));
            assert!(
                push.velocity_change.length() > layout.pitch,
                "{name}, tick {tick}: {push:?}"
            );
            assert!(!CROW.footing(&world, layout), "{name}, tick {tick}");
        }
    }
}

/// Each column's perch follows the face: a perch that moves is placed
/// again where it now is, one that goes is taken away, and one that stays
/// is left alone.
#[test]
fn perches_follow_the_face() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = world(layout);
    // A crow above them all.
    let above = Vec2::new(0.0, layout.pitch * 10.0);
    let mut perches = Perches::default();
    let mut wanted = [None; PERCH_COLUMNS];
    for (column, perch) in wanted.iter_mut().enumerate().take(3) {
        *perch = Some(Perch {
            key: [0, column as u8, 8],
            feet: Vec2::new(column as f32 * layout.pitch, 0.0),
        });
    }
    perches.place(&mut world, &wanted, above, layout);
    assert_eq!((perches.count(), world.body_count()), (3, 3));
    let top = |world: &PhysicsWorld, column: usize| {
        world
            .body_solid_bounds(BodyId::new(perch_entity(column), BodyRole::PRIMARY))
            .map(|(min, max)| Vec2::new((min.x + max.x) * 0.5, max.y))
    };
    wanted[1] = Some(Perch {
        key: [0, 1, 6],
        feet: Vec2::new(layout.pitch, -layout.pitch * 2.0),
    });
    wanted[2] = None;
    perches.place(&mut world, &wanted, above, layout);
    assert_eq!((perches.count(), world.body_count()), (2, 2));
    for (column, perch) in wanted.iter().enumerate().take(2) {
        let feet = perch.unwrap().feet;
        let top = top(&world, column).unwrap();
        assert!((top - feet).length() < 1e-3, "column {column}: {top:?}");
    }
    assert_eq!(top(&world, 2), None);
    perches.place(&mut world, &[None; PERCH_COLUMNS], above, layout);
    assert_eq!((perches.count(), world.body_count()), (0, 0));
}

/// A perch holds the crow from above only. A crow under it, knocked off its
/// spot or under a cell the face has just lit, flies up through it as
/// through the column's other cells, and comes down onto it from above.
#[test]
fn a_perch_holds_the_crow_from_above_only() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = world(layout);
    let top = Vec2::ZERO;
    let mut perches = Perches::default();
    let mut feet = top - Vec2::new(0.0, layout.pitch * 2.0);
    CROW.insert(&mut world, feet, Vec2::ZERO, layout);
    let up = Vec2::new(0.0, layout.pitch * 3.0);
    for _ in 0..60 {
        perches.place(&mut world, &one_perch(top), feet, layout);
        feet = step(&mut world, up).0;
    }
    assert!(
        feet.y > top.y + layout.pitch * 0.5,
        "held under the perch, {} pitches from its top",
        (feet.y - top.y) / layout.pitch
    );
    for _ in 0..60 {
        perches.place(&mut world, &one_perch(top), feet, layout);
        feet = step(&mut world, up * -1.0).0;
    }
    assert!(
        (feet.y - top.y).abs() < layout.pitch * 0.02,
        "{} pitches from the perch's top",
        (feet.y - top.y) / layout.pitch
    );
    assert!(CROW.footing(&world, layout));
}
