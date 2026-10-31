//! The standard for the run's physics arena: there is at most one, and every
//! tick that moves time steps it exactly once, whoever holds it. A visit, an
//! event sharing it, a lease left vacant and a resident such as soil all move
//! in that one step, and a tick that does not move time steps nothing.
//! Standalone events still step worlds of their own beside it, so the count
//! that proves this is the arena's, not the thread's. Soil takes its shape in
//! the arena just before that step, so a paused clock never changes the arena
//! for it.
//!
//! A resident keeps its bodies, their motion and their identities while
//! visits and events come and go, and rests on what actually supports it: a
//! visit's terrain while the visit holds the arena, an event's own floor, or
//! the ordinary floor, closed or open at the drain. Where the floor goes, it
//! falls, and it leaves the arena only by falling out of the view. The arena
//! fixture's balls are the resident these checks use. A standalone Falling or
//! Explosion still steps a world of its own, so its bodies meet a resident
//! only once a player joins and the arena adopts that world.
//!
//! Owners are bounded. A second owner that leaves takes only its own bodies,
//! and the fixture moves on exactly as it would have alone. A visit, an event
//! or a resident the arena has no room for is refused before anything
//! changes, so the residents and everyone else holding the arena stay as they
//! were. Debug builds audit every step: each body belongs to exactly one
//! owner holding the arena, within its budget, and the world's identities
//! name live objects one to one.
//!
//! A resize still finishes visits and events, but the residents stay in the
//! same world. A ball beyond a narrower view comes in just far enough to lie
//! within it, keeping its height and motion. Balls that could only come in
//! by landing in each other stay where they are instead, with the floor they
//! stood on, and the arena takes nothing that uses the floor until a resize
//! they fit or their leaving.

use crate::{
    ArenaAccounting, ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState,
    EVENT_CATALOG, MAX_RESIDENT_CLAIMS, Remap, Unavailable,
    arena::{Owner, ids::canonical::ARENA_FLOOR},
    arena_fixture::{ArenaFixture, FixtureBall, FixtureBallState},
    events::{COOLDOWN_TICKS, EventLifecycle, EventPhase, duck::arena::CourseGeometry},
    layout::Layout,
};
use engine_common::{Action, ClockEventKind, ClockEventProfile, ClockEvents, Scenario};
use engine_core::Vec2;
use engine_rapier::world::physics_activity;
use std::time::Duration;

const ASPECT: f32 = 800.0 / 480.0;
const SEED: u64 = 7;
const FRAME: Duration = Duration::from_nanos(16_666_667);
const DT: f32 = 1.0 / 60.0;
/// Clock's own downward acceleration, which residents keep.
const GRAVITY: f32 = 400.0;

fn plain() -> ClockState {
    plain_at(ASPECT)
}

fn plain_at(aspect_ratio: f32) -> ClockState {
    ready(ClockConfig {
        aspect_ratio,
        event_profile: ClockEventProfile::Off,
        ..ClockConfig::default()
    })
}

/// A run with `config`, showing the time.
fn ready(config: ClockConfig) -> ClockState {
    let mut state = ClockScenario::init(config, SEED);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(8, 8, 0).expect("a valid reading"),
        )],
        Duration::ZERO,
    );
    state
}

fn toggle(state: &mut ClockState, player: u8) {
    ClockScenario::step(
        state,
        &[ClockAction::toggle_player_duck(player)],
        Duration::ZERO,
    );
}

/// Advances `ticks` frames. Each must step the arena `arena` times and the
/// thread's worlds `worlds` times in all.
fn advance(state: &mut ClockState, ticks: usize, arena: u64, worlds: u64, what: &str) {
    for tick in 0..ticks {
        let before = (state.arena_accounting().steps, physics_activity().steps);
        ClockScenario::step(state, &[], FRAME);
        assert_eq!(
            state.arena_accounting().steps - before.0,
            arena,
            "{what}, tick {tick}: arena steps"
        );
        assert_eq!(
            physics_activity().steps - before.1,
            worlds,
            "{what}, tick {tick}: steps of every world"
        );
    }
}

#[test]
fn arena_contract_steps_once_per_tick_whoever_holds_it() {
    let mut state = plain();
    state.admit_soil();
    advance(&mut state, 30, 1, 1, "soil alone");
    assert!(
        state.arena.holds(Owner::Soil),
        "delivered soil keeps the arena"
    );

    // A standalone Falling steps its own world beside the arena until a
    // player joins, when the arena adopts that world.
    state.preview_event(ClockEventKind::Falling);
    advance(&mut state, 30, 1, 2, "soil beside a standalone Falling");
    toggle(&mut state, 1);
    assert!(state.arena.holds(Owner::Event(ClockEventKind::Falling)));
    advance(&mut state, 30, 1, 1, "a player joined to Falling");
    toggle(&mut state, 1);
    while state.duck_visit.is_some() {
        advance(&mut state, 1, 1, 1, "a dismissed player");
    }
    assert!(state.arena.vacant.is_some(), "the lease keeps the terrain");
    while state.event_kind().is_some() {
        advance(&mut state, 1, 1, 1, "a vacant lease");
    }
    assert_eq!(state.arena_accounting().claims, 1, "only the soil is left");
    advance(&mut state, 30, 1, 1, "soil alone again");

    // Events that share a visit's arena step in the visit's one step.
    for kind in [
        ClockEventKind::Aquarium,
        ClockEventKind::Explosion,
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
    ] {
        toggle(&mut state, 1);
        advance(&mut state, 90, 1, 1, "a player beside soil");
        state.preview_event(kind);
        assert_eq!(state.event_kind(), Some(kind));
        assert!(state.arena.holds(Owner::Event(kind)), "{kind:?} shares");
        advance(&mut state, 120, 1, 1, kind.as_str());
        state.finish_event();
        toggle(&mut state, 1);
        while state.duck_visit.is_some() {
            advance(&mut state, 1, 1, 1, "a departing player");
        }
    }

    // A tick that does not move time does not step anything.
    let before = (state.arena_accounting(), physics_activity().steps);
    for _ in 0..30 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.arena_accounting(), before.0);
    assert_eq!(physics_activity().steps, before.1);
}

