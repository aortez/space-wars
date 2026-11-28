use std::time::Duration;

use engine_chess::{Color, Move, Square};
use engine_common::{ClockEventKind, ClockEventProfile, ClockFloorMode, Scenario};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId, PhysicsWorld,
};

use engine_core::Vec2;

use super::*;
use crate::arena::{self, Unavailable};
use crate::chess::Refusal;
use crate::chess::crow;
use crate::chess::duck::{self, Task};
use crate::chess::stage::{self, Scene};
use crate::floor::FloorGeometry;
use crate::layout::Layout;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};

/// One of the Clock's ticks.
const TICK: Duration = Duration::from_nanos(16_666_667);

/// More ticks than any visit here takes, so that one that never ends fails
/// its test.
const LONGEST: u64 = 10_000;

/// The board's, the duck's and the crow's identities in the arena, in their
/// order there.
const BOARD: PhysicsId = PhysicsId::new(arena::ids::canonical::CHESS_BOARD.start);
const DUCK: PhysicsId = duck::ENTITY;
const CROW: PhysicsId = PhysicsId::new(arena::ids::canonical::CHESS_CROW.start);

/// A visit on its own, on the closed floor of the 800×480 view, with the
/// world it keeps its board and its birds in.
struct Bench {
    exhibition: Exhibition,
    world: PhysicsWorld,
    visit: Visit,
    /// Ticks the bench has taken.
    ticked: u64,
}

impl Bench {
    fn new(seed: u64) -> Self {
        Self::with(seed, |_| {})
    }

    /// A bench whose world `furnish` has put things in before the stage is
    /// looked for, which the stage does not know of.
    fn with(seed: u64, furnish: impl FnOnce(&mut PhysicsWorld)) -> Self {
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
        lay_floor(&mut world, &support);
        furnish(&mut world);
        exhibition.admit();
        let visit = Visit::new(stage, layout, &exhibition, &mut world);
        Self {
            exhibition,
            world,
            visit,
            ticked: 0,
        }
    }

