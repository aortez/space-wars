use std::time::Duration;

use engine_chess::{Color, Move, Square};
use engine_common::{ClockEventKind, ClockEventProfile, ClockFloorMode, Scenario};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId, PhysicsWorld,
};

use super::*;
use crate::arena::{self, Unavailable};
use crate::chess::Refusal;
use crate::chess::stage::{self, Scene};
use crate::floor::FloorGeometry;
use crate::layout::Layout;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};

/// One of the Clock's ticks.
const TICK: Duration = Duration::from_nanos(16_666_667);

/// The phases a visit sets its board up in, in order.
const SETUP: [Phase; 3] = [Phase::Arriving, Phase::Opening, Phase::Placing];

/// The phases a visit packs its board away in, in order.
const PACKING: [Phase; 3] = [Phase::Clearing, Phase::Closing, Phase::Leaving];

/// More ticks than any visit here takes, so that one that never ends fails
/// its test.
const LONGEST: u64 = 10_000;

/// Ticks from a visit's admission to its first tick of play.
fn setup_ticks() -> u64 {
    SETUP.iter().map(|phase| phase.ticks().unwrap()).sum()
}

/// A visit on its own, on the closed floor of the 800×480 view, with the
/// world it keeps its board in.
struct Bench {
    exhibition: Exhibition,
    world: PhysicsWorld,
    visit: Visit,
}

impl Bench {
    fn new(seed: u64) -> Self {
        let layout = Layout::new(800.0 / 480.0);
        let support: Vec<_> = FloorGeometry::closed(layout).support().collect();
        let scene = Scene {
            layout,
            support: &support,
            worlds: &[],
            plants: &[],
            water: None,
        };
        let stage = Stage::find(&scene, Side::Left).expect("a stage");
        let mut exhibition = Exhibition::new(seed);
        let mut world = PhysicsWorld::new(crate::arena::mechanics_config(layout));
        let visit = Visit::new(stage, exhibition.admit(), &mut world);
        Self {
            exhibition,
            world,
            visit,
        }
    }

    /// One tick, as the host takes it: the board takes its shape before the
    /// arena's step, and the visit its tick after.
    fn tick(&mut self) -> Option<Event> {
        self.visit.drive(&mut self.world);
        self.visit.tick(&mut self.exhibition)
    }

    fn show(&self) -> Show {
        self.visit.show(&self.exhibition)
    }

    /// Whether anything in the world takes the middle of `area`, all but a
    /// sliver at its edges.
    fn taken(&self, area: Area) -> bool {
        let half = (area.max - area.min) * 0.45;
        !self
            .world
            .current_cuboid_is_clear(area.center(), 0.0, half, CollisionGroups::ALL, &[])
    }

    /// The board's own body in the world, and nothing else there.
    fn assert_alone(&self) {
        let board = PhysicsId::new(arena::ids::canonical::CHESS_BOARD.start);
        assert_eq!(self.world.entity_ids().collect::<Vec<_>>(), [board]);
        assert_eq!(
            (self.world.body_count(), self.world.collider_count()),
            (1, 1)
        );
    }

    /// Ticks the visit through the rest of `phase`, to its next phase or to
    /// its end, and returns what it showed on each of those ticks.
    fn through(&mut self, phase: Phase) -> Vec<Show> {
        assert_eq!(self.visit.phase(), phase);
        let mut shown = vec![self.show()];
        for _ in 0..LONGEST {
            self.tick();
            if self.visit.phase() != phase || self.visit.done() {
                return shown;
            }
            shown.push(self.show());
        }
        panic!("{phase:?} never ended");
    }

    /// Ticks the visit until it has gone, and returns how many ticks that
    /// took. Nothing is played on the way.
    fn leave(&mut self) -> u64 {
        for ticks in 0..LONGEST {
            if self.visit.done() {
                return ticks;
            }
            assert_eq!(self.tick(), None);
        }
        panic!("the visit never left");
    }
}