#[test]
fn arena_contract_soil_delivered_while_paused_enters_with_the_next_moving_tick() {
    let mut state = plain();
    state.admit_soil();
    for _ in 0..30 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(
        state.arena_accounting(),
        ArenaAccounting::default(),
        "a paused clock changes nothing in the arena"
    );
    advance(&mut state, 1, 1, 1, "the first tick that moves time");
    let arena = state.arena_accounting();
    assert_eq!((arena.claims, arena.residents, arena.bodies), (1, 1, 1));
}

#[test]
fn arena_contract_no_world_steps_while_nothing_holds_the_arena() {
    let mut state = plain();
    advance(&mut state, 30, 0, 0, "nothing holds the arena");
    toggle(&mut state, 1);
    advance(&mut state, 120, 1, 1, "a visit's own arena");
    toggle(&mut state, 1);
    while state.duck_visit.is_some() {
        let held = state.arena.world().is_some();
        advance(
            &mut state,
            1,
            u64::from(held),
            u64::from(held),
            "a departing visit",
        );
    }
    assert_eq!(state.arena_accounting().claims, 0);
    assert!(
        state.arena.world().is_none(),
        "the last claim takes the world"
    );
    advance(&mut state, 30, 0, 0, "nothing holds the arena again");
}

/// Where a fixture ball rests on the closed floor.
fn resting(x: f32) -> Vec2 {
    let layout = Layout::new(ASPECT);
    Vec2::new(x, layout.floor_y + ArenaFixture::radius(layout))
}

fn at_rest(position: Vec2) -> FixtureBall {
    FixtureBall {
        position,
        velocity: Vec2::ZERO,
    }
}

/// The first fixture's balls.
fn balls(state: &ClockState) -> Vec<FixtureBallState> {
    state
        .arena_fixture(0)
        .map_or_else(Vec::new, |fixture| fixture.balls)
}

/// Every fixture's balls, each by its fixture and slot.
fn every_ball(state: &ClockState) -> Vec<((usize, usize), FixtureBallState)> {
    state
        .arena_fixtures()
        .into_iter()
        .flat_map(|fixture| {
            fixture
                .balls
                .into_iter()
                .map(move |ball| ((fixture.index, ball.slot), ball))
        })
        .collect()
}

fn slots(state: &ClockState) -> Vec<usize> {
    balls(state).iter().map(|ball| ball.slot).collect()
}

/// The bodies of the arena's own floor under its resting residents: one
/// closed slab, two banks or two panels, and none while a visit's terrain is
/// the floor.
fn own_floor(state: &ClockState) -> usize {
    state.arena.world().map_or(0, |world| {
        world
            .entity_ids()
            .filter(|entity| ARENA_FLOOR.contains(&entity.value()))
            .count()
    })
}

/// One frame with `actions`, in which every fixture's every ball keeps its
/// identity and moves on from where it was, never further than its speed
/// takes it, give or take half its radius of contact correction. A ball
/// leaves only by falling out of the view. Returns the fixtures and slots of
/// those that did.
fn keep_with(state: &mut ClockState, actions: &[Action], what: &str) -> Vec<(usize, usize)> {
    let layout = Layout::new(state.aspect_ratio());
    let (bottom, slack) = (layout.bounds_min.y, ArenaFixture::radius(layout) * 0.5);
    let before = every_ball(state);
    let steps = state.arena_accounting().steps;
    ClockScenario::step(state, actions, FRAME);
    if !before.is_empty() {
        assert_eq!(
            state.arena_accounting().steps,
            steps + 1,
            "{what}: one arena step"
        );
    }
    let after = every_ball(state);
    let mut fell = Vec::new();
    for (id, ball) in &before {
        match after.iter().find(|(now, _)| now == id) {
            Some((_, now)) => {
                let reach = ball.velocity.length().max(now.velocity.length()) * DT + slack;
                assert!(
                    now.position.distance_to(ball.position) <= reach,
                    "{what}: ball {id:?} went from {:?} to {:?} ({:?} then {:?})",
                    ball.position,
                    now.position,
                    ball.velocity,
                    now.velocity
                );
            }
            None => {
                assert!(
                    ball.position.y < bottom,
                    "{what}: ball {id:?} left the arena from {:?}",
                    ball.position
                );
                fell.push(*id);
            }
        }
    }
    assert!(
        after
            .iter()
            .all(|(now, _)| before.iter().any(|(id, _)| id == now)),
        "{what}: no ball appears"
    );
    fell
}

/// `ticks` frames of `keep_with` without actions.
fn keep(state: &mut ClockState, ticks: usize, what: &str) -> Vec<(usize, usize)> {
    (0..ticks)
        .flat_map(|tick| keep_with(state, &[], &format!("{what}, tick {tick}")))
        .collect()
}

