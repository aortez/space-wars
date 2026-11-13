//! What the lamps perceive: a body no lamp code names, where it is and how
//! it moves; which movers stand out, and a bounded look at a crowd; a knock,
//! on the body struck as on the one that strikes, where it lands and how
//! hard, and none without a blow; what changes in the scene; that looking
//! changes nothing; and what a look costs.

use super::*;
use crate::arena_fixture::FixtureBall;
use crate::layout::Layout;
use crate::{ClockAction, ClockConfig, ClockScenario, ClockState};
use engine_common::{
    ClockEventProfile, ClockFontSettings, ClockSettings, ClockTimeFormat, Scenario,
};
use engine_rapier::world::{
    BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorldConfig,
};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_nanos(16_666_667);
const DT: f32 = 1.0 / FIXED_HZ as f32;

fn clock(profile: ClockEventProfile, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: profile,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 39, 50).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn percepts(state: &ClockState) -> &Percepts {
    state.lamps.perception.percepts()
}

#[test]
fn a_ball_no_lamp_code_names_is_perceived_where_it_is_and_as_it_moves() {
    let mut state = clock(ClockEventProfile::Off, 7);
    let layout = Layout::new(state.aspect_ratio());
    // A ball thrown sideways above the floor.
    let index = state
        .admit_arena_fixture(&[FixtureBall {
            position: Vec2::new(
                -layout.bounds_max.x * 0.5,
                layout.floor_y + layout.pitch * 6.0,
            ),
            velocity: Vec2::new(layout.pitch * 8.0, 0.0),
        }])
        .unwrap();
    for _ in 0..20 {
        ClockScenario::step(&mut state, &[], TICK);
        let fixture = state.arena_fixture(index).unwrap();
        let ball = fixture.balls[0];
        let movers = &percepts(&state).movers;
        assert_eq!(movers.len(), 1, "{movers:?}");
        let mover = movers[0];
        assert!(
            (mover.position - ball.position).length() < 1e-3,
            "{mover:?} {ball:?}"
        );
        assert!(
            (mover.velocity - ball.velocity).length() < 1e-3,
            "{mover:?} {ball:?}"
        );
        assert!((mover.size - fixture.radius).abs() < 1e-3, "{mover:?}");
    }
    // The diagnostics show the same, in thousandths, in the arena's world.
    let mover = percepts(&state).movers[0];
    let shown = state.lamps_state().perceived.movers[0];
    assert_eq!(shown.world, 0);
    assert_eq!(
        shown.position_milli[0],
        (mover.position.x * 1000.0).round() as i32
    );
    assert_eq!(
        shown.velocity_milli[1],
        (mover.velocity.y * 1000.0).round() as i32
    );
    assert_eq!(shown.age_ticks, 19);
}

/// Adds a dynamic ball to `world` and returns its body.
fn ball(
    world: &mut PhysicsWorld,
    entity: u64,
    position: Vec2,
    velocity: Vec2,
    radius: f32,
    density: f32,
) -> BodyId {
    let id = PhysicsId::new(entity);
    let body = BodyId::new(id, BodyRole::PRIMARY);
    assert!(world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Dynamic,
            position,
            linear_velocity: velocity,
            ..BodySpec::default()
        },
        &[ColliderSpec {
            density,
            ..ColliderSpec::ball(ColliderId::new(id, ColliderRole::PRIMARY, 0), radius)
        }],
    ));
    body
}

/// Adds a fixed floor, wide and a cell deep, whose top is at `top`.
fn floor(world: &mut PhysicsWorld, entity: u64, top: f32) {
    let id = PhysicsId::new(entity);
    assert!(world.insert_body(
        BodyId::new(id, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(5.0, top - 0.5),
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(id, ColliderRole::PRIMARY, 0),
            20.0,
            0.5,
        )],
    ));
}

/// A world of balls in a row along x, one cell apart, each moving right at
/// the speed given for it in cells a second. It lists them last first, out
/// of the order of their ids, as a scene's worlds may.
fn balls(first: u64, speeds: &[f32]) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    for (index, speed) in speeds.iter().enumerate().rev() {
        ball(
            &mut world,
            first + index as u64,
            Vec2::new(index as f32, 0.0),
            Vec2::new(*speed, 0.0),
            0.25,
            1.0,
        );
    }
    world
}