fn hidden_of(show: &Show) -> Squares {
    match show {
        Show::Board(_, presentation) => presentation.hidden,
        other => panic!("{other:?} is no board"),
    }
}

fn opened_of(show: &Show) -> f32 {
    match show {
        Show::Unfolding { opened } => *opened,
        other => panic!("{other:?} is not unfolding"),
    }
}

/// The squares in the order their pieces show as `shows` go by, from none
/// of them to all.
fn order_shown(shows: &[Show]) -> Vec<Square> {
    let mut order = Vec::new();
    for pair in shows.windows(2) {
        let (before, after) = (hidden_of(&pair[0]), hidden_of(&pair[1]));
        let squares =
            (0..8).flat_map(|rank| (0..8).filter_map(move |file| Square::new(file, rank)));
        let mut came: Vec<_> = squares
            .filter(|&square| before.contains(square) && !after.contains(square))
            .collect();
        assert!(came.len() <= 1, "one piece a tick");
        order.append(&mut came);
    }
    order
}

fn squares(names: &[&str]) -> Vec<Square> {
    names
        .iter()
        .map(|name| Square::from_name(name).expect("a square"))
        .collect()
}

#[test]
fn the_board_comes_packed_opens_sets_its_pieces_out_and_puts_them_away_in_reverse() {
    let mut bench = Bench::new(5);
    let stage = *bench.visit.stage();
    let outside_case = Area::new(
        stage.board.min,
        engine_core::Vec2::new(stage.case().min.x, stage.board.max.y),
    );

    // The case comes, and it alone takes its place in the world. It shows
    // from its admission until the tick after it has arrived, when it opens.
    bench.assert_alone();
    assert!(bench.taken(stage.case()) && !bench.taken(outside_case));
    let arriving = bench.through(Phase::Arriving);
    assert_eq!(arriving.len() as u64, Phase::Arriving.ticks().unwrap() + 1);
    assert!(
        arriving
            .iter()
            .all(|show| *show == Show::Case(stage.case()))
    );

    // It opens out into the board, which takes the board's place at once.
    bench.assert_alone();
    assert!(bench.taken(stage.board) && bench.taken(outside_case));
    let opening: Vec<f32> = bench
        .through(Phase::Opening)
        .iter()
        .map(opened_of)
        .collect();
    assert_eq!(opening.len() as u64, Phase::Opening.ticks().unwrap() - 1);
    assert!(opening[0] > 0.0);
    assert!(
        opening
            .windows(2)
            .all(|pair| pair[0] < pair[1] && pair[1] < 1.0)
    );
    bench.assert_alone();
    assert!(bench.taken(stage.board) && bench.taken(outside_case));

    // The pieces come out one at a time, each player's from its own back
    // rank and its king first, the two in turn, with nothing marked.
    let placing = bench.through(Phase::Placing);
    assert_eq!(placing.len() as u64, Phase::Placing.ticks().unwrap());
    assert_eq!(hidden_of(&placing[0]).len(), 32);
    assert_eq!(hidden_of(placing.last().unwrap()).len(), 0);
    let white = squares(&[
        "e1", "a1", "b1", "c1", "d1", "f1", "g1", "h1", "a2", "b2", "c2", "d2", "e2", "f2", "g2",
        "h2",
    ]);
    let black = squares(&[
        "e8", "a8", "b8", "c8", "d8", "f8", "g8", "h8", "a7", "b7", "c7", "d7", "e7", "f7", "g7",
        "h7",
    ]);
    let expected: Vec<_> = white
        .iter()
        .zip(&black)
        .flat_map(|(w, b)| [*w, *b])
        .collect();
    assert_eq!(order_shown(&placing), expected);
    for show in &placing {
        let Show::Board(board, presentation) = show else {
            unreachable!()
        };
        assert!(!presentation.mark_turn && presentation.last_move.is_none());
        assert_eq!(*board, stage.board_for(bench.visit.side(Color::White)));
    }

    // Then they play, and the board marks whose turn it is.
    assert_eq!(bench.visit.phase(), Phase::Playing);
    assert!(bench.visit.play().is_some());
    let Show::Board(_, playing) = bench.show() else {
        unreachable!()
    };
    assert!(playing.mark_turn && playing.hidden.is_empty());
    assert!(bench.visit.pack() && !bench.visit.pack());
    assert!(
        bench.visit.play().is_none(),
        "nothing is searched for after"
    );

    // Packing up goes back the way setting up came.
    bench.tick();
    let clearing = bench.through(Phase::Clearing);
    assert_eq!(clearing.len() as u64, Phase::Clearing.ticks().unwrap());
    assert_eq!(hidden_of(&clearing[0]).len(), 0);
    let mut gone = order_shown(&clearing.iter().rev().cloned().collect::<Vec<_>>());
    gone.reverse();
    // The first piece out is the last back, as the board closes.
    assert_eq!(
        gone,
        expected[1..].iter().rev().copied().collect::<Vec<_>>()
    );
    let closing: Vec<f32> = bench
        .through(Phase::Closing)
        .iter()
        .map(opened_of)
        .collect();
    assert_eq!(closing[0], 1.0);
    assert!(
        closing
            .windows(2)
            .all(|pair| pair[0] > pair[1] && pair[1] > 0.0)
    );
    assert!(
        bench.taken(outside_case),
        "the board folds up before it packs"
    );
    let leaving = bench.through(Phase::Leaving);
    assert_eq!(leaving.len() as u64, Phase::Leaving.ticks().unwrap());
    assert!(leaving.iter().all(|show| *show == Show::Case(stage.case())));
    assert!(bench.visit.done());
    bench.assert_alone();
    assert!(bench.taken(stage.case()) && !bench.taken(outside_case));
}