#[test]
fn arena_contract_fixture_falls_rests_and_keeps_rolling_while_the_face_changes() {
    let mut state = plain();
    assert_eq!(
        state.admit_arena_fixture(&[
            at_rest(resting(250.0) + Vec2::new(0.0, 60.0)),
            FixtureBall {
                position: resting(-300.0),
                velocity: Vec2::new(15.0, 0.0),
            },
        ]),
        Ok(0)
    );
    let arena = state.arena_accounting();
    assert_eq!(
        (arena.claims, arena.residents, arena.bodies),
        (1, 1, 2),
        "a resident of its own, and no floor until time moves"
    );

    // Gravity: Clock's own, one tick at a time. The step integrates the
    // drop in substeps, so it lies between the two one-step bounds.
    keep(&mut state, 10, "free fall");
    let dropped = balls(&state)[0];
    assert!((dropped.velocity.y + GRAVITY * 10.0 * DT).abs() < 1e-3);
    let drop = resting(250.0).y + 60.0 - dropped.position.y;
    let unit = GRAVITY * DT * DT;
    assert!((45.0 * unit..=55.0 * unit).contains(&drop), "fell {drop}");
    assert_eq!(own_floor(&state), 1, "the closed floor");
    assert_eq!(state.arena_accounting().bodies, 3);

    // Contact: both rest on the closed floor, one of them rolling along it.
    keep(&mut state, 110, "landing");
    let [dropped, rolling] = balls(&state)[..] else {
        panic!("two balls");
    };
    assert!(dropped.position.distance_to(resting(250.0)) < 0.1);
    assert!(dropped.velocity.length() < 0.01);
    assert!((rolling.position.y - resting(0.0).y).abs() < 0.1);
    let speed = rolling.velocity.x;
    assert!(speed > 5.0, "rolling at {speed}");

    // It keeps rolling, through idle time, a face-only event and its finish,
    // another that a third cancels, and a minute change.
    keep(&mut state, 120, "idle time");
    state.preview_event(ClockEventKind::ColorCycle);
    while state.event_kind().is_some() {
        keep(&mut state, 1, "a color cycle");
    }
    state.preview_event(ClockEventKind::Marquee);
    keep(&mut state, 60, "a marquee");
    state.preview_event(ClockEventKind::DigitSlide);
    while state.event_kind().is_some() {
        keep(&mut state, 1, "a digit slide replacing it");
    }
    keep_with(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(8, 9, 0).expect("a valid reading"),
        )],
        "a minute change",
    );
    keep(&mut state, 120, "the new minute");
    let [still, rolled] = balls(&state)[..] else {
        panic!("two balls");
    };
    assert_eq!((still.slot, rolled.slot), (0, 1));
    assert!(still.position.distance_to(resting(250.0)) < 0.1);
    assert!(
        (rolled.velocity.x - speed).abs() < speed * 0.05,
        "rolling at {} after {speed}",
        rolled.velocity.x
    );
    assert!(rolled.position.x > rolling.position.x + 100.0);
}

#[test]
fn arena_contract_fixture_keeps_to_the_banks_through_falling_and_a_player_who_leaves_and_rejoins() {
    let mut state = plain();
    let fixture = state.default_arena_fixture();
    assert_eq!(state.admit_arena_fixture(&fixture), Ok(0));
    keep(&mut state, 120, "landing");
    let landed = balls(&state);

    // The drain opens under the middle ball, which falls out of the view, and
    // the banks keep the others. Then it closes under them.
    state.preview_event(ClockEventKind::Falling);
    assert_eq!(keep(&mut state, 120, "a standalone Falling"), [(0, 1)]);
    assert_eq!(own_floor(&state), 2, "the banks");
    for ball in balls(&state) {
        assert!(ball.position.distance_to(landed[ball.slot].position) < 0.05);
    }
    while state.event_kind().is_some() {
        keep(&mut state, 1, "Falling ending");
    }
    keep(&mut state, 60, "the closed floor");
    assert_eq!(own_floor(&state), 1);
    for ball in balls(&state) {
        assert!(ball.position.distance_to(landed[ball.slot].position) < 0.1);
    }

    // A player joins the next Falling before its bars move. The arena adopts
    // Falling's world, and the balls move into it as they are, where the bars
    // fall among them. When the player leaves, the lease keeps the terrain,
    // and the player rejoins it.
    state.preview_event(ClockEventKind::Falling);
    let before = balls(&state);
    toggle(&mut state, 1);
    assert!(state.arena.holds(Owner::Visit));
    assert_eq!(balls(&state), before, "the balls move worlds unchanged");
    assert_eq!(own_floor(&state), 0, "Falling's world brings its banks");
    keep(&mut state, 60, "a player joined to Falling");
    assert_eq!(own_floor(&state), 0);
    toggle(&mut state, 1);
    while state.duck_visit.is_some() {
        keep(&mut state, 1, "the player leaving");
    }
    assert!(state.arena.vacant.is_some());
    keep(&mut state, 30, "a vacant lease");
    assert_eq!(own_floor(&state), 0, "the vacant terrain is the floor");
    toggle(&mut state, 1);
    assert!(state.duck_visit.is_some() && state.arena.vacant.is_none());
    keep(&mut state, 60, "the player rejoined");
    toggle(&mut state, 1);
    while state.duck_visit.is_some() || state.event_kind().is_some() {
        keep(&mut state, 1, "Falling ending");
    }

    // The drain closes, and the balls rest on the closed floor again, where
    // the bars left them.
    keep(&mut state, 60, "the closed floor again");
    assert_eq!(slots(&state), [0, 2]);
    assert_eq!(own_floor(&state), 1);
    for ball in balls(&state) {
        assert!((ball.position.y - resting(0.0).y).abs() < 0.1);
        assert!(ball.velocity.length() < 0.5);
    }
    let arena = state.arena_accounting();
    assert_eq!(
        (arena.claims, arena.residents, arena.bodies),
        (1, 1, 3),
        "two balls on the closed floor"
    );
}