fn entities(percepts: &Percepts) -> Vec<(usize, u64)> {
    percepts
        .movers
        .iter()
        .map(|mover| (mover.world, mover.body.entity.value()))
        .collect()
}

fn knocked(percepts: &Percepts) -> Vec<u64> {
    percepts
        .impacts
        .iter()
        .map(|impact| impact.body.entity.value())
        .collect()
}

/// A view along the row of `balls`, a cell to a unit.
const ROW: View = View {
    min: Vec2::new(-1.0, -1.0),
    max: Vec2::new(1000.0, 1.0),
    pitch: 1.0,
};

const NOTHING: Facts = Facts {
    event: None,
    events_started: 0,
    font: ClockFont::Classic,
    digits: [None; DIGIT_SLOT_COUNT],
    ceiling_open: false,
    reading: None,
};

#[test]
fn the_most_salient_movers_come_first_and_a_crowd_costs_the_bound() {
    // More balls than a look visits, slower down the row. The world lists
    // them last first, so the first few, the fastest, lie past the bound.
    let count = 600;
    let speeds: Vec<f32> = (0..count)
        .map(|index| 2.0 + (count - 1 - index) as f32 * 0.01)
        .collect();
    let crowd = balls(1, &speeds);
    let mut perception = Perception::default();
    perception.perceive(&[Some(&crowd), None], ROW, NOTHING);
    assert_eq!(perception.percepts().visited, MAX_BODIES);
    let unseen = count - MAX_BODIES;
    let fastest: Vec<_> = (1..=MAX_MOVERS).map(|n| (0, (unseen + n) as u64)).collect();
    assert_eq!(entities(perception.percepts()), fastest);
    for _ in 0..4 * NOVELTY_TICKS as usize {
        perception.perceive(&[Some(&crowd), None], ROW, NOTHING);
    }
    // The crowd takes the whole look, so a second world goes unseen.
    let watched = unseen as u64 + 2;
    let newcomer = balls(watched, &[0.55 * speeds[unseen]]);
    perception.perceive(&[Some(&crowd), Some(&newcomer)], ROW, NOTHING);
    assert_eq!(entities(perception.percepts()), fastest);
    // With room, the newcomer is seen, and at little more than half their
    // speed it outranks movers the lamps have long watched, one of them a
    // body with its id in another world.
    let row = balls(watched, &speeds[unseen + 1..]);
    perception.perceive(&[Some(&row), Some(&newcomer)], ROW, NOTHING);
    let seen = perception.percepts();
    assert_eq!(seen.visited, MAX_BODIES);
    assert_eq!(entities(seen)[..2], [(1, watched), (0, watched)]);
    assert_eq!(seen.movers[0].age, 0);
    assert!(
        seen.movers[1].age > 4 * NOVELTY_TICKS as u64,
        "{:?}",
        seen.movers[1]
    );
    // Below a cell a second, or out of view beside it or above it, nothing
    // is moving.
    let slow = balls(1, &[0.9, 0.0]);
    perception.perceive(&[Some(&slow), None], ROW, NOTHING);
    assert!(perception.percepts().movers.is_empty());
    let short = View {
        max: Vec2::new(0.5, 1.0),
        ..ROW
    };
    let mut three = balls(1, &[5.0, 50.0]);
    ball(
        &mut three,
        3,
        Vec2::new(0.0, 5.0),
        Vec2::new(60.0, 0.0),
        0.25,
        1.0,
    );
    perception.perceive(&[Some(&three), None], short, NOTHING);
    assert_eq!(entities(perception.percepts()), [(0, 1)]);
}