#[test]
fn packing_while_the_board_is_set_up_puts_it_away_from_where_it_got_to() {
    // A whole visit that packs up as soon as its board is set.
    let mut full = Bench::new(6);
    for _ in 0..setup_ticks() {
        full.tick();
    }
    assert_eq!(full.visit.phase(), Phase::Playing);
    assert!(full.visit.pack());
    let whole = setup_ticks() + full.leave();
    let setup = setup_ticks();
    for at in [0, 10, 29, 30, 50, 74, 75, 100, 150, setup - 1] {
        let mut bench = Bench::new(6);
        for _ in 0..at {
            bench.tick();
        }
        let before = bench.show();
        assert!(bench.visit.pack(), "{at}");
        assert!(PACKING.contains(&bench.visit.phase()), "{at}");
        // The board shows just as it did, and goes from there.
        let after = bench.show();
        match (&before, &after) {
            (Show::Board(_, before), Show::Board(_, after)) => {
                assert_eq!(before.hidden.len(), after.hidden.len(), "{at}");
            }
            (Show::Unfolding { opened: a }, Show::Unfolding { opened: b }) => {
                assert!((a - b).abs() < 0.03, "{at}: {a} and {b}");
            }
            _ => assert_eq!(before, after, "{at}"),
        }
        let left = bench.leave();
        assert!(at + left < whole, "{at}: {left} more ticks");
        assert_eq!(game(&bench.exhibition).revision().actions, 0);
    }
}

fn game(exhibition: &Exhibition) -> &engine_chess::Game {
    exhibition.current().expect("a match").game()
}