#[test]
fn arena_contract_fixture_stands_on_a_visits_course_and_stays_when_it_departs() {
    // A player's course takes the closed floor's place under the balls, and
    // gives it back when the player leaves.
    let mut state = plain();
    let fixture = state.default_arena_fixture();
    assert_eq!(state.admit_arena_fixture(&fixture), Ok(0));
    keep(&mut state, 120, "landing");
    let landed = balls(&state);
    toggle(&mut state, 1);
    keep(&mut state, 120, "a player's course");
    assert_eq!(slots(&state), [0, 1, 2], "ground under every ball");
    assert_eq!(own_floor(&state), 0, "the course is the floor");
    toggle(&mut state, 1);
    while state.duck_visit.is_some() {
        keep(&mut state, 1, "the player departing");
    }
    keep(&mut state, 60, "the closed floor again");
    assert_eq!(own_floor(&state), 1);
    for ball in balls(&state) {
        assert!(ball.position.distance_to(landed[ball.slot].position) < 0.1);
    }

    // An automatic visit's course opens a gap under the middle ball while its
    // doors open, and the ground holds the others still.
    state.preview_event(ClockEventKind::Duck);
    let course = {
        let duck = state.duck_visit.as_ref().expect("an automatic visit");
        CourseGeometry::from_terrain(&duck.terrain).slabs()
    };
    let over_ground = |x: f32| course.iter().any(|(min, max)| (min.x..max.x).contains(&x));
    assert!(over_ground(landed[0].position.x) && over_ground(landed[2].position.x));
    assert!(!over_ground(landed[1].position.x));
    assert_eq!(keep(&mut state, 35, "the doors opening"), [(0, 1)]);
    for ball in balls(&state) {
        assert!(ball.position.distance_to(landed[ball.slot].position) < 0.1);
    }
}

#[test]
fn arena_contract_fixture_stays_when_an_automatic_visit_departs() {
    let mut state = plain();
    state.preview_event(ClockEventKind::Duck);
    while state
        .duck_visit
        .as_ref()
        .is_some_and(|duck| duck.phase != EventPhase::Resetting)
    {
        ClockScenario::step(&mut state, &[], FRAME);
    }

    // Once its duck has gone, balls rest on the course's ground, one at the
    // floor's height and one raised above it.
    let layout = Layout::new(ASPECT);
    let radius = ArenaFixture::radius(layout);
    let course = {
        let duck = state.duck_visit.as_ref().expect("a departing visit");
        CourseGeometry::from_terrain(&duck.terrain).slabs()
    };
    let on = |raised: bool| {
        let (min, max) = course
            .iter()
            .find(|(_, max)| (max.y > layout.floor_y + 1.0) == raised)
            .expect("the course's ground");
        Vec2::new((min.x + max.x) * 0.5, max.y + radius)
    };
    let (level, raised) = (on(false), on(true));
    assert_eq!(
        state.admit_arena_fixture(&[at_rest(level), at_rest(raised)]),
        Ok(0)
    );
    keep(&mut state, 10, "the visit's last fade");
    assert!(balls(&state)[0].position.distance_to(level) < 0.1);
    assert_eq!(own_floor(&state), 0, "the course is the floor");

    // The visit lets the arena go, and the closed floor takes the course's
    // place: level with the first ball, and below the raised one, which falls
    // onto it.
    while state.duck_visit.is_some() {
        keep(&mut state, 1, "the visit departing");
    }
    keep(&mut state, 120, "the closed floor");
    assert_eq!(own_floor(&state), 1);
    let [first, second] = balls(&state)[..] else {
        panic!("both balls stay");
    };
    assert!(first.position.distance_to(level) < 0.1);
    assert!(second.position.distance_to(resting(raised.x)) < 0.1);
    assert_eq!(state.arena_accounting().claims, 1, "only the fixture");
}

#[test]
fn arena_contract_fixture_rides_moving_panels_and_falls_through_their_opening() {
    let mut state = plain();
    assert_eq!(
        state.admit_arena_fixture(&[at_rest(resting(-100.0))]),
        Ok(0)
    );
    keep(&mut state, 30, "at rest");
    state.preview_event(ClockEventKind::Rain);
    let layout = Layout::new(ASPECT);
    let mut ticks = 0;
    while keep(&mut state, 1, "Rain's panels").is_empty() {
        ticks += 1;
        assert!(state.event_kind().is_some(), "it falls while Rain is open");
        assert_eq!(own_floor(&state), 2, "Rain's two panels");
        if let [ball] = balls(&state)[..]
            && ball.position.x.abs() > layout.drain_half_width()
        {
            assert!(
                ball.position.y > layout.floor_y,
                "the panels carry it until it reaches their opening"
            );
        }
    }
    assert!(ticks > 60, "it rolls to the opening as the panels tip");
    assert!(state.arena_fixture(0).is_none(), "its last ball gone");
    assert_eq!(state.arena_accounting().claims, 0);
    assert!(
        state.arena.world().is_none(),
        "the last claim takes the world"
    );
}

#[test]
fn arena_contract_fixture_admitted_while_paused_waits_for_time_to_move() {
    let mut state = plain();
    let fixture = state.default_arena_fixture();
    assert_eq!(state.admit_arena_fixture(&fixture), Ok(0));
    let admitted = balls(&state);
    for _ in 0..30 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(balls(&state), admitted, "a paused clock moves nothing");
    let arena = state.arena_accounting();
    assert_eq!(
        (arena.claims, arena.bodies, arena.steps),
        (1, 3, 0),
        "and builds no floor"
    );
    keep(&mut state, 10, "time moving");
    assert_eq!(state.arena_accounting().bodies, 4, "the closed floor");

    // Paused in mid-fall, the balls hang where they are.
    let falling = balls(&state);
    for _ in 0..30 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(balls(&state), falling);
    keep(&mut state, 1, "time moving again");
    for (ball, before) in balls(&state).iter().zip(&falling) {
        assert!((ball.velocity.y - (before.velocity.y - GRAVITY * DT)).abs() < 1e-3);
    }
}