#[test]
fn a_mover_stands_out_by_its_speed_and_size_and_more_while_new() {
    // A small ball and a big one at half its speed.
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    ball(&mut world, 1, Vec2::ZERO, Vec2::new(4.0, 0.0), 0.25, 1.0);
    ball(
        &mut world,
        2,
        Vec2::new(5.0, 0.0),
        Vec2::new(2.0, 0.0),
        1.0,
        1.0,
    );
    let mut perception = Perception::default();
    let mut look = || {
        perception.perceive(&[Some(&world)], ROW, NOTHING);
        perception
            .percepts()
            .movers
            .iter()
            .map(|mover| (mover.body.entity.value(), mover.salience))
            .collect::<Vec<_>>()
    };
    // Speed times size, in cells: the big one stands out, and both twice as
    // much as they start moving.
    let seen = look();
    assert_eq!(seen.len(), 2);
    assert_eq!((seen[0].0, seen[1].0), (2, 1));
    assert!((seen[0].1 - 2.0 * 1.0 * 2.0).abs() < 1e-4, "{seen:?}");
    assert!((seen[1].1 - 4.0 * 0.25 * 2.0).abs() < 1e-4, "{seen:?}");
    // Two seconds on, 37% more.
    for _ in 0..NOVELTY_TICKS as usize - 1 {
        look();
    }
    let seen = look();
    let novelty = 1.0 + (-1.0_f32).exp();
    assert!((seen[0].1 - 2.0 * novelty).abs() < 1e-4, "{seen:?}");
}

#[test]
fn a_mover_that_pauses_keeps_its_age_and_one_that_stops_is_new_again() {
    let (fast, slow, still) = (balls(1, &[4.0]), balls(1, &[0.5]), balls(1, &[0.0]));
    let mut perception = Perception::default();
    let mut look = |world: &PhysicsWorld, looks: u64| {
        for _ in 0..looks {
            perception.perceive(&[Some(world)], ROW, NOTHING);
        }
        perception.percepts().movers.first().map(|mover| mover.age)
    };
    // Its age counts the looks since the first that saw it moving.
    assert_eq!(look(&fast, 10), Some(9));
    // Still or slow for less than the forgetting time, it moves on as it
    // was, its age counting the pause.
    assert_eq!(look(&still, FORGET_TICKS - 1), None);
    assert_eq!(look(&fast, 1), Some(10 + FORGET_TICKS - 1));
    assert_eq!(look(&slow, FORGET_TICKS - 1), None);
    assert_eq!(look(&fast, 1), Some(10 + 2 * FORGET_TICKS - 1));
    // Slow for the whole forgetting time, it stopped.
    assert_eq!(look(&slow, FORGET_TICKS), None);
    assert_eq!(look(&fast, 1), Some(0));
}

#[test]
fn a_ball_landing_is_an_impact_where_it_meets_the_floor() {
    let mut state = clock(ClockEventProfile::Off, 7);
    let layout = Layout::new(state.aspect_ratio());
    let x = layout.bounds_max.x * 0.3;
    let index = state
        .admit_arena_fixture(&[FixtureBall {
            position: Vec2::new(x, layout.floor_y + layout.pitch * 8.0),
            velocity: Vec2::ZERO,
        }])
        .unwrap();
    let mut impacts = Vec::new();
    for tick in 0..300 {
        let falling = state.arena_fixture(index).unwrap().balls[0].velocity.y;
        ClockScenario::step(&mut state, &[], TICK);
        let after = state.arena_fixture(index).unwrap().balls[0].velocity.y;
        impacts.extend(
            percepts(&state)
                .impacts
                .iter()
                .map(|impact| (tick, falling, after, *impact)),
        );
    }
    let fixture = state.arena_fixture(index).unwrap();
    let floor = fixture.balls[0].position.y - fixture.radius;
    let (_, falling, after, landing) = impacts[0];
    // It landed falling fast, and its strength is the floor's push: the
    // whole change in its speed, and the step of gravity the floor took
    // away too.
    assert!(falling < -KNOCK * layout.pitch, "{falling}");
    let world = state.arena.world().unwrap();
    let gravity = world.gravity().y * world.gravity_scale(landing.body).unwrap();
    let push = (after - falling - gravity * DT) / layout.pitch;
    assert!(push >= KNOCK, "{push}");
    assert!((landing.strength - push).abs() < 1e-3, "{landing:?} {push}");
    // The lamps place the knock where the ball meets the floor.
    assert!(
        (landing.position.x - x).abs() < fixture.radius * 0.5,
        "{landing:?}"
    );
    assert!(
        (landing.position.y - floor).abs() < fixture.radius * 0.5,
        "{landing:?} {floor}"
    );
    // Resting on the floor, it takes no more knocks.
    assert!(
        impacts.iter().all(|(tick, _, _, _)| *tick < 240),
        "{impacts:?}"
    );
}