#[test]
fn a_move_travels_to_its_square_and_the_board_shows_one_revision_throughout() {
    let mut bench = Bench::new(7);
    let mut acts = Vec::new();
    let mut ticks = 0;
    while acts.len() < 3 {
        assert!(ticks < LONGEST, "no bird acted");
        if let Some(Event::Act { action, .. }) = bench.tick() {
            acts.push((ticks, action));
        }
        ticks += 1;
        // Whatever the board shows is of the game as it stands.
        if let Show::Board(_, presentation) = bench.show() {
            let game = game(&bench.exhibition);
            assert_eq!(presentation.revision, game.revision());
            assert_eq!(presentation.position, *game.position());
        }
    }
    // The bird to move acts at the end of each turn of play.
    let setup = setup_ticks();
    let turn = Pace::CLOCK.turn;
    let at: Vec<_> = acts.iter().map(|(tick, _)| *tick).collect();
    assert_eq!(
        at,
        [setup + turn - 1, setup + 2 * turn - 1, setup + 3 * turn - 1]
    );

    // A move travels for its ticks, from its square to its next, and is
    // still on its way when the visit is asked to pack up, so it arrives
    // first. Nothing more is played.
    let moving = |bench: &Bench| match bench.show() {
        Show::Board(_, presentation) => presentation.transition.map(|t| t.progress),
        _ => None,
    };
    assert_eq!(moving(&bench), Some(0.0));
    for tick in 1..10 {
        bench.tick();
        assert_eq!(moving(&bench), Some(tick as f32 / MOVE_TICKS as f32));
    }
    let actions = game(&bench.exhibition).revision().actions;
    assert!(bench.visit.pack());
    for _ in 10..MOVE_TICKS {
        assert_eq!(bench.visit.phase(), Phase::Playing);
        assert_eq!(bench.tick(), None);
    }
    let last = (MOVE_TICKS - 1) as f32 / MOVE_TICKS as f32;
    assert_eq!(moving(&bench), Some(last));
    bench.tick();
    assert_eq!(moving(&bench), None);
    assert_eq!(bench.visit.phase(), Phase::Clearing);
    bench.leave();
    assert_eq!(game(&bench.exhibition).revision().actions, actions);
}

#[test]
fn a_resumed_match_is_set_out_as_it_stands() {
    let mut bench = Bench::new(8);
    let game = &mut bench.exhibition.current.as_mut().unwrap().game;
    for text in ["e2e4", "c7c5", "g1f3"] {
        let color = game.position().side_to_move();
        let mv = Move::parse(text).unwrap();
        game.apply(game.revision(), color, engine_chess::Action::Move(mv))
            .unwrap();
    }
    for _ in 0..Phase::Arriving.ticks().unwrap() + Phase::Opening.ticks().unwrap() {
        bench.tick();
    }
    let placing = bench.through(Phase::Placing);
    let order = order_shown(&placing);
    let position = self::game(&bench.exhibition).position().clone();
    assert_eq!(order.len(), 32);
    assert!(
        order
            .iter()
            .all(|&square| position.piece_at(square).is_some())
    );
    // Each player's pieces come out by how far they stand from its back
    // rank: White's knight on f3 after the pawns still at home, and its pawn
    // on e4 last. Black's moved pawn is last of its own.
    let white: Vec<_> = order.iter().step_by(2).copied().collect();
    let black: Vec<_> = order.iter().skip(1).step_by(2).copied().collect();
    assert_eq!(white[white.len() - 2..], squares(&["f3", "e4"]));
    assert_eq!(black.last(), Square::from_name("c5").as_ref());
    let Show::Board(_, playing) = bench.show() else {
        unreachable!()
    };
    // At play the last move and whose turn it is show again.
    assert_eq!(playing.last_move, Move::parse("g1f3"));
    assert!(playing.mark_turn);
}

/// Puts a fixed box of side `size` in `world`, centred at `at`, as any body
/// that came to stand there would be.
fn stand_a_box(world: &mut PhysicsWorld, at: engine_core::Vec2, size: f32) {
    let entity = PhysicsId::new(arena::ids::canonical::FIXTURE_BALLS.start);
    assert!(world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: at,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            size * 0.5,
            size * 0.5,
        )],
    ));
}