#[test]
fn arena_contract_fixture_falls_at_clocks_gravity_in_a_visits_world() {
    // At this width a duck's world falls slower than Clock's mechanics do.
    let mut state = plain_at(1.0);
    let layout = Layout::new(1.0);
    assert_eq!(
        state.admit_arena_fixture(&[at_rest(Vec2::new(0.0, layout.floor_y + 300.0))]),
        Ok(0)
    );
    toggle(&mut state, 1);
    keep(&mut state, 10, "a player's world");
    let gravity = state.arena.world().expect("the arena").gravity().y;
    assert!(
        gravity > -GRAVITY + 1.0,
        "the duck's own gravity, {gravity}"
    );
    let [ball] = balls(&state)[..] else {
        panic!("one ball");
    };
    assert!((ball.velocity.y + GRAVITY * 10.0 * DT).abs() < 1e-3);
}

#[test]
fn arena_contract_fixture_takes_its_floor_with_its_last_ball() {
    let mut state = plain();
    state.admit_soil();
    assert_eq!(state.admit_arena_fixture(&[at_rest(resting(0.0))]), Ok(0));
    keep(&mut state, 30, "a ball beside soil");
    assert_eq!(own_floor(&state), 1);
    state.preview_event(ClockEventKind::Falling);
    assert_eq!(keep(&mut state, 60, "the drain opening"), [(0, 0)]);
    assert!(state.arena_fixture(0).is_none(), "its last ball gone");
    assert!(state.arena.holds(Owner::Soil), "the soil keeps the arena");
    assert_eq!(own_floor(&state), 0, "with no floor of its own");
    let arena = state.arena_accounting();
    assert_eq!((arena.claims, arena.residents), (1, 1));
}

/// A fixture of two balls beside the default one, far enough apart that
/// none of them touch.
fn second_fixture() -> [FixtureBall; 2] {
    [
        at_rest(resting(-330.0) + Vec2::new(0.0, 40.0)),
        at_rest(resting(330.0) + Vec2::new(0.0, 40.0)),
    ]
}

#[test]
fn arena_contract_a_second_owner_leaves_without_disturbing_the_fixture() {
    // The same fixture alone, to hold the one beside a second owner against.
    let mut alone = plain();
    let mut state = plain();
    let fixture = state.default_arena_fixture();
    assert_eq!(alone.admit_arena_fixture(&fixture), Ok(0));
    assert_eq!(state.admit_arena_fixture(&fixture), Ok(0));
    assert_eq!(state.admit_arena_fixture(&second_fixture()), Ok(1));
    let arena = state.arena_accounting();
    assert_eq!((arena.claims, arena.residents, arena.bodies), (2, 2, 5));
    for tick in 0..120 {
        keep(&mut alone, 1, "alone");
        keep(
            &mut state,
            1,
            &format!("beside a second owner, tick {tick}"),
        );
        assert_eq!(balls(&state), balls(&alone), "tick {tick}");
    }
    assert_eq!(state.arena_fixture(1).map(|f| f.balls.len()), Some(2));

    let before = state.arena_accounting();
    let resting = balls(&state);
    assert!(state.remove_arena_fixture(1));
    assert!(!state.remove_arena_fixture(1), "it went once");
    assert_eq!(balls(&state), resting, "the removal moves nothing");
    assert!(state.arena_fixture(1).is_none());
    let after = state.arena_accounting();
    assert_eq!(
        (after.claims, after.residents, after.bodies),
        (1, 1, 4),
        "its two balls gone, the first's and the floor kept"
    );
    assert_eq!(after.released, before.released + 1);
    assert_eq!(own_floor(&state), 1);
    for tick in 0..120 {
        keep(&mut alone, 1, "alone");
        keep(
            &mut state,
            1,
            &format!("after the second owner, tick {tick}"),
        );
        assert_eq!(balls(&state), balls(&alone), "tick {tick}");
    }
}

/// The run's state an action the arena refuses must leave as it was.
fn untouched(state: &ClockState) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        every_ball(state),
        state.arena.owners().to_vec(),
        state.event_kind(),
        state.player_duck_session(),
        state.has_duck_visit(),
    )
}