#[test]
fn a_body_struck_at_rest_takes_the_knock_and_a_heavy_striker_hardly_one() {
    // A heavy ball rolls into a light one at rest, a tenth of a cell away.
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let heavy = ball(&mut world, 1, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.25, 100.0);
    let light = ball(&mut world, 2, Vec2::new(0.6, 0.0), Vec2::ZERO, 0.25, 1.0);
    let mut perception = Perception::default();
    perception.perceive(&[Some(&world)], ROW, NOTHING);
    let mut looks = 0;
    while perception.percepts().impacts.is_empty() {
        world.step(DT);
        perception.perceive(&[Some(&world)], ROW, NOTHING);
        looks += 1;
        assert!(looks < 5, "no blow");
    }
    // The light ball takes the whole of its new speed as a knock, where the
    // two meet. The heavy one hardly slows: no knock.
    let motion = |body| world.motion(body).unwrap().linear_velocity;
    let impacts = &perception.percepts().impacts;
    assert_eq!(knocked(perception.percepts()), [2], "{impacts:?}");
    let speed = motion(light).length();
    assert!(speed >= KNOCK, "{speed}");
    assert!((impacts[0].strength - speed).abs() < 1e-3, "{impacts:?}");
    assert!(10.0 - motion(heavy).x < 1.0, "{:?}", motion(heavy));
    assert!(
        (impacts[0].position - Vec2::new(0.35, 0.0)).length() < 0.125,
        "{impacts:?}"
    );
    // One blow is one knock, though the two go on touching.
    for _ in 0..10 {
        world.step(DT);
        perception.perceive(&[Some(&world)], ROW, NOTHING);
        assert!(perception.percepts().impacts.is_empty());
    }
}

#[test]
fn a_kinematic_body_knocks_what_it_strikes_and_takes_no_knock_itself() {
    // A dense ball at rest, touching a paddle that moves as it is told.
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let ball = ball(&mut world, 1, Vec2::ZERO, Vec2::ZERO, 0.25, 10.0);
    let paddle = BodyId::new(PhysicsId::new(9), BodyRole::PRIMARY);
    assert!(world.insert_body(
        paddle,
        BodySpec {
            kind: BodyKind::KinematicVelocity,
            position: Vec2::new(-0.75, 0.0),
            linear_velocity: Vec2::new(20.0, 0.0),
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(PhysicsId::new(9), ColliderRole::PRIMARY, 0),
            0.5,
            0.5,
        )],
    ));
    let mut perception = Perception::default();
    perception.perceive(&[Some(&world)], ROW, NOTHING);
    world.step(DT);
    perception.perceive(&[Some(&world)], ROW, NOTHING);
    // The paddle sends the ball off at its own speed: the ball is knocked,
    // and the paddle, which pushed it as hard, is not.
    let speed = world.motion(ball).unwrap().linear_velocity.x;
    assert!(speed > KNOCK, "{speed}");
    let impacts = &perception.percepts().impacts;
    assert_eq!(knocked(perception.percepts()), [1], "{impacts:?}");
    assert!((impacts[0].strength - speed).abs() < 1e-3, "{impacts:?}");
    // Stopped dead against the ball, it still takes none.
    assert!(world.set_velocity(paddle, Vec2::ZERO, 0.0, true));
    world.step(DT);
    perception.perceive(&[Some(&world)], ROW, NOTHING);
    assert!(!knocked(perception.percepts()).contains(&9));
}