#[test]
fn the_case_opens_only_while_the_boards_place_is_still_clear() {
    // Something that comes to stand beside the board's place does not stop
    // the case opening.
    let mut bench = Bench::new(9);
    let stage = *bench.visit.stage();
    stand_a_box(&mut bench.world, stage.room(Side::Left).center(), 4.0);
    bench.through(Phase::Arriving);
    assert_eq!(bench.visit.phase(), Phase::Opening);
    assert!(bench.taken(stage.board));

    // Something in the board's place does, though the case itself is clear
    // of it. The case goes as it came: it never opens, the board's square
    // never enters the world, and nothing is played.
    let mut bench = Bench::new(9);
    let corner = stage.board.min + engine_core::Vec2::new(4.0, 4.0);
    assert!(corner.x + 2.0 < stage.case().min.x);
    stand_a_box(&mut bench.world, corner, 4.0);
    let arriving = bench.through(Phase::Arriving);
    assert_eq!(bench.visit.phase(), Phase::Leaving);
    assert!(!bench.visit.pack(), "it is packing up already");
    let leaving = bench.through(Phase::Leaving);
    assert!(bench.visit.done());
    let case = Show::Case(stage.case());
    assert!(arriving.iter().chain(&leaving).all(|show| *show == case));
    let far_corner = Area::new(
        stage.board.max - engine_core::Vec2::new(8.0, 8.0),
        stage.board.max,
    );
    assert!(bench.taken(stage.case()) && !bench.taken(far_corner));
    assert_eq!(bench.world.body_count(), 2);
    assert_eq!(game(&bench.exhibition).revision().actions, 0);
}

fn clock(seed: u64) -> ClockState {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Off,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, seed);
    let reading = ClockAction::set_reading(ClockReading::new(10, 8, 0).unwrap());
    ClockScenario::step(&mut state, &[reading], Duration::ZERO);
    state
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], TICK);
    }
}

fn entities(state: &ClockState) -> Vec<PhysicsId> {
    let mut ids: Vec<_> = state
        .arena
        .world()
        .map(|world| world.entity_ids().collect())
        .unwrap_or_default();
    ids.sort();
    ids
}

/// A Clock with soil, grass growing in it and a ball resting on the floor,
/// all to the left of the face, residents a visit must leave as it finds
/// them.
fn furnished(seed: u64) -> ClockState {
    let mut state = clock(seed);
    state.admit_soil();
    let soil = state.soil().unwrap();
    let columns = [soil.column_at(-150.0), soil.column_at(-110.0)];
    for column in columns {
        state.plant_grass(column).unwrap();
    }
    let ball = state.rolling_arena_ball(4.0, 0.0);
    state.admit_arena_fixture(&[ball]).unwrap();
    ticks(&mut state, 600);
    state
}