#[test]
fn arena_contract_refuses_an_event_it_has_no_room_for_before_anything_changes() {
    let mut state = plain();
    state.admit_soil();
    let fixture = state.default_arena_fixture();
    assert_eq!(state.admit_arena_fixture(&fixture), Ok(0));
    assert_eq!(state.admit_arena_fixture(&second_fixture()), Ok(1));
    keep(&mut state, 60, "two fixtures and soil");
    // A visit fits beside them, and so does Falling's batch, but Meltdown's
    // and Explosion's cells and Aquarium's digits do not.
    state.preview_event(ClockEventKind::Duck);
    keep(&mut state, 1, "the visit claiming the arena");
    assert!(state.arena.holds(Owner::Visit));
    for kind in [
        ClockEventKind::Aquarium,
        ClockEventKind::Explosion,
        ClockEventKind::Meltdown,
    ] {
        assert!(state.event_blocked_by_arena(kind), "{kind:?}");
    }
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::Rain,
        ClockEventKind::ColorCycle,
    ] {
        assert!(!state.event_blocked_by_arena(kind), "{kind:?}");
    }

    let before = untouched(&state);
    let arena = state.arena_accounting();
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Meltdown)],
        Duration::ZERO,
    );
    assert_eq!(untouched(&state), before, "nothing changed");
    assert_eq!(
        state.arena_accounting(),
        ArenaAccounting {
            refused: arena.refused + 1,
            ..arena
        }
    );
    assert_eq!(
        state.event_notice.map(|(notice, _)| notice),
        Some(Unavailable::Capacity.notice())
    );
    // Once the visit's cooldown has passed, a trigger starts an event, but
    // not one the arena has no room for.
    keep(&mut state, COOLDOWN_TICKS as usize, "the visit going on");
    assert_eq!(state.lifecycle(), EventLifecycle::Idle);
    let before = untouched(&state);
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::Explosion)],
        Duration::ZERO,
    );
    assert_eq!(untouched(&state), before, "nothing triggered");

    // With the second fixture gone there is room, and Meltdown shares the
    // visit's arena with the first.
    assert!(state.remove_arena_fixture(1));
    assert!(!state.event_blocked_by_arena(ClockEventKind::Meltdown));
    state.preview_event(ClockEventKind::Meltdown);
    assert!(state.arena.holds(Owner::Event(ClockEventKind::Meltdown)));
    keep(&mut state, 30, "Meltdown beside the fixture");
    // The event a preview replaces leaves its room to the next.
    assert!(!state.event_blocked_by_arena(ClockEventKind::Explosion));
    state.preview_event(ClockEventKind::Explosion);
    assert!(state.arena.holds(Owner::Event(ClockEventKind::Explosion)));
    keep(&mut state, 30, "Explosion beside the fixture");
}

#[test]
fn arena_contract_refuses_a_player_who_would_bring_an_event_into_a_full_arena() {
    let mut state = plain();
    let fixture = state.default_arena_fixture();
    assert_eq!(state.admit_arena_fixture(&fixture), Ok(0));
    assert_eq!(state.admit_arena_fixture(&second_fixture()), Ok(1));
    keep(&mut state, 60, "two fixtures");
    // A standalone Meltdown needs nothing of the arena until a player joins.
    state.preview_event(ClockEventKind::Meltdown);
    assert_eq!(state.event_kind(), Some(ClockEventKind::Meltdown));
    keep(&mut state, 30, "a standalone Meltdown");

    let before = untouched(&state);
    let refused = state.arena_accounting().refused;
    toggle(&mut state, 1);
    assert_eq!(untouched(&state), before, "no player, nothing changed");
    assert_eq!(state.arena_accounting().refused, refused + 1);
    assert_eq!(
        state.event_notice.map(|(notice, _)| notice),
        Some(Unavailable::Capacity.notice())
    );
    keep(&mut state, 30, "Meltdown going on alone");

    assert!(state.remove_arena_fixture(1));
    toggle(&mut state, 1);
    assert!(state.player_duck_session().is_some(), "the player joins");
    assert!(state.arena.holds(Owner::Event(ClockEventKind::Meltdown)));
    keep(&mut state, 30, "a player in Meltdown beside the fixture");
}

#[test]
fn arena_contract_holds_no_more_residents_than_its_limit() {
    let mut state = plain();
    // Soil's place is reserved, so one fewer fixture fits.
    for index in 0..MAX_RESIDENT_CLAIMS - 1 {
        let x = -300.0 + 150.0 * index as f32;
        assert_eq!(state.admit_arena_fixture(&[at_rest(resting(x))]), Ok(index));
    }
    keep(&mut state, 10, "three fixtures");
    let before = untouched(&state);
    let arena = state.arena_accounting();
    assert_eq!(
        state.admit_arena_fixture(&[at_rest(resting(300.0))]),
        Err(Unavailable::Residents)
    );
    assert_eq!(untouched(&state), before);
    assert_eq!(
        state.arena_accounting(),
        ArenaAccounting {
            refused: arena.refused + 1,
            ..arena
        }
    );
    // Soil takes its place whenever it comes.
    state.admit_soil();
    keep(&mut state, 10, "soil arriving");
    assert!(state.arena.holds(Owner::Soil));
    assert_eq!(state.arena_accounting().residents, MAX_RESIDENT_CLAIMS);
}

/// A view 480 wide, narrower than the ordinary 800, so that balls resting
/// beyond 240 to either side lie outside it.
const NARROW: f32 = 1.0;

/// How far the arena's own floor reaches to either side.
fn floor_reach(state: &ClockState) -> Option<(f32, f32)> {
    let world = state.arena.world()?;
    world
        .body_ids()
        .filter(|body| ARENA_FLOOR.contains(&body.entity.value()))
        .filter_map(|body| world.body_solid_bounds(body))
        .map(|(min, max)| (min.x, max.x))
        .reduce(|(left, right), (min, max)| (left.min(min), right.max(max)))
}