#[test]
fn the_strongest_knocks_come_first_and_no_more_than_four() {
    // Six balls side by side, falling at 10 to 15 cells a second, a
    // hundredth of a cell above a floor.
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    floor(&mut world, 99, -0.26);
    for entity in 1..=6 {
        ball(
            &mut world,
            entity,
            Vec2::new(2.0 * entity as f32, 0.0),
            Vec2::new(0.0, -9.0 - entity as f32),
            0.25,
            1.0,
        );
    }
    let view = View {
        min: Vec2::new(-1.0, -2.0),
        max: Vec2::new(20.0, 2.0),
        pitch: 1.0,
    };
    // Lamps looking only above the floor see the balls but not where they
    // land.
    let above = View {
        min: Vec2::new(-1.0, 0.0),
        ..view
    };
    let (mut perception, mut high) = (Perception::default(), Perception::default());
    let mut look = |world: &PhysicsWorld| {
        perception.perceive(&[Some(world)], view, NOTHING);
        high.perceive(&[Some(world)], above, NOTHING);
        (knocked(perception.percepts()), knocked(high.percepts()))
    };
    look(&world);
    let mut looks = 0;
    let (knocks, seen_above) = loop {
        world.step(DT);
        let (knocks, seen_above) = look(&world);
        if !knocks.is_empty() {
            break (knocks, seen_above);
        }
        looks += 1;
        assert!(looks < 5, "no landing");
    };
    // All six land in one step; the lamps keep the four fastest, fastest
    // first.
    assert_eq!(knocks, [6, 5, 4, 3]);
    assert!(seen_above.is_empty(), "{seen_above:?}");
}

#[test]
fn the_contact_that_pushed_hardest_places_the_knock() {
    // A light ball resting on the floor, and a heavy one rolling into it
    // from the side.
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -10.0),
        ..PhysicsWorldConfig::default()
    });
    floor(&mut world, 99, 0.0);
    ball(
        &mut world,
        1,
        Vec2::new(0.0, 0.25),
        Vec2::new(10.0, 0.0),
        0.25,
        100.0,
    );
    let light = ball(&mut world, 2, Vec2::new(0.6, 0.25), Vec2::ZERO, 0.25, 1.0);
    let view = View {
        min: Vec2::new(-1.0, -1.0),
        max: Vec2::new(20.0, 10.0),
        pitch: 1.0,
    };
    let mut perception = Perception::default();
    perception.perceive(&[Some(&world)], view, NOTHING);
    let mut looks = 0;
    while perception.percepts().impacts.is_empty() {
        world.step(DT);
        assert!(world.body_contact_push(light).is_some(), "on the floor");
        perception.perceive(&[Some(&world)], view, NOTHING);
        looks += 1;
        assert!(looks < 5, "no blow");
    }
    // The floor holds up the light ball's weight as the heavy one strikes it
    // far harder, so the knock is where the two meet.
    let impacts = &perception.percepts().impacts;
    assert_eq!(knocked(perception.percepts()), [2], "{impacts:?}");
    assert!(
        (impacts[0].position - Vec2::new(0.35, 0.25)).length() < 0.1,
        "{impacts:?}"
    );
}

#[test]
fn a_body_stopped_without_a_blow_takes_no_knock() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -10.0),
        ..PhysicsWorldConfig::default()
    });
    floor(&mut world, 99, 0.0);
    // One ball flying, one rolling along the floor, both fast.
    let flying = ball(
        &mut world,
        1,
        Vec2::new(0.0, 5.0),
        Vec2::new(20.0, 0.0),
        0.25,
        1.0,
    );
    let rolling = ball(
        &mut world,
        2,
        Vec2::new(0.0, 0.25),
        Vec2::new(7.2, 0.0),
        0.25,
        1.0,
    );
    let view = View {
        min: Vec2::new(-1.0, -1.0),
        max: Vec2::new(20.0, 10.0),
        pitch: 1.0,
    };
    let mut perception = Perception::default();
    for _ in 0..5 {
        world.step(DT);
        perception.perceive(&[Some(&world)], view, NOTHING);
        assert!(perception.percepts().impacts.is_empty());
    }
    assert!(world.body_contact_push(rolling).is_some(), "on the floor");
    // Stopped dead by a script, or anything else that does not strike them,
    // though one rests on the floor.
    for body in [flying, rolling] {
        assert!(world.set_velocity(body, Vec2::ZERO, 0.0, true));
    }
    world.step(DT);
    perception.perceive(&[Some(&world)], view, NOTHING);
    assert!(perception.percepts().impacts.is_empty());
    assert!(world.body_contact_push(rolling).is_some(), "still on it");
}