#[test]
fn a_visit_leaves_the_arena_and_its_residents_as_it_found_them() {
    let mut reference = furnished(31);
    let mut state = furnished(31);
    let owners = state.arena.owners().to_vec();
    let before = entities(&state);
    assert!(!before.is_empty());

    state.admit_chess_visit().expect("a visit");
    let board = PhysicsId::new(arena::ids::canonical::CHESS_BOARD.start);
    let mut with_board = before.clone();
    with_board.push(board);
    with_board.sort();
    let stage = *state.chess_visit().unwrap().stage();
    // The duck cannot walk in past them, but the crow flies over them.
    let plants = state.plant_areas();
    assert!(!plants.is_empty());
    assert_eq!(stage.duck, Side::Right);
    let flight = stage.way(Side::Left);
    for area in [
        stage.board,
        stage.room(Side::Left),
        stage.room(Side::Right),
        flight,
    ] {
        assert!(plants.iter().all(|plant| !plant.overlaps(area)));
    }

    // Through the visit the board is its owner's one body, everything else
    // goes on as it would have, and the arena steps once a tick.
    let mut ticked = 0;
    while state.chess_visit().is_some() {
        if ticked == setup_ticks() + 400 {
            assert!(state.pack_chess_visit() && !state.pack_chess_visit());
        }
        ticks(&mut state, 1);
        ticks(&mut reference, 1);
        ticked += 1;
        let (with, without) = (state.arena_accounting(), reference.arena_accounting());
        assert_eq!(with.steps, without.steps, "tick {ticked}");
        if state.chess_visit().is_some() {
            assert_eq!(entities(&state), with_board, "tick {ticked}");
            assert_eq!(with.bodies, without.bodies + 1, "tick {ticked}");
            assert_eq!(with.colliders, without.colliders + 1, "tick {ticked}");
            assert_eq!(state.arena.owners().last(), Some(&arena::Owner::Chess));
        }
        assert!(ticked < LONGEST, "the visit never left");
    }
    let played = state.chess().current().unwrap().game().revision().actions;
    assert_eq!(played, 1);
    assert_eq!(state.arena.owners(), owners);
    assert_eq!(entities(&state), before);
    assert_eq!(entities(&state), entities(&reference));
    assert_eq!(
        state.arena_accounting().bodies,
        reference.arena_accounting().bodies
    );
    assert_eq!(
        state.soil().unwrap().quads(),
        reference.soil().unwrap().quads()
    );
    assert_eq!(state.plant_areas(), reference.plant_areas());
    assert_eq!(state.arena_fixtures(), reference.arena_fixtures());
    assert!(state.chess_play().is_none());

    // The next visit finds the board's old place free again.
    state.admit_chess_visit().expect("another visit");
    assert_eq!(state.chess_visit().unwrap().stage().board, stage.board);
}

#[test]
fn a_refused_visit_changes_nothing() {
    let unchanged = |state: &mut ClockState, refusal: Refusal| {
        let owners = state.arena.owners().to_vec();
        let accounting = state.arena_accounting();
        let entities = entities(state);
        let current = state.chess().current().cloned();
        assert_eq!(state.admit_chess_visit(), Err(refusal));
        assert_eq!(state.arena.owners(), owners);
        assert_eq!(entities, self::entities(state));
        assert_eq!(state.chess().current(), current.as_ref());
        let after = state.arena_accounting();
        assert_eq!(after.granted, accounting.granted);
        assert_eq!(after.bodies, accounting.bodies);
    };

    // One visit at a time.
    let mut state = clock(41);
    state.admit_chess_visit().unwrap();
    unchanged(&mut state, Refusal::Busy);

    // An event that has the floor to itself.
    let mut state = clock(42);
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::Rain)],
        Duration::ZERO,
    );
    ticks(&mut state, 30);
    assert_eq!(state.floor.mode(), ClockFloorMode::EventOwned);
    unchanged(&mut state, Refusal::Floor);

    // A view with no room for a legible board.
    let mut state = clock(43);
    state.set_aspect_ratio(2.0);
    unchanged(&mut state, Refusal::Stage(stage::Refusal::TooSmall));

    // Grass all along the floor leaves the duck no way in.
    let mut state = clock(44);
    state.admit_soil();
    let soil = state.soil().unwrap();
    let layout = Layout::new(state.aspect_ratio());
    let columns: Vec<_> = [-0.9, -0.45, 0.0, 0.45, 0.9]
        .iter()
        .map(|share| soil.column_at(layout.bounds_max.x * share))
        .collect();
    for column in columns {
        state.plant_grass(column).unwrap();
    }
    ticks(&mut state, 600);
    unchanged(&mut state, Refusal::Stage(stage::Refusal::NoRoom));

    // Tanks has the arena to itself. The arena turns the board away, and
    // counts that it did.
    let mut state = clock(45);
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::Tanks)],
        Duration::ZERO,
    );
    ticks(&mut state, 2);
    let refused = state.arena_accounting().refused;
    unchanged(&mut state, Refusal::Arena(Unavailable::Capacity));
    assert_eq!(state.arena_accounting().refused, refused + 1);
}