#[test]
fn arena_contract_fixture_comes_within_a_narrower_view_as_it_is() {
    let mut state = plain();
    let rolling = |x: f32, speed: f32| FixtureBall {
        position: resting(x),
        velocity: Vec2::new(speed, 0.0),
    };
    assert_eq!(
        state.admit_arena_fixture(&[
            rolling(-320.0, 30.0),
            at_rest(resting(0.0)),
            rolling(320.0, -30.0)
        ]),
        Ok(0)
    );
    assert!(keep(&mut state, 30, "rolling inward").is_empty());
    // A player's visit holds the arena when the view changes.
    toggle(&mut state, 1);
    assert!(keep(&mut state, 30, "beside a player").is_empty());
    assert!(state.arena.holds(Owner::Visit));
    let arena = state.arena_accounting();
    let before = balls(&state);

    state.set_aspect_ratio(NARROW);
    assert!(!state.has_duck_visit(), "a resize still finishes the visit");
    let after = balls(&state);
    let radius = ArenaFixture::radius(Layout::new(ASPECT));
    let edge = Layout::new(NARROW).bounds_max.x - radius;
    let expected = [-edge, before[1].position.x, edge];
    for ((ball, was), x) in after.iter().zip(&before).zip(expected) {
        assert_eq!(ball.slot, was.slot);
        assert!(
            (ball.position.x - x).abs() < 1.0e-3,
            "ball {} at {:?}, not {x}",
            ball.slot,
            ball.position
        );
        assert_eq!(
            ball.position.y, was.position.y,
            "ball {} keeps its height",
            ball.slot
        );
        assert_eq!(
            ball.velocity, was.velocity,
            "ball {} keeps its motion",
            ball.slot
        );
    }
    assert_eq!(after[1], before[1], "the ball within the view stays put");
    let resized = state.arena_accounting();
    assert_eq!(resized.last_remap, Some(Remap::Clamped { moved: 2 }));
    assert!(!resized.layout_incompatible);
    assert_eq!(resized.worlds, arena.worlds, "the same world");
    assert_eq!(state.arena.owners(), [Owner::Fixture(0)]);
    // The floor follows the view from the next tick, and the balls roll on.
    assert!(keep(&mut state, 60, "in the narrower view").is_empty());
    assert_eq!(floor_reach(&state), Some((-240.0, 240.0)));
    assert_eq!(
        state.arena.world().map(|world| world.length_unit()),
        Some(Layout::new(NARROW).pitch)
    );
    assert!(
        balls(&state)
            .iter()
            .all(|ball| ball.position.x.abs() < 240.0)
    );

    // A wider view has room for everything where it is.
    let before = balls(&state);
    state.set_aspect_ratio(ASPECT);
    assert_eq!(balls(&state), before);
    assert_eq!(state.arena_accounting().last_remap, Some(Remap::Exact));
    assert!(keep(&mut state, 30, "in the wider view").is_empty());
    assert_eq!(floor_reach(&state), Some((-400.0, 400.0)));
    assert_eq!(state.arena_accounting().worlds, arena.worlds);
}

/// Two balls beyond the narrower view's right edge, which would come in to
/// the same place, and one within it.
fn crowded_edge() -> [FixtureBall; 3] {
    [
        at_rest(resting(-100.0)),
        at_rest(resting(300.0)),
        at_rest(resting(330.0)),
    ]
}

#[test]
fn arena_contract_keeps_its_world_when_the_fixture_cannot_come_in_as_it_is() {
    let mut state = plain();
    assert_eq!(state.admit_arena_fixture(&crowded_edge()), Ok(0));
    assert!(keep(&mut state, 30, "settling").is_empty());
    let arena = state.arena_accounting();
    let before = untouched(&state);

    state.set_aspect_ratio(NARROW);
    assert_eq!(untouched(&state), before, "nothing moved");
    let declined = state.arena_accounting();
    assert_eq!(declined.last_remap, Some(Remap::Declined));
    assert!(declined.layout_incompatible);
    assert_eq!(declined.worlds, arena.worlds, "the same world");
    // The world keeps the floor the balls stand on, out of the view.
    assert!(keep(&mut state, 60, "beyond the view").is_empty());
    assert_eq!(floor_reach(&state), Some((-400.0, 400.0)));
    assert_eq!(balls(&state).len(), 3);

    // Nothing that uses the floor starts meanwhile.
    for kind in ClockEventKind::ALL {
        assert_eq!(
            state.event_blocked_by_arena(kind),
            EVENT_CATALOG[kind as usize].uses_floor(),
            "{kind:?}"
        );
    }
    let before = untouched(&state);
    let refused = state.arena_accounting().refused;
    state.preview_event(ClockEventKind::Falling);
    assert_eq!(untouched(&state), before, "no Falling");
    assert_eq!(
        state.event_notice.map(|(notice, _)| notice),
        Some(Unavailable::LayoutIncompatible.notice())
    );
    toggle(&mut state, 1);
    assert_eq!(untouched(&state), before, "no player");
    assert_eq!(state.arena_accounting().refused, refused + 2);
    // An event that leaves the floor alone starts, and a resident joins.
    state.preview_event(ClockEventKind::ColorCycle);
    assert_eq!(state.event_kind(), Some(ClockEventKind::ColorCycle));
    assert_eq!(
        state.admit_arena_fixture(&[at_rest(resting(0.0) + Vec2::new(0.0, 40.0))]),
        Ok(1)
    );
    assert!(keep(&mut state, 60, "a second fixture").is_empty());

    // A view the balls fit, even narrowed, ends it.
    let before = balls(&state);
    state.set_aspect_ratio(1.4);
    assert_eq!(
        state.arena_accounting().last_remap,
        Some(Remap::Clamped { moved: 1 })
    );
    assert!(!state.arena_accounting().layout_incompatible);
    let after = balls(&state);
    assert_eq!((after[0], after[1]), (before[0], before[1]));
    assert!(after[2].position.x < before[2].position.x);
    assert!(keep(&mut state, 30, "in a view they fit").is_empty());
    assert_eq!(floor_reach(&state), Some((-336.0, 336.0)));
    assert!(!state.event_blocked_by_arena(ClockEventKind::Duck));
    toggle(&mut state, 1);
    assert!(state.player_duck_session().is_some(), "the player joins");
}