#[test]
fn a_body_held_against_a_pull_takes_no_knock_and_a_thrown_one_only_its_blow() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    floor(&mut world, 99, 0.0);
    let view = View {
        min: Vec2::new(-1.0, -1.0),
        max: Vec2::new(20.0, 10.0),
        pitch: 1.0,
    };
    // A ball pulled onto the floor ten cells a second harder every step: the
    // floor pushes back as hard every step, and the ball does not move.
    let held = ball(&mut world, 1, Vec2::new(0.0, 0.25), Vec2::ZERO, 0.25, 1.0);
    let mut perception = Perception::default();
    perception.perceive(&[Some(&world)], view, NOTHING);
    for _ in 0..30 {
        world.clear_forces();
        assert!(world.apply_acceleration(held, Vec2::new(0.0, -10.0 / DT), true));
        world.step(DT);
        perception.perceive(&[Some(&world)], view, NOTHING);
        assert!(perception.percepts().impacts.is_empty());
    }
    let push = world.body_contact_push(held).unwrap();
    assert!(push.velocity_change.y > KNOCK, "{push:?}");
    world.clear_forces();
    // A ball landing, and thrown up again by a script before the lamps
    // look, takes the floor's blow and no more.
    let thrown = ball(
        &mut world,
        2,
        Vec2::new(5.0, 0.4),
        Vec2::new(0.0, -10.0),
        0.25,
        1.0,
    );
    perception.perceive(&[Some(&world)], view, NOTHING);
    let mut landing = None;
    for _ in 0..5 {
        world.step(DT);
        landing = world.body_contact_push(thrown);
        if landing.is_some() {
            break;
        }
        perception.perceive(&[Some(&world)], view, NOTHING);
    }
    let landing = landing.expect("the ball lands");
    assert!(world.set_velocity(thrown, Vec2::new(0.0, 20.0), 0.0, true));
    perception.perceive(&[Some(&world)], view, NOTHING);
    let impacts = &perception.percepts().impacts;
    assert_eq!(knocked(perception.percepts()), [2], "{impacts:?}");
    let blow = landing.velocity_change.length();
    assert!(blow >= KNOCK, "{landing:?}");
    assert!((impacts[0].strength - blow).abs() < 1e-3, "{impacts:?}");
}