#[test]
fn over_an_open_drain_the_board_stands_on_a_bank() {
    let mut state = clock(51);
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::Falling)],
        Duration::ZERO,
    );
    assert_eq!(state.floor.mode(), ClockFloorMode::DrainOpen);
    let layout = Layout::new(state.aspect_ratio());
    let drain = layout.drain_half_width();
    // The digits have yet to fall, and the board stands to one side of the
    // drain with the duck's way in on that side too.
    assert_eq!(state.admit_chess_visit(), Ok(()));
    let stage = *state.chess_visit().unwrap().stage();
    for area in [
        stage.board,
        stage.room(Side::Left),
        stage.room(Side::Right),
        stage.way(stage.duck),
    ] {
        assert!(area.max.x <= -drain || area.min.x >= drain, "{area:?}");
    }
}

#[test]
fn the_fixture_stages_visit_after_visit_and_each_goes_on_with_the_match() {
    use crate::chess::FixtureStart;
    let config = |aspect_ratio| ClockConfig {
        event_profile: ClockEventProfile::Off,
        chess_fixture: FixtureStart::Visits,
        aspect_ratio,
        ..ClockConfig::default()
    };
    let hear_the_time = |state: &mut ClockState| {
        let reading = ClockAction::set_reading(ClockReading::new(10, 8, 0).unwrap());
        ClockScenario::step(state, &[reading], Duration::ZERO);
    };
    let mut state = ClockScenario::init(config(800.0 / 480.0), 61);
    // Nothing is staged before the Clock has heard the time.
    ticks(&mut state, 2 * FixtureStart::PAUSE);
    assert!(state.chess_visit().is_none() && state.chess().current().is_none());
    hear_the_time(&mut state);

    // Each visit comes after the fixture's pause, plays its two moves and
    // packs up, and the next goes on with the same match from there.
    let moves = |state: &ClockState| {
        state
            .chess()
            .current()
            .map_or(0, |current| current.game().moves().len())
    };
    let mut sides = Vec::new();
    for visit in 1..=3u32 {
        // The tick in which a visit left was the first without one.
        let waited = u64::from(visit > 1);
        ticks(&mut state, FixtureStart::PAUSE - 1 - waited);
        assert!(state.chess_visit().is_none(), "visit {visit}");
        assert!(!state.arena.holds(arena::Owner::Chess), "visit {visit}");
        ticks(&mut state, 1);
        let admitted = state.chess_visit().expect("the fixture's visit");
        assert_eq!((admitted.phase(), admitted.ticks()), (Phase::Arriving, 0));
        sides.push(admitted.side(Color::White));
        assert_eq!(
            state.chess().current().unwrap().number(),
            1,
            "visit {visit}"
        );
        assert_eq!(moves(&state), 2 * (visit as usize - 1), "visit {visit}");
        let mut lasted = 0;
        while state.chess_visit().is_some() {
            assert!(lasted < LONGEST, "visit {visit} never left");
            ticks(&mut state, 1);
            lasted += 1;
        }
        let packing: u64 = PACKING.iter().map(|phase| phase.ticks().unwrap()).sum();
        assert_eq!(lasted, setup_ticks() + FixtureStart::PLAY + 1 + packing);
        assert_eq!(moves(&state), 2 * visit as usize, "visit {visit}");
    }
    assert!(sides.iter().all(|side| *side == sides[0]), "{sides:?}");

    // In a view with no room for the board the fixture stages nothing,
    // however long it waits, and asks nothing of the arena.
    let mut state = ClockScenario::init(config(2.0), 62);
    hear_the_time(&mut state);
    let refused = state.arena_accounting().refused;
    ticks(&mut state, 5 * FixtureStart::PAUSE);
    assert!(state.chess_visit().is_none() && state.chess().current().is_none());
    assert_eq!(state.arena_accounting().refused, refused);
    assert_eq!(
        FixtureStart::from_override(Some("visits")),
        FixtureStart::Visits
    );
    assert_eq!(
        FixtureStart::from_override(Some("other")),
        FixtureStart::None
    );
    assert_eq!(FixtureStart::from_override(None), FixtureStart::None);
}