    /// One tick, as the host takes it: the visit drives its board and its
    /// birds, the arena steps, and the visit takes its tick.
    fn tick(&mut self) -> Option<Event> {
        self.ticked += 1;
        self.visit.drive(&mut self.world);
        self.world.step(crate::events::duck::DT);
        self.visit.tick(&mut self.exhibition, &self.world)
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

    /// What the visit has in the world: everything there but the floor and
    /// whatever a test stood on it.
    fn owned(&self) -> Vec<PhysicsId> {
        let mut ids: Vec<_> = self
            .world
            .entity_ids()
            .filter(|id| [BOARD, DUCK, CROW].contains(id))
            .collect();
        ids.sort();
        ids
    }

    /// Which of its board and its duck the visit has in the world. Its crow
    /// comes and goes by its own times.
    fn board_and_duck(&self) -> Vec<PhysicsId> {
        let mut ids = self.owned();
        ids.retain(|id| *id != CROW);
        ids
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

    /// Ticks the visit until `reached`, and returns how many ticks that
    /// took.
    fn until(&mut self, what: &str, reached: impl Fn(&Visit) -> bool) -> u64 {
        for ticks in 0..LONGEST {
            if reached(&self.visit) {
                return ticks;
            }
            self.tick();
        }
        panic!("the visit never came to {what}");
    }

    /// Ticks the visit until the duck is at `task`.
    fn to(&mut self, task: Task) -> u64 {
        self.until(&format!("{task:?}"), |visit| visit.duck().task() == task)
    }

    /// Ticks the visit until its birds play.
    fn until_play(&mut self) -> u64 {
        self.until("play", |visit| visit.phase() == Phase::Playing)
    }

    /// Ticks the visit until it is over, and returns how many ticks that
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

/// Lays the floor's `support` in `world`, as the arena keeps it under
/// whoever rests on it.
fn lay_floor(world: &mut PhysicsWorld, support: &[(Vec2, Vec2)]) {
    for (index, (min, max)) in support.iter().enumerate() {
        let entity = PhysicsId::new(arena::ids::canonical::ARENA_FLOOR.start + index as u64);
        let half = (*max - *min) * 0.5;
        assert!(world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: (*min + *max) * 0.5,
                ..BodySpec::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                half.x,
                half.y,
            )],
        ));
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

fn case_of(show: &Show) -> Area {
    match show {
        Show::Case(case) => *case,
        other => panic!("{other:?} is no case"),
    }
}

fn game(exhibition: &Exhibition) -> &engine_chess::Game {
    exhibition.current().expect("a match").game()
}

/// The part of the board's place above where its case stands.
fn above_case(stage: &Stage) -> Area {
    let case = stage.case();
    Area::new(
        Vec2::new(case.min.x, case.max.y + 1.0),
        Vec2::new(case.max.x, stage.board.max.y),
    )
}

#[test]
fn the_duck_brings_the_case_sets_it_down_where_the_board_is_to_stand_and_takes_its_seat() {
    let mut bench = Bench::new(5);
    let stage = *bench.visit.stage();
    let layout = Layout::new(800.0 / 480.0);
    let rest = stage.case();
    assert_eq!(stage.duck, Side::Left);

    // The duck comes from beyond the edge of the view on its side with the
    // case before it, and none of either shows yet. Its body and the crow's
    // are the visit's only bodies.
    assert_eq!(bench.owned(), [DUCK, CROW]);
    assert_eq!(bench.visit.duck().task(), Task::Bringing);
    let first = case_of(&bench.show());
    assert!(first.max.x < layout.bounds_min.x, "{first:?}");
    assert!(bench.visit.duck().position().x < first.min.x);

    // It walks to where the board is to stand. The visit shows the case with
    // it all the way, as large as it stands, going no faster than the duck
    // walks.
    let mut shown = vec![first];
    while bench.visit.duck().task() == Task::Bringing {
        bench.tick();
        shown.push(case_of(&bench.show()));
        assert!(bench.ticked < LONGEST);
    }
    let walk = (layout.bounds_max.x - layout.bounds_min.x) / 7.0 / 60.0;
    for pair in shown.windows(2) {
        let (before, after) = (pair[0], pair[1]);
        assert!((after.width() - rest.width()).abs() < 1e-3);
        assert!((after.height() - rest.height()).abs() < 1e-3);
        let moved = after.center() - before.center();
        assert!((0.0..=walk * 1.01).contains(&moved.x), "{moved:?}");
        assert!(moved.y.abs() <= bench.visit.duck().radius() * 0.11);
    }
    let held = *shown.last().unwrap();
    assert!((held.center().x - rest.center().x).abs() < 1.0, "{held:?}");
    assert!(held.min.y > rest.min.y, "it holds the case off the ground");

    // It sets the case down. Only then is the case a body, where it stands,
    // and nothing of the board is above it yet.
    assert_eq!(bench.visit.duck().task(), Task::Lowering);
    assert_eq!(bench.board_and_duck(), [DUCK]);
    let lowering = bench.to(Task::Retiring);
    assert_eq!(lowering, duck::LOWER_TICKS);
    assert_eq!(bench.show(), Show::Case(rest));
    bench.tick();
    assert_eq!(bench.board_and_duck(), [BOARD, DUCK]);
    assert!(bench.taken(rest) && !bench.taken(above_case(&stage)));

    // It steps back, and the case opens as soon as it is clear of the
    // board's place, which the board's square takes at once.
    bench.until("opening", |visit| visit.phase() == Phase::Opening);
    assert!(bench.visit.duck().beside(stage.board));
    assert!(bench.taken(above_case(&stage)));
    assert_eq!(bench.board_and_duck(), [BOARD, DUCK]);

    // At its seat it stands on the ground within its room, facing the
    // board, as near as lets its bill reach the frame's inner edge.
    bench.to(Task::Seated);
    let seated = bench.visit.duck();
    let (at, radius) = (seated.position(), seated.radius());
    let room = stage.room(Side::Left);
    assert!(seated.facing() > 0.0);
    assert!(at.x - radius * duck::BACK >= room.min.x, "{at:?}");
    let frame = stage.board.min.x + stage.side() * engine_chess_board::FRAME_SHARE;
    let bill = at.x + radius * (duck::FRONT + duck::LEAN);
    assert!(
        bill <= frame && frame - bill < radius * 0.21,
        "{bill} and {frame}"
    );
    assert!((at.y - (stage.ground() + radius)).abs() < radius * 0.05);
    assert!(at.y + radius * duck::TOP <= room.max.y);

    // The board is set up well within the ten seconds a visit has for it.
    bench.until_play();
    assert!(bench.ticked < 9 * 60, "{} ticks", bench.ticked);
}

#[test]
fn the_board_opens_sets_its_pieces_out_and_puts_them_away_in_reverse() {
    let mut bench = Bench::new(5);
    let stage = *bench.visit.stage();
    bench.until("opening", |visit| visit.phase() == Phase::Opening);

    // The case opens out into the board.
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
    assert_eq!(bench.board_and_duck(), [BOARD, DUCK]);
    assert!(bench.taken(above_case(&stage)));

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
        bench.taken(above_case(&stage)),
        "the board folds up before it packs"
    );
    // Folded, it is the case again, standing where the board stood, for
    // the duck to come for.
    assert_eq!(bench.visit.phase(), Phase::Leaving);
    assert_eq!(bench.show(), Show::Case(stage.case()));
    bench.tick();
    assert_eq!(bench.board_and_duck(), [BOARD, DUCK]);
    assert!(bench.taken(stage.case()) && !bench.taken(above_case(&stage)));
}

#[test]
fn the_duck_picks_the_case_up_turns_with_it_and_walks_out() {
    let mut bench = Bench::new(5);
    let stage = *bench.visit.stage();
    let layout = Layout::new(800.0 / 480.0);
    let rest = stage.case();
    bench.until_play();
    assert!(bench.visit.pack());
    bench.until("leaving", |visit| visit.phase() == Phase::Leaving);

    // The duck steps up to the case and picks it up. From then on the case
    // is no body.
    assert_eq!(bench.visit.duck().task(), Task::Fetching);
    assert_eq!(bench.show(), Show::Case(rest));
    assert_eq!(bench.board_and_duck(), [BOARD, DUCK]);
    bench.to(Task::Lifting);
    let lifted = case_of(&bench.show());
    assert!(
        (lifted.center() - rest.center()).length() < 0.1,
        "{lifted:?}"
    );
    bench.tick();
    assert_eq!(bench.board_and_duck(), [DUCK]);
    assert!(!bench.taken(rest));
    let lifting = 1 + bench.to(Task::Turning);
    assert_eq!(lifting, duck::LOWER_TICKS);
    let held = case_of(&bench.show());
    assert!(held.min.y > rest.min.y && held.center().x > bench.visit.duck().position().x);

    // It turns to the edge it came by, and the case swings over its head
    // to its other side.
    let mut highest = held.min.y;
    let mut facings = vec![bench.visit.duck().facing()];
    while bench.visit.duck().task() == Task::Turning {
        bench.tick();
        highest = highest.max(case_of(&bench.show()).min.y);
        facings.push(bench.visit.duck().facing());
        assert!(bench.ticked < LONGEST);
    }
    assert_eq!(facings.len() as u64, duck::TURN_TICKS + 1);
    assert_eq!((facings[0], *facings.last().unwrap()), (1.0, -1.0));
    assert_eq!(
        facings.windows(2).filter(|pair| pair[0] != pair[1]).count(),
        1,
        "it turns once"
    );
    let turned = bench.visit.duck();
    let head = turned.position().y + turned.radius() * duck::TOP;
    assert!(highest > head, "the case clears its head: {highest} {head}");
    assert!(highest + rest.height() <= stage.room(Side::Left).max.y);
    let held = case_of(&bench.show());
    assert!(held.center().x < turned.position().x);

    // It walks out the way it came, and leaves the arena once all of it is
    // beyond the edge of the view. The door closes behind it, and then the
    // visit is over.
    assert_eq!(turned.task(), Task::Taking);
    bench.to(Task::Gone);
    let gone = bench.visit.duck();
    assert!(gone.position().x + gone.radius() * duck::BACK <= layout.bounds_min.x);
    assert!(!gone.present() && !gone.lost());
    assert_eq!(bench.owned(), []);
    assert!(!bench.visit.done(), "its door is still open");
    assert!(gone.door_area().is_some());
    let closing = bench.leave();
    assert!((1..=24).contains(&closing), "{closing} ticks");
    assert!(bench.visit.duck().door_area().is_none());
    assert!(!bench.visit.abandoned());
    assert_eq!(game(&bench.exhibition).revision().actions, 0);
}

#[test]
fn packing_while_the_board_is_set_up_puts_it_away_from_where_it_got_to() {
    // A whole visit that packs up as soon as its board is set tells when
    // each part of its setup begins.
    let mut full = Bench::new(6);
    let lowering = full.to(Task::Lowering);
    let retiring = lowering + full.to(Task::Retiring);
    let opening = retiring + full.until("opening", |visit| visit.phase() == Phase::Opening);
    let placing = opening + full.until("placing", |visit| visit.phase() == Phase::Placing);
    let setup = placing + full.until_play();
    assert!(full.visit.pack());
    let whole = setup + full.leave();
    let moments = [
        (0, Task::Bringing, Phase::Arriving),
        (20, Task::Bringing, Phase::Arriving),
        (lowering / 2, Task::Bringing, Phase::Arriving),
        (
            lowering + duck::LOWER_TICKS / 3,
            Task::Lowering,
            Phase::Arriving,
        ),
        (retiring + 3, Task::Retiring, Phase::Arriving),
        (opening + 20, Task::Seated, Phase::Opening),
        (placing + 50, Task::Seated, Phase::Placing),
        (setup - 1, Task::Seated, Phase::Placing),
    ];
    for (at, task, phase) in moments {
        let mut bench = Bench::new(6);
        for _ in 0..at {
            bench.tick();
        }
        assert_eq!(
            (bench.visit.duck().task(), bench.visit.phase()),
            (task, phase),
            "{at}"
        );
        let before = bench.show();
        assert!(bench.visit.pack() && !bench.visit.pack(), "{at}");
        let packing = [Phase::Clearing, Phase::Closing, Phase::Leaving];
        assert!(packing.contains(&bench.visit.phase()), "{at}");
        // The board shows just as it did, and goes from there.
        let after = bench.show();
        match (&before, &after) {
            (Show::Board(_, before), Show::Board(_, after)) => {
                assert_eq!(before.hidden.len(), after.hidden.len(), "{at}");
            }
            (Show::Unfolding { opened: a }, Show::Unfolding { opened: b }) => {
                assert!((a - b).abs() < 0.03, "{at}: {a} and {b}");
            }
            (Show::Case(before), Show::Case(after)) => {
                let moved = (after.center() - before.center()).length();
                assert!(moved < 0.5, "{at}: the case jumps {moved}");
            }
            _ => panic!("{at}: {before:?} became {after:?}"),
        }
        // A case not yet set down never becomes a body. The crow flies out
        // from wherever it is once the board is packed, and not before.
        let carried = matches!(task, Task::Bringing | Task::Lowering);
        let mut left = 0;
        while !bench.visit.done() {
            let going = matches!(
                bench.visit.crow().task(),
                crow::Task::Going | crow::Task::Gone
            );
            assert_eq!(going, bench.visit.phase() == Phase::Leaving, "{at}");
            assert_eq!(bench.tick(), None, "{at}");
            assert!(!carried || !bench.owned().contains(&BOARD), "{at}");
            left += 1;
            assert!(left < LONGEST, "{at}: the visit never left");
        }
        assert!(at + left <= whole, "{at}: {left} more ticks");
        assert!(!bench.visit.abandoned(), "{at}");
        assert_eq!(bench.owned(), [], "{at}");
        assert_eq!(game(&bench.exhibition).revision().actions, 0);
    }
}

#[test]
fn a_move_travels_to_its_square_and_the_board_shows_one_revision_throughout() {
    let mut bench = Bench::new(7);
    bench.until_play();
    let mut acts = Vec::new();
    let mut reaches = Vec::new();
    let mut ticks = 0;
    while acts.len() < 3 {
        assert!(ticks < LONGEST, "no bird acted");
        match bench.tick() {
            Some(Event::Act { action, bird, .. }) => acts.push((ticks, bird, action)),
            Some(Event::Reach { bird, .. }) => reaches.push((ticks, bird)),
            None => {}
        }
        ticks += 1;
        // Whatever the board shows is of the game as it stands.
        if let Show::Board(_, presentation) = bench.show() {
            let game = game(&bench.exhibition);
            assert_eq!(presentation.revision, game.revision());
            assert_eq!(presentation.position, *game.position());
        }
        // From when a bird reaches for its piece until its move has
        // arrived, that bird and no other is moving.
        let reaching = reaches
            .last()
            .filter(|(at, _)| ticks - 1 < at + Pace::CLOCK.reach + MOVE_TICKS)
            .map(|(_, bird)| *bird);
        for bird in [Bird::Crow, Bird::Duck] {
            let expected = if reaching == Some(bird) {
                Pose::Moving
            } else {
                Pose::Waiting
            };
            assert_eq!(
                bench.visit.pose(bird, &bench.exhibition),
                expected,
                "tick {ticks}"
            );
        }
        // Each bird leans over the board for its own moves only. It takes
        // ten ticks to lean all the way and ten to straighten up, as from
        // its last nod over the pieces that were set out.
        let straightened = match reaches.last() {
            None => 10,
            Some((at, _)) => at + Pace::CLOCK.reach + MOVE_TICKS + 10,
        };
        let leans = [
            (Bird::Crow, bench.visit.crow().lean()),
            (Bird::Duck, bench.visit.duck().lean()),
        ];
        for (bird, lean) in leans {
            if reaching == Some(bird) {
                let since = ticks - 1 - reaches.last().unwrap().0;
                assert!(since < 10 || lean == 1.0, "tick {ticks}: {bird:?} {lean}");
            } else if ticks > straightened {
                assert_eq!(lean, 0.0, "tick {ticks}: {bird:?}");
            }
        }
        // Both keep their seats while they play.
        assert!(bench.visit.crow().settled(), "tick {ticks}");
        assert_eq!(bench.visit.duck().task(), Task::Seated, "tick {ticks}");
    }
    // The bird to move acts at the end of each turn of play, and reaches a
    // second before. The birds take turns.
    let turn = Pace::CLOCK.turn;
    let at: Vec<_> = acts.iter().map(|(tick, ..)| *tick).collect();
    assert_eq!(at, [turn - 1, 2 * turn - 1, 3 * turn - 1]);
    let birds: Vec<_> = acts.iter().map(|(_, bird, _)| *bird).collect();
    assert!(birds[0] != birds[1] && birds[0] == birds[2], "{birds:?}");

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
    bench.until("placing", |visit| visit.phase() == Phase::Placing);
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
fn stand_a_box(world: &mut PhysicsWorld, at: Vec2, size: f32) {
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

fn take_the_box(world: &mut PhysicsWorld) {
    let entity = PhysicsId::new(arena::ids::canonical::FIXTURE_BALLS.start);
    assert!(world.remove_entity(entity));
}

/// Ticks `bench` until its visit is over, and returns whether the board was
/// ever a body in the world on the way, and whether it ever opened.
fn watch_it_leave(bench: &mut Bench) -> (bool, bool) {
    let (mut stood, mut opened) = (false, false);
    while !bench.visit.done() {
        assert_eq!(bench.tick(), None);
        stood |= bench.owned().contains(&BOARD);
        opened |= !matches!(bench.show(), Show::Case(_));
        assert!(bench.ticked < LONGEST, "the visit never left");
    }
    (stood, opened)
}

#[test]
fn the_duck_sets_the_case_down_and_the_case_opens_only_while_the_boards_place_is_clear() {
    let stage = *Bench::new(9).visit.stage();
    // A corner of the board's place that neither the duck nor the case
    // comes near.
    let corner = stage.board.max - Vec2::new(4.0, 4.0);

    // Something that comes to stand beside the board's place, on the floor
    // under the crow's way to its room, stops nothing.
    let mut bench = Bench::new(9);
    let beyond = stage.room(Side::Right).max.x + 8.0;
    assert!(beyond + 2.0 < stage.way(Side::Right).max.x);
    let under = Vec2::new(beyond, stage.ground() + 2.0);
    stand_a_box(&mut bench.world, under, 4.0);
    bench.until_play();
    assert!(bench.taken(stage.board));
    assert_eq!(bench.owned(), [BOARD, DUCK, CROW]);
    assert!(bench.visit.crow().settled());

    // Something in the board's place when the duck comes with the case
    // does. The duck takes the case away as it brought it: the case is
    // never set down, the board never enters the world, and nothing is
    // played.
    let mut bench = Bench::new(9);
    stand_a_box(&mut bench.world, corner, 4.0);
    bench.until("leaving", |visit| visit.phase() == Phase::Leaving);
    assert_eq!(bench.visit.duck().task(), Task::Turning);
    assert!(!bench.visit.pack(), "it is packing up already");
    assert_eq!(watch_it_leave(&mut bench), (false, false));
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
    assert_eq!(game(&bench.exhibition).revision().actions, 0);

    // Something that comes there while the duck sets the case down does
    // too. The duck picks the case up again from where it got to.
    let mut bench = Bench::new(9);
    bench.to(Task::Lowering);
    stand_a_box(&mut bench.world, corner, 4.0);
    bench.until("leaving", |visit| visit.phase() == Phase::Leaving);
    assert_eq!(bench.visit.duck().task(), Task::Lifting);
    assert_eq!(watch_it_leave(&mut bench), (false, false));
    assert_eq!(bench.owned(), []);

    // So does something that comes there once the case is down, before the
    // duck is clear of it. The case never opens, and the duck takes it
    // away.
    let mut bench = Bench::new(9);
    bench.to(Task::Retiring);
    stand_a_box(&mut bench.world, corner, 4.0);
    bench.until("leaving", |visit| visit.phase() == Phase::Leaving);
    assert_eq!(bench.visit.duck().task(), Task::Fetching);
    assert_eq!(watch_it_leave(&mut bench), (true, false));
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
    assert_eq!(game(&bench.exhibition).revision().actions, 0);
}

#[test]
fn a_duck_that_is_late_gives_up_and_one_that_cannot_take_the_board_away_ends_the_visit() {
    let stage = *Bench::new(10).visit.stage();
    let ground = stage.ground();
    let way = stage.way(Side::Left);

    // Something in the duck's way in, which its stage knew nothing of. The
    // duck comes up against it, and once it is late it turns back with the
    // case and walks out.
    let block = Vec2::new(way.center().x, ground + 4.0);
    let mut bench = Bench::with(10, |world| stand_a_box(world, block, 8.0));
    let late = bench.until("leaving", |visit| visit.phase() == Phase::Leaving);
    let stopped = bench.visit.duck();
    assert_eq!(stopped.task(), Task::Turning);
    assert!(stopped.position().x + stopped.radius() <= block.x - 4.0 + 0.5);
    // It has had half as long again as its walk takes, and a little more.
    let walk = (stage.case().center().x - way.min.x) / (800.0 / 7.0) * 60.0;
    assert!(
        (walk * 1.5..walk * 1.5 + 4.0 * 60.0).contains(&(late as f32)),
        "late after {late} ticks of a walk of {walk}"
    );
    assert_eq!(watch_it_leave(&mut bench), (false, false));
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);

    // Something that comes to stand in its seat while it sets the case
    // down. It cannot step clear of the board's place, so once it is late
    // it picks the case up again. Then it cannot get past to walk out, and
    // once it is late again the visit is over where it stands: the duck and
    // the case leave the world at once, the crow with them if it is still
    // on its way out, and nothing else does.
    let seat = Vec2::new(stage.room(Side::Left).center().x, ground + 4.0);
    let mut bench = Bench::new(10);
    bench.to(Task::Lowering);
    stand_a_box(&mut bench.world, seat, 8.0);
    bench.until("leaving", |visit| visit.phase() == Phase::Leaving);
    assert_eq!(bench.visit.duck().task(), Task::Fetching);
    assert_eq!(bench.board_and_duck(), [BOARD, DUCK]);
    bench.to(Task::Taking);
    assert_eq!(bench.board_and_duck(), [DUCK]);
    assert!(!bench.visit.done());
    bench.until("its end", Visit::done);
    assert!(bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
    assert_eq!(bench.visit.duck().task(), Task::Gone);
    assert_eq!(bench.visit.crow().task(), crow::Task::Gone);
    assert_eq!(bench.world.body_count(), 2, "the floor and the box stay");

    // Something that leaves it clear of the board's place but three units
    // short of its seat. The case opens, and once the duck is late to its
    // seat it packs the board up and takes it away, its way out free again
    // by then.
    let radius = stage.side() * duck::RADIUS;
    let seat = stage.board.min.x - radius * (duck::FRONT + duck::LEAN + 0.1)
        + stage.side() * engine_chess_board::FRAME_SHARE;
    let near = Vec2::new(seat + 3.0 - radius - 4.0, ground + 4.0);
    let mut bench = Bench::new(10);
    bench.to(Task::Lowering);
    stand_a_box(&mut bench.world, near, 8.0);
    bench.until("opening", |visit| visit.phase() == Phase::Opening);
    assert_eq!(bench.visit.duck().task(), Task::Retiring);
    bench.until("packing", |visit| {
        [Phase::Clearing, Phase::Closing].contains(&visit.phase())
    });
    assert_eq!(bench.visit.duck().task(), Task::Retiring);
    assert!(!bench.visit.pack(), "it is packing up already");
    take_the_box(&mut bench.world);
    assert_eq!(watch_it_leave(&mut bench), (true, true));
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);

    // A duck whose floor goes from under it falls out of the scene, and the
    // visit is over where it stands, board, crow and all. The match keeps
    // what was played.
    let mut bench = Bench::new(10);
    bench.until_play();
    while game(&bench.exhibition).revision().actions == 0 {
        bench.tick();
        assert!(bench.ticked < LONGEST);
    }
    let floor = PhysicsId::new(arena::ids::canonical::ARENA_FLOOR.start);
    assert!(bench.world.remove_entity(floor));
    let fell = bench.until("its end", Visit::done);
    assert!(fell < 60, "{fell} ticks");
    assert!(bench.visit.abandoned() && bench.visit.duck().lost());
    // The crow had only taken to the air.
    assert!(!bench.visit.crow().lost() && !bench.visit.crow().present());
    assert_eq!(bench.owned(), []);
    assert_eq!(bench.world.body_count(), 0);
    assert_eq!(game(&bench.exhibition).revision().actions, 1);
    assert!(bench.visit.play().is_none());
}

#[test]
fn each_bird_takes_the_end_of_the_game_as_it_went_for_it() {
    // The game ends by the bird to move giving it up: the player of White
    // at once, or the player of Black after White's `first` move. Returns
    // the bird that gave up.
    let resigned = |first: &[&str]| -> Bird {
        let mut bench = Bench::new(11);
        bench.until_play();
        let current = bench.exhibition.current.as_mut().unwrap();
        for text in first {
            let color = current.game.position().side_to_move();
            let mv = engine_chess::Action::Move(Move::parse(text).unwrap());
            current
                .game
                .apply(current.game.revision(), color, mv)
                .unwrap();
        }
        let color = current.game.position().side_to_move();
        let loser = current.bird(color);
        let revision = current.game.revision();
        current
            .game
            .apply(revision, color, engine_chess::Action::Resign)
            .unwrap();
        let actions = game(&bench.exhibition).revision().actions;
        bench.tick();
        assert_eq!(bench.visit.pose(loser, &bench.exhibition), Pose::Lost);
        assert_eq!(
            bench.visit.pose(loser.other(), &bench.exhibition),
            Pose::Won
        );
        // The bird that won hops three times where it sits, its body on the
        // ground all the while, and the crow raises its wing as it does.
        // The duck that lost leans over the board, and the crow that lost
        // hangs its head to the ground before it.
        let duck_at = bench.visit.duck().position();
        let crow_at = bench.visit.crow().position();
        let (mut duck_hops, mut crow_hops) = (Vec::new(), Vec::new());
        let mut wings = Vec::new();
        for tick in 0..120 {
            bench.tick();
            let (crow, duck) = (bench.visit.crow(), bench.visit.duck());
            duck_hops.push(duck.hop());
            crow_hops.push(crow.hop());
            wings.push(crow.wing());
            assert!((duck.position() - duck_at).length() < 0.01, "tick {tick}");
            assert!((crow.position() - crow_at).length() < 0.05, "tick {tick}");
            assert_eq!(crow.bowed(), loser == Bird::Crow, "tick {tick}");
            assert_eq!(crow.lean(), 0.0, "tick {tick}");
        }
        // The crow's wing is spread from its first hop until its last has
        // come down, and raised all the way at the top of each.
        let last = crow_hops.iter().rposition(|hop| *hop > 0.0);
        let top = crow_hops.iter().copied().fold(0.0, f32::max);
        for (tick, (wing, hop)) in wings.iter().zip(&crow_hops).enumerate() {
            assert_eq!(wing.is_some(), last.is_some_and(|last| tick <= last));
            let at_the_top = last.is_some() && *hop == top;
            assert!(!at_the_top || *wing == Some(1.0), "tick {tick}: {wing:?}");
        }
        let tops = |hops: &[f32]| {
            hops.windows(3)
                .filter(|hop| hop[1] > hop[0] && hop[1] > hop[2])
                .count()
        };
        let (lost, won, highest, lean) = match loser {
            Bird::Duck => (&duck_hops, &crow_hops, 2.0, 1.0),
            Bird::Crow => (&crow_hops, &duck_hops, 1.5, 0.0),
        };
        assert!(lost.iter().all(|hop| *hop == 0.0), "{lost:?}");
        assert_eq!(tops(won), 3, "{won:?}");
        assert!(won.iter().all(|hop| (0.0..=highest).contains(hop)));
        assert!(won[0] > 0.0 && won.last() == Some(&0.0), "{won:?}");
        assert_eq!(bench.visit.duck().lean(), lean);
        // Nothing more is played, and the visit goes on until it is asked
        // to pack up.
        for _ in 0..Pace::CLOCK.turn {
            bench.tick();
        }
        assert!(bench.visit.play().is_some_and(Play::is_over));
        assert_eq!(game(&bench.exhibition).revision().actions, actions);
        assert_eq!(bench.visit.phase(), Phase::Playing);
        // A reach for a move the ended game refused came to nothing.
        assert_eq!(bench.visit.pose(loser, &bench.exhibition), Pose::Lost);
        assert_eq!(
            bench.visit.pose(loser.other(), &bench.exhibition),
            Pose::Won
        );
        assert!(bench.visit.pack());
        bench.tick();
        for bird in [Bird::Crow, Bird::Duck] {
            assert_eq!(bench.visit.pose(bird, &bench.exhibition), Pose::Packing);
        }
        assert!(!bench.visit.crow().bowed());
        loser
    };
    let losers = [resigned(&[]), resigned(&["e2e4"])];
    assert_eq!(losers[0], losers[1].other(), "each bird has lost one");

    // A drawn game is the same to both. The crow nods to it once, slowly,
    // and keeps its feet.
    let mut bench = Bench::new(11);
    bench.until_play();
    let current = bench.exhibition.current.as_mut().unwrap();
    for text in ["g1f3", "g8f6", "f3g1", "f6g8"].repeat(2) {
        let color = current.game.position().side_to_move();
        let mv = engine_chess::Action::Move(Move::parse(text).unwrap());
        current
            .game
            .apply(current.game.revision(), color, mv)
            .unwrap();
    }
    let color = current.game.position().side_to_move();
    let claim = engine_chess::Action::ClaimDraw {
        claim: engine_chess::DrawClaim::Threefold,
        intended: None,
    };
    current
        .game
        .apply(current.game.revision(), color, claim)
        .unwrap();
    bench.tick();
    for bird in [Bird::Crow, Bird::Duck] {
        assert_eq!(bench.visit.pose(bird, &bench.exhibition), Pose::Drawn);
    }
    let mut nod = Vec::new();
    for _ in 0..60 {
        bench.tick();
        let crow = bench.visit.crow();
        nod.push(crow.lean());
        assert!(!crow.bowed() && crow.hop() == 0.0 && crow.wing().is_none());
    }
    let down = nod.iter().filter(|lean| **lean == 1.0).count();
    assert!((15..30).contains(&down), "{nod:?}");
    assert_eq!(nod.last(), Some(&0.0));
}

/// Stands a fixed wall in `world` across the way out of the crow of `stage`,
/// beyond its room, from the ground to above the top of its way.
fn wall_the_crow_in(world: &mut PhysicsWorld, stage: &Stage) -> f32 {
    let side = stage.duck.other();
    let (room, way) = (stage.room(side), stage.way(side));
    let size = 2.0 * (way.max.y - stage.ground());
    let near = match side {
        Side::Left => room.min.x - 20.0 - size,
        Side::Right => room.max.x + 20.0,
    };
    let at = Vec2::new(near + size * 0.5, stage.ground() + size * 0.5);
    stand_a_box(world, at, size);
    match side {
        Side::Left => near + size,
        Side::Right => near,
    }
}

#[test]
fn the_crow_flies_in_as_the_duck_brings_the_board_and_out_once_the_board_is_packed() {
    let mut bench = Bench::new(13);
    let stage = *bench.visit.stage();
    let layout = Layout::new(800.0 / 480.0);
    let room = stage.room(Side::Right);

    // The crow is the visit's from its admission: out of the view at the
    // side the duck does not come from, on its way to its seat.
    let crow = bench.visit.crow();
    assert_eq!(crow.task(), crow::Task::Coming);
    assert!(crow.present() && crow.position().x > layout.bounds_max.x);
    assert_eq!(bench.owned(), [DUCK, CROW]);

    // It is down at its seat in its room before the case opens, and no
    // part of it came over the board's place on its way.
    let mut coming = 0;
    while bench.visit.phase() == Phase::Arriving {
        bench.tick();
        let crow = bench.visit.crow();
        coming += u64::from(crow.task() == crow::Task::Coming);
        let before = crow.position().x - crow.pitch() * crow::FRONT;
        assert!(before > stage.board.max.x, "tick {}", bench.ticked);
        assert!(bench.ticked < LONGEST, "the case never opened");
    }
    assert_eq!(bench.visit.phase(), Phase::Opening);
    assert!(coming < crow::FLIGHT_TICKS / 2, "{coming} ticks");
    assert!(bench.visit.crow().settled());
    let seat = bench.visit.crow().position();
    assert!(room.min.x < seat.x && seat.x < room.max.x, "{seat:?}");
    assert!((seat.y - stage.ground()).abs() < 0.1, "{seat:?}");

    // It keeps its seat while the pieces come out, and nods over them by
    // turns with the duck.
    let (mut nods, mut last) = ([0, 0], [0.0, 0.0]);
    while matches!(bench.visit.phase(), Phase::Opening | Phase::Placing) {
        bench.tick();
        let leans = [bench.visit.crow().lean(), bench.visit.duck().lean()];
        assert!(leans[0] + leans[1] < 1.001, "tick {}", bench.ticked);
        for bird in 0..2 {
            nods[bird] += usize::from(leans[bird] == 1.0 && last[bird] < 1.0);
        }
        last = leans;
        assert!(bench.visit.crow().settled(), "tick {}", bench.ticked);
    }
    assert!(nods[0] >= 4 && nods[1] >= 4, "{nods:?}");

    // It keeps it while the pieces go back and the board folds up.
    assert_eq!(bench.visit.phase(), Phase::Playing);
    assert!(bench.visit.pack());
    while bench.visit.phase() != Phase::Leaving {
        assert!(bench.visit.crow().settled(), "tick {}", bench.ticked);
        bench.tick();
        assert!(bench.ticked < LONGEST, "the board never folded up");
    }

    // Then it flies out the way it came while the duck comes for the case,
    // and leaves the arena once it is out of the view.
    assert_eq!(bench.visit.crow().task(), crow::Task::Going);
    assert_eq!(bench.visit.duck().task(), Task::Fetching);
    let mut going = 0;
    while bench.visit.crow().present() {
        bench.tick();
        going += 1;
        assert!(going < LONGEST, "the crow never left");
    }
    assert!(going < crow::FLIGHT_TICKS / 2, "{going} ticks");
    let crow = bench.visit.crow();
    assert_eq!(crow.task(), crow::Task::Gone);
    assert!(!crow.lost());
    let tail = crow.position().x - crow.pitch() * crow::BACK;
    assert!(tail > layout.bounds_max.x, "{tail}");
    assert!(!bench.owned().contains(&CROW));

    // The visit goes on until the duck has gone with the case.
    assert!(bench.visit.duck().present() && !bench.visit.done());
    bench.leave();
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
}

#[test]
fn a_crow_that_cannot_come_down_at_its_seat_flies_out_again_and_the_visit_packs_up() {
    let seat = Bench::new(14).visit.crow().seat();
    // Something stands on the crow's seat, which its stage knew nothing
    // of. The crow comes down on it, which is not its seat, and tries
    // again. Meanwhile the duck brings the board and sets it up.
    let block = seat + Vec2::new(0.0, 4.0);
    let mut bench = Bench::with(14, |world| stand_a_box(world, block, 8.0));
    let mut seated = false;
    let late = bench.until("the crow's leaving", |visit| {
        visit.crow().task() == crow::Task::Going
    });
    // Once it is late it flies out, and the visit packs up from where it
    // got to.
    assert!(
        (crow::FLIGHT_TICKS..crow::FLIGHT_TICKS + 3).contains(&late),
        "{late} ticks"
    );
    assert!(!bench.visit.pack(), "it is packing up already");
    assert!(bench.visit.play().is_none());
    let packing = bench.visit.phase();
    assert!(
        [Phase::Clearing, Phase::Closing].contains(&packing),
        "{packing:?}"
    );
    let mut going = 0;
    while !bench.visit.done() {
        assert_eq!(bench.tick(), None);
        let crow = bench.visit.crow();
        seated |= crow.task() == crow::Task::Seated;
        going += u64::from(crow.task() == crow::Task::Going);
        assert_ne!(bench.visit.phase(), Phase::Playing);
        assert!(bench.ticked < LONGEST, "the visit never left");
    }
    assert!(!seated, "it never had its seat");
    assert!(going < crow::FLIGHT_TICKS / 2, "{going} ticks");
    assert!(!bench.visit.crow().lost() && !bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
    assert_eq!(bench.world.body_count(), 2, "the floor and the box stay");
    assert_eq!(game(&bench.exhibition).revision().actions, 0);
}

#[test]
fn a_crow_whose_body_leaves_the_scene_ends_play_and_the_duck_takes_the_board_away() {
    use engine_common::ClockChessBird;
    let mut bench = Bench::new(15);
    bench.until_play();
    while game(&bench.exhibition).revision().actions == 0 {
        bench.tick();
        assert!(bench.ticked < LONGEST, "no bird acted");
    }
    // The crow's body is taken from the arena under it. The visit knows by
    // the tick after, and takes the crow for gone.
    assert!(bench.world.remove_entity(CROW));
    bench.tick();
    assert!(bench.visit.crow().lost() && bench.visit.crow().present());
    bench.tick();
    let crow = bench.visit.crow();
    assert!(crow.lost() && !crow.present());
    assert_eq!(crow.task(), crow::Task::Gone);
    let state = bench.visit.state(&bench.exhibition);
    let birds: Vec<_> = state.actors.iter().map(|actor| actor.bird).collect();
    assert_eq!(birds, [ClockChessBird::Duck]);
    // It packs up: a move on its way arrives, nothing more is played, and
    // the duck packs the board and takes it away as it does any other.
    assert!(!bench.visit.pack(), "it is packing up already");
    assert!(bench.visit.play().is_none());
    let (stood, opened) = watch_it_leave(&mut bench);
    assert!(stood && opened);
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
    assert_eq!(game(&bench.exhibition).revision().actions, 1);

    // So it does when the crow has fallen out of the scene.
    let mut bench = Bench::new(15);
    bench.until_play();
    let fallen = bench.visit.crow().seat() - Vec2::new(0.0, 100.0);
    let body = crate::crow::body::CHESS_CROW.id();
    assert!(bench.world.set_pose(body, fallen, 0.0, true));
    for _ in 0..2 {
        bench.tick();
    }
    assert!(bench.visit.crow().lost() && !bench.visit.crow().present());
    assert!(!bench.owned().contains(&CROW));
    watch_it_leave(&mut bench);
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
}

#[test]
fn a_crow_that_cannot_fly_out_leaves_the_arena_where_it_is_once_it_is_late() {
    let mut bench = Bench::new(16);
    let stage = *bench.visit.stage();
    bench.until_play();
    // Something has come to stand across the crow's way out.
    let wall = wall_the_crow_in(&mut bench.world, &stage);
    assert!(bench.visit.pack());
    bench.until("leaving", |visit| visit.phase() == Phase::Leaving);
    assert_eq!(bench.visit.crow().task(), crow::Task::Going);

    // The duck takes the board away and its door closes behind it, and the
    // visit is not over: its crow is still in the arena, short of the wall.
    bench.to(Task::Gone);
    bench.until("its door's closing", |visit| visit.duck().door_closed());
    assert!(!bench.visit.done());
    let crow = bench.visit.crow();
    assert!(crow.present() && !crow.away() && !crow.late());
    assert!(crow.position().x < wall, "{:?}", crow.position());
    assert_eq!(bench.owned(), [CROW]);

    // Once the crow is late it leaves the arena where it is, and the visit
    // is over. Nothing was left behind, and the visit ended as any other.
    let mut going = bench.visit.crow().ticks();
    while !bench.visit.done() {
        bench.tick();
        going += 1;
        assert!(going < LONGEST, "the visit never left");
    }
    assert!(
        (crow::FLIGHT_TICKS..crow::FLIGHT_TICKS + 3).contains(&going),
        "{going} ticks"
    );
    let crow = bench.visit.crow();
    assert!(!crow.present() && !crow.lost());
    assert!(crow.position().x < wall, "{:?}", crow.position());
    assert!(!bench.visit.abandoned());
    assert_eq!(bench.owned(), []);
    assert_eq!(bench.world.body_count(), 2, "the floor and the wall stay");
}

#[test]
fn the_visit_publishes_its_board_and_its_birds_as_its_own() {
    use engine_common::{ClockChessBird, ClockChessPose};
    let mut bench = Bench::new(12);
    let stage = *bench.visit.stage();
    let layout = Layout::new(800.0 / 480.0);
    let milli = |value: f32| (value * 1000.0).round() as i32;
    let state = bench.visit.state(&bench.exhibition);
    assert_eq!((state.visit_id, state.match_number), (1, 1));
    assert_eq!(
        (state.phase, state.phase_tick, state.actions),
        (Phase::Arriving, 0, 0)
    );
    assert_eq!(
        state.board_milli,
        [
            milli(stage.board.min.x),
            milli(stage.board.min.y),
            milli(stage.board.max.x),
            milli(stage.board.max.y)
        ]
    );
    // Both birds are the visit's from its admission, the crow first, each
    // with its role and the colour it plays.
    let duck_plays_white = bench.exhibition.current().unwrap().bird(Color::White) == Bird::Duck;
    let [crow, duck] = state.actors[..] else {
        panic!("two birds: {:?}", state.actors);
    };
    assert_eq!(
        (crow.bird, duck.bird),
        (ClockChessBird::Crow, ClockChessBird::Duck)
    );
    assert_eq!(
        (crow.white, duck.white),
        (!duck_plays_white, duck_plays_white)
    );
    assert_eq!(
        (crow.pose, duck.pose),
        (ClockChessPose::Arriving, ClockChessPose::Arriving)
    );
    // The duck has the case, and each faces the board from its own side,
    // out of the view. The duck is where the middle of its body is, and
    // the crow where its feet are.
    assert!(duck.carrying && duck.facing_right);
    assert!(!crow.carrying && !crow.facing_right);
    let at = bench.visit.duck().position();
    assert_eq!(duck.position_milli, [milli(at.x), milli(at.y)]);
    let at = bench.visit.crow().position();
    assert_eq!(crow.position_milli, [milli(at.x), milli(at.y)]);
    assert!(crow.position_milli[0] > milli(layout.bounds_max.x));

    // Seated, the duck has the case no longer, the crow stands on the
    // ground in its room, and their poses are the visit's.
    bench.until_play();
    let state = bench.visit.state(&bench.exhibition);
    assert_eq!(state.phase, Phase::Playing);
    let [crow, duck] = state.actors[..] else {
        panic!("two birds: {:?}", state.actors);
    };
    assert_eq!(
        (crow.pose, duck.pose),
        (ClockChessPose::Waiting, ClockChessPose::Waiting)
    );
    assert!(!duck.carrying && duck.facing_right);
    assert!(!crow.carrying && !crow.facing_right);
    let room = stage.room(Side::Right);
    assert!((milli(room.min.x)..milli(room.max.x)).contains(&crow.position_milli[0]));
    assert!((crow.position_milli[1] - milli(stage.ground())).abs() < 100);

    // On their way out the duck carries the case, and each faces the edge
    // it came by.
    assert!(bench.visit.pack());
    bench.to(Task::Taking);
    let state = bench.visit.state(&bench.exhibition);
    let [crow, duck] = state.actors[..] else {
        panic!("two birds: {:?}", state.actors);
    };
    assert_eq!(state.phase, Phase::Leaving);
    assert_eq!(
        (crow.pose, duck.pose),
        (ClockChessPose::Leaving, ClockChessPose::Leaving)
    );
    assert!(duck.carrying && !duck.facing_right);
    assert!(!crow.carrying && crow.facing_right);
    // Each is among the visit's birds until it has gone, and the visit is
    // not over until both have.
    let mut seen: Vec<Vec<ClockChessBird>> = Vec::new();
    while !bench.visit.done() {
        let state = bench.visit.state(&bench.exhibition);
        let birds: Vec<_> = state.actors.iter().map(|actor| actor.bird).collect();
        if seen.last() != Some(&birds) {
            seen.push(birds);
        }
        bench.tick();
        assert!(bench.ticked < LONGEST, "the visit never left");
    }
    assert_eq!(
        seen,
        [
            vec![ClockChessBird::Crow, ClockChessBird::Duck],
            vec![ClockChessBird::Duck],
            vec![]
        ]
    );

    // The run's next visit has the next number, and plays the same match.
    bench.exhibition.admit();
    let next = Visit::new(stage, layout, &bench.exhibition, &mut bench.world);
    let state = next.state(&bench.exhibition);
    assert_eq!((next.id(), state.visit_id, state.match_number), (2, 2, 1));
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
    let stage = *state.chess_visit().unwrap().stage();
    // The duck cannot walk in past them, so it comes from the right, and
    // the crow's way is over them.
    let plants = state.plant_areas();
    assert!(!plants.is_empty());
    assert_eq!(stage.duck, Side::Right);
    let flight = stage.way(Side::Left);
    for area in [
        stage.board,
        stage.room(Side::Left),
        stage.room(Side::Right),
        stage.way(Side::Right),
        flight,
    ] {
        assert!(plants.iter().all(|plant| !plant.overlaps(area)));
    }

    // Through the visit its birds and its board are its owner's only
    // bodies, everything else goes on as it would have, and the arena steps
    // once a tick.
    let with = |own: &[PhysicsId]| {
        let mut ids = before.clone();
        ids.extend(own);
        ids.sort();
        ids
    };
    let (mut ticked, mut played) = (0, None);
    let mut seen = std::collections::BTreeSet::new();
    while let Some(visit) = state.chess_visit() {
        if visit.phase() == Phase::Playing && visit.ticks() == 400 {
            played = Some(ticked);
            assert!(state.pack_chess_visit() && !state.pack_chess_visit());
        }
        ticks(&mut state, 1);
        ticks(&mut reference, 1);
        ticked += 1;
        let (now, without) = (state.arena_accounting(), reference.arena_accounting());
        assert_eq!(now.steps, without.steps, "tick {ticked}");
        if let Some(visit) = state.chess_visit() {
            let own: Vec<_> = [BOARD, DUCK, CROW]
                .into_iter()
                .filter(|id| entities(&state).contains(id))
                .collect();
            assert_eq!(entities(&state), with(&own), "tick {ticked}");
            assert_eq!(now.bodies, without.bodies + own.len(), "tick {ticked}");
            assert_eq!(
                now.colliders,
                without.colliders + own.len(),
                "tick {ticked}"
            );
            assert_eq!(state.arena.owners().last(), Some(&arena::Owner::Chess));
            assert_eq!(own.contains(&DUCK), visit.duck().present(), "tick {ticked}");
            assert_eq!(own.contains(&CROW), visit.crow().present(), "tick {ticked}");
            seen.insert(own);
        }
        assert!(ticked < LONGEST, "the visit never left");
    }
    assert!(played.is_some(), "the board was set up");
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        [
            vec![],
            vec![BOARD, DUCK, CROW],
            vec![DUCK],
            vec![DUCK, CROW]
        ],
        "the birds alone, with their board, the duck alone once the crow \
         has flown out, and neither once it has gone"
    );
    let actions = state.chess().current().unwrap().game().revision().actions;
    assert_eq!(actions, 1);
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
    assert!(state.chess_play().is_none() && state.chess_state().is_none());

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
        let visits = state.chess().visits();
        assert_eq!(state.admit_chess_visit(), Err(refusal));
        assert_eq!(state.arena.owners(), owners);
        assert_eq!(entities, self::entities(state));
        assert_eq!(state.chess().current(), current.as_ref());
        assert_eq!(state.chess().visits(), visits);
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

    // Tanks has the arena to itself. The arena turns the visit away, and
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
fn over_an_open_drain_the_board_stands_on_a_bank_and_the_duck_keeps_to_it() {
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
    // The duck walks in along that bank and sets the case down on it.
    let bank = stage.board.center().x.signum();
    let mut ticked = 0;
    while state.chess_visit().unwrap().phase() == Phase::Arriving {
        ticks(&mut state, 1);
        ticked += 1;
        let visit = state.chess_visit().expect("the visit goes on");
        let at = visit.duck().position();
        assert!(at.x * bank > drain, "tick {ticked}: {at:?}");
        assert!(at.y > stage.ground(), "tick {ticked}: {at:?}");
        assert!(ticked < LONGEST, "the duck never came");
    }
    assert_eq!(state.chess_visit().unwrap().phase(), Phase::Opening);
}

#[test]
fn nobody_takes_the_chess_birds_over_and_they_are_in_no_other_birds_state() {
    use engine_common::ClockChessBird;
    let mut state = clock(55);
    state.admit_chess_visit().expect("a visit");
    while state.chess_visit().unwrap().phase() != Phase::Playing {
        ticks(&mut state, 1);
    }
    // The chess birds are the visit's own: the Clock has no visiting duck
    // and nothing to hand a player, and no crow visits it.
    assert!(state.duck_state().is_none() && state.player_duck_state().is_none());
    assert!(state.player_duck_session().is_none());
    assert!(state.crow_state().is_none() && state.crow_states().is_empty());
    let chess = state.chess_state().expect("a visit's state");
    let birds: Vec<_> = chess.actors.iter().map(|actor| actor.bird).collect();
    assert_eq!(birds, [ClockChessBird::Crow, ClockChessBird::Duck]);
    let seated = chess.actors[1].position_milli;

    // A player who joins gets a duck of their own, with its own course,
    // and neither chess bird is it or theirs.
    ClockScenario::step(
        &mut state,
        &[ClockAction::toggle_player_duck(1)],
        Duration::ZERO,
    );
    let (session, player) = state.player_duck_session().expect("the player's duck");
    assert_eq!(player, 1);
    let joined = state.chess_state().expect("the visit goes on");
    assert_eq!(joined.actors, chess.actors);
    assert!(state.crow_state().is_none() && state.crow_states().is_empty());
    // The player's input moves the player's duck and never the chess duck,
    // which keeps its seat on their course until the visit packs up.
    let input = ClockAction::player_duck_input(crate::ClockDuckInput {
        session_id: session,
        player,
        move_milli: 1000,
        jump: false,
        run: true,
        dive: false,
    });
    let mut ticked = 0;
    while ticked < 600 {
        ClockScenario::step(&mut state, std::slice::from_ref(&input), TICK);
        ticked += 1;
        let Some(chess) = state.chess_state() else {
            break;
        };
        let duck = chess
            .actors
            .iter()
            .find(|actor| actor.bird == ClockChessBird::Duck);
        if let Some(actor) = duck.filter(|_| chess.phase != Phase::Leaving) {
            let moved = (actor.position_milli[0] - seated[0]).abs();
            assert!(moved < 2_000, "tick {ticked}: the chess duck went {moved}");
        }
        // No crow visits the Clock, whatever becomes of the chess crow.
        assert!(state.crow_state().is_none(), "tick {ticked}");
    }
    // Whatever became of the visit, the arena holds what its owners claim.
    assert!(state.player_duck_session().is_some());
    let claims = state.arena_state().claims;
    assert!(
        claims
            .iter()
            .all(|claim| claim.population.within(claim.budget))
    );
}

#[test]
fn a_visit_a_player_joins_ends_or_plays_on_and_leaves_nothing_of_its_own_behind() {
    // A player's duck lays its course in place of the floor, with gaps and
    // raised surfaces about the middle of it, where the board stands. A
    // visit does not check its stage again, so what becomes of it depends
    // on what the course leaves under its birds.
    let own = |state: &ClockState| -> Vec<PhysicsId> {
        entities(state)
            .into_iter()
            .filter(|id| [BOARD, DUCK, CROW].contains(id))
            .collect()
    };
    let (mut ended, mut played_on) = (0, 0);
    for seed in 50..58 {
        let mut state = clock(seed);
        state.admit_chess_visit().expect("a visit");
        while state.chess_visit().unwrap().phase() != Phase::Playing {
            ticks(&mut state, 1);
        }
        ClockScenario::step(
            &mut state,
            &[ClockAction::toggle_player_duck(1)],
            Duration::ZERO,
        );
        assert!(state.player_duck_session().is_some(), "{seed}");
        // A duck left over a gap falls out of the scene, and the visit is
        // over where it stands. A crow left over one is late six seconds
        // later, and the duck then carries the board out. Twenty seconds
        // see the end of either.
        for _ in 0..20 * 60 {
            if state.chess_visit().is_none() {
                break;
            }
            ticks(&mut state, 1);
        }
        if let Some(visit) = state.chess_visit() {
            // The course is level under both birds, and they play on
            // beside the player's duck until the visit is asked to pack up.
            assert_eq!(visit.phase(), Phase::Playing, "{seed}");
            assert_eq!(visit.duck().task(), Task::Seated, "{seed}");
            assert_eq!(visit.crow().task(), crow::Task::Seated, "{seed}");
            assert_eq!(own(&state), [BOARD, DUCK, CROW], "{seed}");
            assert!(state.pack_chess_visit());
            let mut packing = 0;
            while state.chess_visit().is_some() {
                ticks(&mut state, 1);
                packing += 1;
                assert!(packing < LONGEST, "{seed}");
            }
            played_on += 1;
        } else {
            ended += 1;
        }
        // Either way nothing of the visit is left in the arena or in the
        // Clock's state, and what holds the arena is within its claims.
        assert_eq!(own(&state), [], "{seed}");
        assert!(state.chess_state().is_none(), "{seed}");
        let claims = state.arena_state().claims;
        assert!(
            claims
                .iter()
                .all(|claim| claim.population.within(claim.budget)),
            "{seed}"
        );
    }
    // These seeds' courses do both.
    assert!(
        ended > 0 && played_on > 0,
        "{ended} ended, {played_on} played on"
    );
}

#[test]
fn ordinary_crows_keep_their_own_times_and_their_own_state_beside_the_chess_crow() {
    use crate::crow::{CROW_TICKS, MAX_CROWS, Phase as CrowPhase};
    use engine_common::ClockChessBird;
    // By this age every crow of an ordinary visit has started for home.
    const DEADLINE: u64 = 17 * 60;
    let chess_crow = |state: &ClockState| {
        let chess = state.chess_state()?;
        chess
            .actors
            .iter()
            .find(|actor| actor.bird == ClockChessBird::Crow)
            .copied()
    };
    let crow_bodies = |state: &ClockState| -> Vec<usize> {
        (0..MAX_CROWS)
            .filter(|member| {
                let body = crate::crow::body::crow(*member).id().entity;
                entities(state).contains(&body)
            })
            .collect()
    };
    // A crow alone, and as many as come together, each beside the birds at
    // the board.
    for size in [1, MAX_CROWS] {
        let mut state = clock(71);
        state.admit_chess_visit().expect("a visit");
        while state.chess_visit().unwrap().phase() != Phase::Playing {
            ticks(&mut state, 1);
        }
        // The chess crow is in the scene, and it is the chess visit's: no
        // crow visits the Clock, and the crows' state has none.
        assert!(chess_crow(&state).is_some());
        assert!(entities(&state).contains(&CROW));
        assert!(state.crow_state().is_none() && state.crow_states().is_empty());
        assert!(crow_bodies(&state).is_empty());
        assert!(!state.arena.holds(arena::Owner::Crow));
        let visit_id = state.chess_state().unwrap().visit_id;

        // Crows come to visit beside it, with their own members, bodies
        // and age, and the chess crow is none of them.
        assert!(state.preview_crows(size), "{size} crows");
        assert!(state.arena.holds(arena::Owner::Crow));
        assert_eq!(crow_bodies(&state), (0..size).collect::<Vec<_>>());
        let mut age = 0;
        let mut most = 0;
        while !state.crow_states().is_empty() {
            let crows = state.crow_states();
            most = most.max(crows.len());
            assert!(crows.len() <= size, "{size} crows at {age}: {crows:?}");
            assert!(
                crows
                    .iter()
                    .all(|crow| usize::from(crow.member) < size && crow.age_ticks == age),
                "{size} crows at {age}: {crows:?}"
            );
            assert_eq!(
                state.crow_state().map(|crow| crow.member),
                Some(crows[0].member)
            );
            // They start for home by their 17 seconds and none outlasts
            // its 22, however long the chess crow stays.
            assert!(age < CROW_TICKS, "{size} crows");
            if age >= DEADLINE {
                assert!(
                    crows.iter().all(|crow| crow.phase == CrowPhase::Leaving),
                    "{size} crows at {age}: {crows:?}"
                );
            }
            // The chess crow is still in the scene, and still the chess
            // visit's.
            let chess = state.chess_state().expect("the chess visit goes on");
            assert_eq!(chess.visit_id, visit_id);
            assert!(chess_crow(&state).is_some(), "{size} crows at {age}");
            assert!(entities(&state).contains(&CROW), "{size} crows at {age}");
            ticks(&mut state, 1);
            age += 1;
        }
        assert_eq!(most, size);
        eprintln!("{size} ordinary crows beside the chess crow left after {age} ticks");

        // The crows' visit is over and took what was its own. The chess
        // visit goes on with its crow, which has now stayed longer than
        // any ordinary crow may.
        assert!(state.crow_state().is_none() && crow_bodies(&state).is_empty());
        assert!(!state.arena.holds(arena::Owner::Crow));
        assert!(state.arena.holds(arena::Owner::Chess));
        let visit = state.chess_visit().expect("the chess visit goes on");
        assert!(visit.crow().present() && !visit.crow().lost());
        assert!(entities(&state).contains(&CROW));
        ticks(&mut state, CROW_TICKS);
        let visit = state.chess_visit().expect("the chess visit goes on");
        assert!(visit.crow().present() && visit.crow().ticks() > CROW_TICKS);

        // Crows that come as the chess visit packs up are left as they are
        // when its crow leaves the arena.
        assert!(state.preview_crows(size), "{size} crows again");
        assert!(state.pack_chess_visit());
        let mut age = 0;
        while state.chess_visit().is_some() {
            let crows = state.crow_states();
            assert_eq!(crows.len(), size, "{size} crows at {age}: {crows:?}");
            assert!(crows.iter().all(|crow| crow.age_ticks == age));
            assert_eq!(crow_bodies(&state), (0..size).collect::<Vec<_>>());
            ticks(&mut state, 1);
            age += 1;
            assert!(age < DEADLINE, "the chess visit never left");
        }
        assert!(!entities(&state).contains(&CROW));
        assert!(chess_crow(&state).is_none() && state.chess_state().is_none());
        assert!(!state.arena.holds(arena::Owner::Chess));
        assert!(state.arena.holds(arena::Owner::Crow));
        assert_eq!(state.crow_states().len(), size);
        assert_eq!(crow_bodies(&state), (0..size).collect::<Vec<_>>());
    }
}

#[test]
fn a_stranded_spaceling_goes_by_the_chess_crow_as_by_the_crows_that_visit() {
    use crate::crow::MAX_CROWS;
    // What a spaceling is shown of the crows that visit the face, and of
    // the crow at the board: where each is, and where it is going.
    let visiting = |state: &ClockState| state.stranded_view().birds[..2 * MAX_CROWS].to_vec();
    let at_the_board = |state: &ClockState| {
        let birds = state.stranded_view().birds;
        [birds[2 * MAX_CROWS], birds[2 * MAX_CROWS + 1]]
    };
    let mut state = clock(72);
    assert!(state.stranded_view().birds.iter().all(Option::is_none));

    // On its way in, the chess crow is going to its seat.
    state.admit_chess_visit().expect("a visit");
    ticks(&mut state, 30);
    let crow = state.chess_visit().unwrap().crow();
    assert_eq!(crow.task(), crow::Task::Coming);
    assert_eq!(
        at_the_board(&state),
        [Some(crow.position()), Some(crow.seat())]
    );
    assert!(visiting(&state).iter().all(Option::is_none));

    // Seated, it is going nowhere.
    while state.chess_visit().unwrap().phase() != Phase::Playing {
        ticks(&mut state, 1);
    }
    let crow = state.chess_visit().unwrap().crow();
    assert_eq!(crow.task(), crow::Task::Seated);
    assert_eq!(at_the_board(&state), [Some(crow.position()), None]);

    // Crows that visit meanwhile are shown as they are without it, and it
    // beside them.
    assert!(state.preview_crows(MAX_CROWS));
    ticks(&mut state, 30);
    let crows: Vec<_> = state
        .crow_visit
        .as_ref()
        .expect("a visit of crows")
        .crows()
        .flat_map(|crow| [Some(crow.position), crow.destination()])
        .collect();
    assert!(!crows.is_empty());
    let shown = visiting(&state);
    assert_eq!(shown[..crows.len()], crows[..]);
    assert!(shown[crows.len()..].iter().all(Option::is_none));
    let crow = state.chess_visit().unwrap().crow();
    assert_eq!(at_the_board(&state), [Some(crow.position()), None]);

    // Once it has flown out it is shown no longer, though its visit goes on.
    assert!(state.pack_chess_visit());
    while state
        .chess_visit()
        .is_some_and(|visit| visit.crow().present())
    {
        ticks(&mut state, 1);
    }
    assert!(state.chess_visit().is_some(), "the duck is still going");
    assert_eq!(at_the_board(&state), [None, None]);
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
    let mut lengths = Vec::new();
    for visit in 1..=3u32 {
        // The tick in which a visit left was the first without one.
        let waited = u64::from(visit > 1);
        ticks(&mut state, FixtureStart::PAUSE - 1 - waited);
        assert!(state.chess_visit().is_none(), "visit {visit}");
        assert!(state.chess_state().is_none(), "visit {visit}");
        assert!(!state.arena.holds(arena::Owner::Chess), "visit {visit}");
        ticks(&mut state, 1);
        let admitted = state.chess_visit().expect("the fixture's visit");
        assert_eq!((admitted.phase(), admitted.ticks()), (Phase::Arriving, 0));
        assert_eq!(admitted.id(), u64::from(visit));
        sides.push(admitted.side(Color::White));
        assert_eq!(
            state.chess().current().unwrap().number(),
            1,
            "visit {visit}"
        );
        assert_eq!(moves(&state), 2 * (visit as usize - 1), "visit {visit}");
        let (mut lasted, mut setup, mut played) = (0, 0, 0);
        while let Some(visit) = state.chess_visit() {
            assert!(lasted < LONGEST, "the visit never left");
            match visit.phase() {
                Phase::Arriving | Phase::Opening | Phase::Placing => setup += 1,
                Phase::Playing => played += 1,
                _ => {}
            }
            ticks(&mut state, 1);
            lasted += 1;
        }
        // Its board was set up within the ten seconds a visit has for
        // that, it played for the fixture's time, and it took no longer
        // than ten seconds more to pack up and go.
        assert!(setup < 10 * 60, "visit {visit}: {setup} ticks of setup");
        assert_eq!(played, FixtureStart::PLAY + 1, "visit {visit}");
        assert!(lasted - setup - played < 10 * 60, "visit {visit}");
        lengths.push(lasted);
        assert_eq!(moves(&state), 2 * visit as usize, "visit {visit}");
    }
    assert!(sides.iter().all(|side| *side == sides[0]), "{sides:?}");
    assert!(
        lengths.iter().all(|length| *length == lengths[0]),
        "{lengths:?}"
    );

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