#[test]
fn the_lamps_perceive_what_changes_in_the_scene() {
    let mut state = clock(ClockEventProfile::Off, 7);
    ClockScenario::step(&mut state, &[], TICK);
    let signals = percepts(&state).signals;
    assert_eq!(signals.changed_digits, [false; DIGIT_SLOT_COUNT]);
    let day = signals.time_of_day.unwrap();
    assert!(
        (day - (12.0 * 3600.0 + 39.0 * 60.0 + 50.0) / 86_400.0).abs() < 1e-6,
        "{day}"
    );
    // A new minute changes the last two digits, on that tick alone.
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 40, 0).unwrap(),
        )],
        TICK,
    );
    assert_eq!(
        percepts(&state).signals.changed_digits,
        [false, false, true, true]
    );
    ClockScenario::step(&mut state, &[], TICK);
    assert_eq!(
        percepts(&state).signals.changed_digits,
        [false; DIGIT_SLOT_COUNT]
    );
    // A new font changes every digit the face shows: here all four, and in
    // twelve-hour time at one o'clock, all but the blank first.
    let font = |state: &ClockState, selected| ClockSettings {
        fonts: ClockFontSettings {
            selected,
            ..state.settings().fonts
        },
        ..state.settings()
    };
    let matrix = font(&state, ClockFont::Matrix);
    ClockScenario::step(&mut state, &[ClockAction::configure(matrix)], TICK);
    assert_eq!(state.display.font, ClockFont::Matrix);
    assert_eq!(
        percepts(&state).signals.changed_digits,
        [true; DIGIT_SLOT_COUNT]
    );
    let twelve = ClockSettings {
        time_format: ClockTimeFormat::TwelveHour,
        ..state.settings()
    };
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::configure(twelve),
            ClockAction::set_reading(ClockReading::new(13, 40, 0).unwrap()),
        ],
        TICK,
    );
    assert_eq!(state.display.digits, [None, Some(1), Some(4), Some(0)]);
    let classic = font(&state, ClockFont::Classic);
    ClockScenario::step(&mut state, &[ClockAction::configure(classic)], TICK);
    assert_eq!(state.display.font, ClockFont::Classic);
    assert_eq!(
        percepts(&state).signals.changed_digits,
        [false, true, true, true]
    );
    // An event's beginning and its end, each on its tick, and one started
    // over another of its kind ends it and begins.
    let color_cycle = || ClockAction::preview_event(ClockEventKind::ColorCycle);
    ClockScenario::step(&mut state, &[color_cycle()], TICK);
    let signals = percepts(&state).signals;
    assert_eq!(signals.event, Some(ClockEventKind::ColorCycle));
    assert!(signals.event_began);
    assert_eq!(signals.event_ended, None);
    for _ in 0..10 {
        ClockScenario::step(&mut state, &[], TICK);
        let signals = percepts(&state).signals;
        assert!(!signals.event_began && signals.event_ended.is_none());
    }
    ClockScenario::step(&mut state, &[color_cycle()], TICK);
    let signals = percepts(&state).signals;
    assert!(signals.event_began);
    assert_eq!(signals.event_ended, Some(ClockEventKind::ColorCycle));
    let mut ended = None;
    for tick in 0..60 * 60 {
        ClockScenario::step(&mut state, &[], TICK);
        let signals = percepts(&state).signals;
        assert!(!signals.event_began, "tick {tick}");
        if let Some(kind) = signals.event_ended {
            assert_eq!(signals.event, None);
            ended = Some(kind);
            break;
        }
    }
    assert_eq!(ended, Some(ClockEventKind::ColorCycle));
    // The last minutes of the day stay below its end.
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(23, 59, 59).unwrap(),
        )],
        TICK,
    );
    assert_eq!(state.lamps_state().perceived.time_of_day_milli, Some(999));
}

#[test]
fn perception_is_bounded_and_the_scene_runs_the_same_without_it() {
    let mut seeing = clock(ClockEventProfile::Demo, 42);
    let mut blind = clock(ClockEventProfile::Demo, 42);
    blind.lamps.perception.blind = true;
    let (mut moving, mut knocks) = (0, 0);
    for tick in 0..6 * 60 * 60 {
        ClockScenario::step(&mut seeing, &[], TICK);
        ClockScenario::step(&mut blind, &[], TICK);
        let percepts = percepts(&seeing);
        assert!(percepts.movers.len() <= MAX_MOVERS, "tick {tick}");
        assert!(percepts.impacts.len() <= MAX_IMPACTS, "tick {tick}");
        assert!(percepts.visited <= MAX_BODIES, "tick {tick}");
        moving += usize::from(!percepts.movers.is_empty());
        knocks += percepts.impacts.len();
        // The same events, visits and residents.
        assert_eq!(seeing.event_id(), blind.event_id(), "tick {tick}");
        let visits = |state: &ClockState| {
            [
                state.duck_visit.is_some(),
                state.crow_visit.is_some(),
                state.spider_visit.is_some(),
            ]
        };
        assert_eq!(visits(&seeing), visits(&blind), "tick {tick}");
        let (seen, unseen) = (seeing.arena_state(), blind.arena_state());
        assert_eq!(seen.accounting, unseen.accounting, "tick {tick}");
        assert_eq!(seen.claims, unseen.claims, "tick {tick}");
        if tick % 60 == 0 {
            assert_eq!(
                ClockScenario::render_frame(&seeing),
                ClockScenario::render_frame(&blind),
                "tick {tick}"
            );
        }
    }
    // The run gave the lamps something to see.
    assert!(
        moving > 600 && knocks > 10,
        "{moving} ticks with movers, {knocks} knocks"
    );
}

