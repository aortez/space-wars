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

use crate::{
    ArenaAccounting, ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState,
    arena::{Owner, ids::canonical::ARENA_FLOOR},
    arena_fixture::{ArenaFixture, FixtureBall, FixtureBallState},
    events::{EventPhase, duck::arena::CourseGeometry},
    layout::Layout,
};
use engine_common::{Action, ClockEventKind, ClockEventProfile, Scenario};
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
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        SEED,
    );
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

fn balls(state: &ClockState) -> Vec<FixtureBallState> {
    state
        .arena_fixture()
        .map_or_else(Vec::new, |fixture| fixture.balls)
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

/// One frame with `actions`, in which every ball keeps its identity and moves
/// on from where it was, never further than its speed takes it, give or take
/// half its radius of contact correction. A ball leaves only by falling out of
/// the view. Returns the slots of those that did.
fn keep_with(state: &mut ClockState, actions: &[Action], what: &str) -> Vec<usize> {
    let layout = Layout::new(state.aspect_ratio());
    let (bottom, slack) = (layout.bounds_min.y, ArenaFixture::radius(layout) * 0.5);
    let before = balls(state);
    let steps = state.arena_accounting().steps;
    ClockScenario::step(state, actions, FRAME);
    if !before.is_empty() {
        assert_eq!(
            state.arena_accounting().steps,
            steps + 1,
            "{what}: one arena step"
        );
    }
    let after = balls(state);
    let mut fell = Vec::new();
    for ball in &before {
        match after.iter().find(|now| now.slot == ball.slot) {
            Some(now) => {
                let reach = ball.velocity.length().max(now.velocity.length()) * DT + slack;
                assert!(
                    now.position.distance_to(ball.position) <= reach,
                    "{what}: ball {} went from {:?} to {:?} ({:?} then {:?})",
                    ball.slot,
                    ball.position,
                    now.position,
                    ball.velocity,
                    now.velocity
                );
            }
            None => {
                assert!(
                    ball.position.y < bottom,
                    "{what}: ball {} left the arena from {:?}",
                    ball.slot,
                    ball.position
                );
                fell.push(ball.slot);
            }
        }
    }
    assert!(
        after
            .iter()
            .all(|now| before.iter().any(|ball| ball.slot == now.slot)),
        "{what}: no ball appears"
    );
    fell
}

/// `ticks` frames of `keep_with` without actions.
fn keep(state: &mut ClockState, ticks: usize, what: &str) -> Vec<usize> {
    (0..ticks)
        .flat_map(|tick| keep_with(state, &[], &format!("{what}, tick {tick}")))
        .collect()
}

#[test]
fn arena_contract_fixture_falls_rests_and_keeps_rolling_while_the_face_changes() {
    let mut state = plain();
    assert!(state.admit_arena_fixture(&[
        at_rest(resting(250.0) + Vec2::new(0.0, 60.0)),
        FixtureBall {
            position: resting(-300.0),
            velocity: Vec2::new(15.0, 0.0),
        },
    ]));
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
    assert!(state.admit_arena_fixture(&fixture));
    keep(&mut state, 120, "landing");
    let landed = balls(&state);

    // The drain opens under the middle ball, which falls out of the view, and
    // the banks keep the others. Then it closes under them.
    state.preview_event(ClockEventKind::Falling);
    assert_eq!(keep(&mut state, 120, "a standalone Falling"), [1]);
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
    assert!(state.admit_arena_fixture(&fixture));
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
    assert_eq!(keep(&mut state, 35, "the doors opening"), [1]);
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
    assert!(state.admit_arena_fixture(&[at_rest(level), at_rest(raised)]));
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
    assert!(state.admit_arena_fixture(&[at_rest(resting(-100.0))]));
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
    assert!(state.arena_fixture().is_none(), "its last ball gone");
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
    assert!(state.admit_arena_fixture(&fixture));
    assert!(
        !state.admit_arena_fixture(&fixture),
        "one fixture at a time"
    );
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
    assert!(state.admit_arena_fixture(&[at_rest(Vec2::new(0.0, layout.floor_y + 300.0))]));
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
    assert!(state.admit_arena_fixture(&[at_rest(resting(0.0))]));
    keep(&mut state, 30, "a ball beside soil");
    assert_eq!(own_floor(&state), 1);
    state.preview_event(ClockEventKind::Falling);
    assert_eq!(keep(&mut state, 60, "the drain opening"), [0]);
    assert!(state.arena_fixture().is_none(), "its last ball gone");
    assert!(state.arena.holds(Owner::Soil), "the soil keeps the arena");
    assert_eq!(own_floor(&state), 0, "with no floor of its own");
    let arena = state.arena_accounting();
    assert_eq!((arena.claims, arena.residents), (1, 1));
}