#[test]
fn arena_contract_keeps_its_world_rather_than_bring_a_falling_ball_into_the_floor() {
    let mut state = plain();
    let rolling_off = FixtureBall {
        position: resting(395.0),
        velocity: Vec2::new(60.0, 0.0),
    };
    assert_eq!(
        state.admit_arena_fixture(&[at_rest(resting(0.0)), rolling_off]),
        Ok(0)
    );
    // The ball rolls off the floor's end and falls beside it.
    let floor_y = Layout::new(ASPECT).floor_y;
    while balls(&state)[1].position.y > floor_y {
        assert!(keep(&mut state, 1, "rolling off").is_empty());
    }
    let before = untouched(&state);
    state.set_aspect_ratio(NARROW);
    assert_eq!(untouched(&state), before, "nothing moved");
    assert_eq!(state.arena_accounting().last_remap, Some(Remap::Declined));
    // It falls out of the view as it would have.
    let mut fell = Vec::new();
    while fell.is_empty() {
        fell = keep(&mut state, 1, "falling beside the floor");
    }
    assert_eq!(fell, [(0, 1)]);
    assert!(state.arena_accounting().layout_incompatible);
    state.set_aspect_ratio(ASPECT);
    assert_eq!(state.arena_accounting().last_remap, Some(Remap::Exact));
    assert!(!state.arena_accounting().layout_incompatible);
}

#[test]
fn arena_contract_schedule_starts_nothing_that_uses_the_floor_while_residents_do_not_fit() {
    let mut state = ready(ClockConfig {
        aspect_ratio: ASPECT,
        event_profile: ClockEventProfile::Demo,
        events: ClockEvents {
            falling: true,
            color_cycle: false,
            meltdown: false,
            duck: false,
            marquee: false,
            digit_slide: false,
            rain: false,
            crow: false,
            explosion: false,
            aquarium: false,
            cell_swap: false,
            grass: false,
        },
        ..ClockConfig::default()
    });
    assert_eq!(state.admit_arena_fixture(&crowded_edge()), Ok(0));
    state.set_aspect_ratio(NARROW);
    assert!(state.arena_accounting().layout_incompatible);
    assert_eq!(state.next_event_tick(), None, "nothing to wait for");
    // The schedule starts Falling every six to ten seconds otherwise.
    assert!(keep(&mut state, 11 * 60, "no Falling").is_empty());
    assert_eq!(state.event_kind(), None);
    state.set_aspect_ratio(ASPECT);
    assert!(state.next_event_tick().is_some());
    let mut ticks = 0;
    while state.event_kind().is_none() {
        assert!(keep(&mut state, 1, "Falling coming back").is_empty());
        ticks += 1;
        assert!(ticks <= 11 * 60, "Falling starts again");
    }
    assert_eq!(state.event_kind(), Some(ClockEventKind::Falling));
}

#[test]
fn arena_contract_a_declined_resize_ends_with_the_residents_that_did_not_fit() {
    let mut state = plain();
    state.admit_soil();
    assert_eq!(state.admit_arena_fixture(&crowded_edge()), Ok(0));
    assert!(keep(&mut state, 30, "settling").is_empty());
    state.set_aspect_ratio(NARROW);
    assert!(state.arena_accounting().layout_incompatible);
    assert!(state.event_blocked_by_arena(ClockEventKind::Duck));

    assert!(state.remove_arena_fixture(0));
    assert!(!state.arena_accounting().layout_incompatible);
    assert!(!state.event_blocked_by_arena(ClockEventKind::Duck));
    assert_eq!(
        state.arena.owners(),
        [Owner::Soil],
        "the soil keeps the world"
    );
    keep(&mut state, 1, "the soil alone");
    assert_eq!(floor_reach(&state), None);
}

/// Debug builds audit the arena before each step.
#[cfg(debug_assertions)]
mod audit {
    use super::*;
    use engine_rapier::world::{
        BodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    };

    #[test]
    #[should_panic(expected = "belongs to no owner")]
    fn arena_contract_audit_finds_a_body_no_owner_claims() {
        let mut state = plain();
        assert_eq!(state.admit_arena_fixture(&[at_rest(resting(0.0))]), Ok(0));
        keep(&mut state, 1, "the fixture alone");
        let stray = PhysicsId::new(7500);
        let world = state.arena.world_mut().expect("the fixture's arena");
        assert!(world.insert_body(
            BodyId::new(stray, BodyRole::PRIMARY),
            BodySpec::default(),
            &[ColliderSpec::ball(
                ColliderId::new(stray, ColliderRole::PRIMARY, 0),
                1.0
            )],
        ));
        keep(&mut state, 1, "a stray body");
    }

    #[test]
    #[should_panic(expected = "more than")]
    fn arena_contract_audit_finds_an_owner_over_its_budget() {
        let mut state = plain();
        let fixture = [-150.0, -50.0, 50.0, 150.0].map(|x| at_rest(resting(x)));
        assert_eq!(state.admit_arena_fixture(&fixture), Ok(0));
        keep(&mut state, 1, "a full fixture");
        let ball = state.arena.world().and_then(|world| {
            world.body_ids().find(|body| {
                crate::arena::ids::canonical::FIXTURE_BALLS.contains(&body.entity.value())
            })
        });
        let ball = ball.expect("a ball");
        let world = state.arena.world_mut().expect("the fixture's arena");
        assert!(world.insert_collider(
            ball,
            &ColliderSpec::ball(ColliderId::new(ball.entity, ColliderRole::PRIMARY, 1), 1.0),
        ));
        keep(&mut state, 1, "a fifth collider");
    }
}