/// What one look cost, and what it saw.
struct Cost {
    spent: Duration,
    visited: usize,
    moving: bool,
    impacts: usize,
}

/// Steps the scene for at most `ticks` while `during` holds, and after each
/// step looks again at what the scene looked at, with a perception of its
/// own, timed.
fn shadow(state: &mut ClockState, ticks: usize, during: impl Fn(&ClockState) -> bool) -> Vec<Cost> {
    let mut perception = Perception::default();
    let mut costs = Vec::new();
    for _ in 0..ticks {
        ClockScenario::step(state, &[], TICK);
        if !during(state) {
            break;
        }
        let (view, facts) = state.lamp_sight(&Layout::new(state.aspect_ratio()));
        let worlds = ClockState::scene_worlds(&state.arena, state.active_event.as_ref());
        let started = Instant::now();
        perception.perceive(&worlds, view, facts);
        let spent = started.elapsed();
        let seen = perception.percepts();
        assert_eq!(seen.visited, percepts(state).visited);
        costs.push(Cost {
            spent,
            visited: seen.visited,
            moving: !seen.movers.is_empty(),
            impacts: seen.impacts.len(),
        });
    }
    costs
}

fn report(what: &str, costs: &[Cost]) {
    if costs.is_empty() {
        println!("{what}: no looks");
        return;
    }
    let mut spent: Vec<f64> = costs
        .iter()
        .map(|cost| cost.spent.as_secs_f64() * 1e6)
        .collect();
    spent.sort_by(f64::total_cmp);
    let at = |fraction: f64| spent[((spent.len() - 1) as f64 * fraction) as usize];
    let moving = costs.iter().filter(|cost| cost.moving).count();
    println!(
        "{what}: {} looks, up to {} bodies, movers on {:.1}% of looks, {} impacts; \
         mean {:.1} µs, median {:.1}, p99 {:.1}, worst {:.1}",
        costs.len(),
        costs.iter().map(|cost| cost.visited).max().unwrap_or(0),
        100.0 * moving as f64 / costs.len() as f64,
        costs.iter().map(|cost| cost.impacts).sum::<usize>(),
        spent.iter().sum::<f64>() / spent.len() as f64,
        at(0.5),
        at(0.99),
        at(1.0),
    );
}

/// The figures `docs/clock.md` gives for what a look costs: through each of
/// Demo's busiest events, ten minutes of Demo, and a crowd past the bound.
#[test]
#[ignore = "measures what perceiving costs; run with --ignored --nocapture"]
fn what_a_look_costs() {
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::Explosion,
        ClockEventKind::Coins,
        ClockEventKind::Tanks,
    ] {
        let mut state = clock(ClockEventProfile::Off, 42);
        ClockScenario::step(&mut state, &[ClockAction::preview_event(kind)], TICK);
        let costs = shadow(&mut state, 60 * 60, |state| {
            state.event_kind() == Some(kind)
        });
        report(kind.label(), &costs);
    }
    let mut demo = clock(ClockEventProfile::Demo, 42);
    report(
        "Ten minutes of Demo",
        &shadow(&mut demo, 10 * 60 * 60, |_| true),
    );
    let crowd = balls(1, &[2.0; 600]);
    let mut perception = Perception::default();
    let costs: Vec<Cost> = (0..1000)
        .map(|_| {
            let started = Instant::now();
            perception.perceive(&[Some(&crowd)], ROW, NOTHING);
            Cost {
                spent: started.elapsed(),
                visited: perception.percepts().visited,
                moving: !perception.percepts().movers.is_empty(),
                impacts: 0,
            }
        })
        .collect();
    report("A crowd of 600 balls", &costs);
}
