//! One visit to the board, from the duck's arrival with the packed case to
//! its departure with it.
//!
//! A [`Visit`] is admitted with its [`Stage`] found and the arena's claim
//! granted. The duck walks in with the board packed in its case, sets the
//! case down where the board is to stand, and steps back to its seat. As
//! soon as it is clear of the board's place the case opens out, and the
//! pieces of the match in hand come out onto their squares, each player's
//! from its own back rank outward. Then the birds
//! play, as [`Play`] paces them, until the host asks the visit to pack up.
//! No action is searched for or taken after that: the pieces go back, the
//! board folds into its case, and the duck picks the case up and walks out
//! with it. The match keeps every action its game accepted.
//!
//! What the board shows always comes from the match's game as it stands, so
//! the pieces set out, those in play and a move in motion are of one
//! revision. Packed, it shows none.
//!
//! In the arena the visit has two bodies. The [`Duck`]'s is there from the
//! visit's admission until the duck has walked out. The board's is fixed, in
//! the chess board's identity, and shaped as the board is drawn: the case
//! while it stands on the ground, and the board's square from the moment the
//! case opens until the board has folded up again. The case the duck holds
//! is no body.
//!
//! The duck sets the case down, and the case opens, only while the board's
//! place is clear of every other body. Where something has come to stand
//! there since the stage was found, the duck takes the case away as it
//! brought it.
//!
//! A duck that is late on its way in turns back, and one late to its seat
//! takes the board away again. A duck that is late to the case or on its way
//! out, or whose body has left the scene, cannot take the board away. The
//! visit is then over where it stands, and the host takes what is left of
//! it out of the arena.

use engine_chess::{Action, Color, Outcome, PieceKind, Position, Square};
use engine_chess_board::{Board, Presentation, Squares, Transition};
use engine_common::{ClockChessActor, ClockChessBird, ClockChessState};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld,
};

use super::duck::{self, Duck, Task};
use super::stage::{Area, Side, Stage};
use super::{Bird, Event, Exhibition, Pace, Play, Pose};
use crate::arena::ids::canonical;
use crate::layout::Layout;

#[cfg(test)]
mod tests;

/// What a visit is doing with its board.
pub use engine_common::ClockChessPhase as Phase;

/// What a phase of a visit takes, beyond its name.
pub(super) trait Timed: Sized {
    fn ticks(self) -> Option<u64>;
    fn next(self) -> Option<Self>;
    fn packed(self) -> bool;
}

impl Timed for Phase {
    /// How many ticks the phase lasts. The duck's walks last as long as they
    /// take, and play lasts until the visit packs up.
    fn ticks(self) -> Option<u64> {
        match self {
            Self::Arriving | Self::Playing | Self::Leaving => None,
            Self::Opening | Self::Closing => Some(45),
            Self::Placing => Some(120),
            Self::Clearing => Some(90),
        }
    }

    fn next(self) -> Option<Self> {
        match self {
            Self::Arriving => Some(Self::Opening),
            Self::Opening => Some(Self::Placing),
            Self::Placing => Some(Self::Playing),
            Self::Playing => Some(Self::Clearing),
            Self::Clearing => Some(Self::Closing),
            Self::Closing => Some(Self::Leaving),
            Self::Leaving => None,
        }
    }

    /// Whether the board is packed in its case.
    fn packed(self) -> bool {
        matches!(self, Self::Arriving | Self::Leaving)
    }
}

/// How long a move takes to travel to its square, in ticks.
pub const MOVE_TICKS: u64 = 30;

/// What a visit's board shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Show {
    /// The case, packed, where it is: with the duck, or standing where the
    /// board is to stand.
    Case(Area),
    /// The board opening out of the case or folding back into it, `opened`
    /// from 0, the case, to 1, the board.
    Unfolding { opened: f32 },
    /// The board, with its pieces as the presentation has them.
    Board(Board, Box<Presentation>),
}

/// The board's shape in the arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Case,
    Board,
}

/// One visit to the board. See the [module documentation](self).
pub struct Visit {
    /// Which of the run's visits this is, from one.
    id: u64,
    stage: Stage,
    /// The session of the match the visit was admitted to.
    session: u64,
    /// The side the player of White sits at.
    white: Side,
    phase: Phase,
    /// Ticks into the phase.
    ticks: u64,
    play: Option<Play>,
    /// Ticks since the last action the game accepted, while it was a move
    /// still on its way.
    moving: Option<u64>,
    /// The bird that reaches for its piece, or whose move is on its way.
    reaching: Option<Bird>,
    /// The host asked the visit to pack up, or the visit gave up.
    packing: bool,
    /// The case stands on the ground where the board is to stand.
    down: bool,
    /// The board's shape in the arena, as last driven. It has none while the
    /// duck has the case.
    shape: Option<Shape>,
    duck: Duck,
    /// The visit ended where it stood, with no duck to take the board away.
    abandoned: bool,
}

impl Visit {
    /// The visit `exhibition` has just admitted to its match in hand, on
    /// `stage` in the view `layout` lays out. The arena's claim was granted,
    /// and the duck goes into `world`, out of the view with the case.
    pub(crate) fn new(
        stage: Stage,
        layout: Layout,
        exhibition: &Exhibition,
        world: &mut PhysicsWorld,
    ) -> Self {
        let current = exhibition.current().expect("an admitted visit's match");
        let white = match current.bird(Color::White) {
            Bird::Duck => stage.duck,
            Bird::Crow => stage.duck.other(),
        };
        Self {
            id: exhibition.visits(),
            stage,
            session: current.seed,
            white,
            phase: Phase::Arriving,
            ticks: 0,
            play: None,
            moving: None,
            reaching: None,
            packing: false,
            down: false,
            shape: None,
            duck: Duck::new(&stage, layout, world),
            abandoned: false,
        }
    }

    /// Which of the run's visits this is, from one.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Ticks into the phase.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Play at the board, while the birds play.
    pub fn play(&self) -> Option<&Play> {
        self.play.as_ref()
    }

    /// The visit's duck.
    pub fn duck(&self) -> &Duck {
        &self.duck
    }

    /// The side the bird playing `color` sits at.
    pub fn side(&self, color: Color) -> Side {
        match color {
            Color::White => self.white,
            Color::Black => self.white.other(),
        }
    }

    /// Whether the visit is over: the duck has gone with the case and its
    /// door has closed behind it, or the visit ended where it stood.
    pub fn done(&self) -> bool {
        self.abandoned
            || (self.phase == Phase::Leaving
                && self.duck.task() == Task::Gone
                && self.duck.door_closed())
    }

    /// Whether the visit ended where it stood, with no duck to take the
    /// board away.
    pub fn abandoned(&self) -> bool {
        self.abandoned
    }

    /// Asks the visit to pack up: no action is searched for or taken from
    /// now on. A move on its way still arrives, a board still being set up
    /// is put away from where it got to, and a duck still bringing the case
    /// takes it away again. False if the visit is packing up already.
    pub fn pack(&mut self) -> bool {
        if self.packing {
            return false;
        }
        self.packing = true;
        self.play = None;
        if self.moving.is_none() {
            self.reaching = None;
        }
        let undo = match self.phase {
            Phase::Arriving => {
                self.leave();
                return true;
            }
            Phase::Opening => Phase::Closing,
            Phase::Placing => Phase::Clearing,
            _ => return true,
        };
        let (done, total) = (self.fraction(), undo.ticks().expect("a timed phase"));
        self.phase = undo;
        self.ticks = total - (done * total as f32).round() as u64;
        true
    }

    /// How far through its phase the visit is, from 0 to 1. A phase that
    /// lasts as long as it takes is always at its start.
    fn fraction(&self) -> f32 {
        self.phase.ticks().map_or(0.0, |total| {
            (self.ticks as f32 / total as f32).clamp(0.0, 1.0)
        })
    }

    /// The duck takes the case away, from whatever it is doing with it.
    fn leave(&mut self) {
        self.enter(Phase::Leaving);
        match self.duck.task() {
            Task::Bringing => self.duck.begin(Task::Turning),
            Task::Lowering => self.duck.lift_again(),
            Task::Retiring | Task::Seated => self.duck.begin(Task::Fetching),
            Task::Fetching | Task::Lifting | Task::Turning | Task::Taking | Task::Gone => {}
        }
    }

    /// The visit gives up on its board before it has opened: the duck takes
    /// the case away, and nothing is played.
    fn give_up(&mut self) {
        self.packing = true;
        self.play = None;
        self.leave();
    }

    /// The visit is over where it stands. Its duck leaves `world` at once,
    /// and its board with the next shape it is given.
    fn abandon(&mut self, world: &mut PhysicsWorld) {
        self.abandoned = true;
        self.packing = true;
        self.play = None;
        self.down = false;
        self.duck.remove(world);
        self.enter(Phase::Leaving);
    }

    /// Whether the board's place in `world` is clear of every body but the
    /// visit's own.
    fn clear(&self, world: &PhysicsWorld) -> bool {
        let own = [PhysicsId::new(canonical::CHESS_BOARD.start), duck::ENTITY];
        self.stage.board.is_clear_in(world, &own)
    }

    /// Has the duck go on with the case as far as it has got, gives the
    /// board in `world` the shape it has now, and steers the duck for the
    /// step to come. The host calls this before the arena's step.
    pub fn drive(&mut self, world: &mut PhysicsWorld) {
        if !self.abandoned {
            self.direct(world);
        }
        self.shape_board(world);
        self.duck.drive(world);
    }

    /// The duck's next task with the case, once it has done its last, and
    /// what a duck that cannot do its task does instead.
    fn direct(&mut self, world: &mut PhysicsWorld) {
        let task = self.duck.task();
        let stuck = self.duck.late() && matches!(task, Task::Fetching | Task::Taking);
        if self.duck.lost() || stuck {
            self.abandon(world);
            return;
        }
        match (self.phase, task) {
            (Phase::Arriving, Task::Bringing | Task::Retiring) if self.duck.late() => {
                self.give_up();
            }
            // Late to its seat with the board open already, it packs up.
            (_, Task::Retiring) if self.duck.late() => {
                self.pack();
            }
            (Phase::Arriving, Task::Bringing) if self.duck.arrived() => {
                if self.clear(world) {
                    self.duck.begin(Task::Lowering);
                } else {
                    self.give_up();
                }
            }
            (Phase::Arriving, Task::Lowering) if self.duck.finished() => {
                if self.clear(world) {
                    self.down = true;
                    self.duck.begin(Task::Retiring);
                } else {
                    self.give_up();
                }
            }
            // The case opens as soon as the duck has stepped clear of the
            // board's place, on its way to its seat.
            (Phase::Arriving, Task::Retiring) if self.duck.beside(self.stage.board) => {
                if self.clear(world) {
                    self.enter(Phase::Opening);
                } else {
                    self.give_up();
                }
            }
            (_, Task::Retiring) if self.duck.arrived() => self.duck.begin(Task::Seated),
            (Phase::Leaving, Task::Fetching) if self.duck.arrived() => {
                self.down = false;
                self.duck.begin(Task::Lifting);
            }
            (Phase::Leaving, Task::Lifting) if self.duck.finished() => {
                self.duck.begin(Task::Turning);
            }
            (Phase::Leaving, Task::Turning) if self.duck.finished() => {
                self.duck.begin(Task::Taking);
            }
            (Phase::Leaving, Task::Taking) if self.duck.arrived() => {
                self.duck.remove(world);
            }
            _ => {}
        }
    }

    /// Gives the board in `world` the shape it has now: none while the duck
    /// has the case, the case while it stands packed, and the board's square
    /// while it is open.
    fn shape_board(&mut self, world: &mut PhysicsWorld) {
        let shape = if self.abandoned {
            None
        } else if self.phase.packed() {
            self.down.then_some(Shape::Case)
        } else {
            Some(Shape::Board)
        };
        if self.shape == shape {
            return;
        }
        let entity = PhysicsId::new(canonical::CHESS_BOARD.start);
        if self.shape.is_some() {
            assert!(world.remove_entity(entity), "the board is in the arena");
        }
        if let Some(shape) = shape {
            let area = match shape {
                Shape::Case => self.stage.case(),
                Shape::Board => self.stage.board,
            };
            let half = (area.max - area.min) * 0.5;
            assert!(
                world.insert_body(
                    BodyId::new(entity, BodyRole::PRIMARY),
                    BodySpec {
                        kind: BodyKind::Fixed,
                        position: area.center(),
                        ..BodySpec::default()
                    },
                    &[ColliderSpec::cuboid(
                        ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                        half.x,
                        half.y,
                    )],
                ),
                "the board goes into the arena"
            );
        }
        self.shape = shape;
    }

    /// Takes one tick of the visit: reads the duck back from `world` after
    /// the arena's step, plays a tick at the board while the birds play
    /// there, and returns what they did in it. The host calls this for each
    /// tick that moves time.
    pub fn tick(&mut self, exhibition: &mut Exhibition, world: &PhysicsWorld) -> Option<Event> {
        self.duck.observe(world);
        let event = self.advance(exhibition);
        self.duck.set_pose(self.pose(Bird::Duck, exhibition));
        event
    }

    /// The board's part of a tick.
    fn advance(&mut self, exhibition: &mut Exhibition) -> Option<Event> {
        let mut event = None;
        if let Some(moving) = &mut self.moving {
            *moving += 1;
            if *moving >= MOVE_TICKS {
                self.moving = None;
                self.reaching = None;
            }
        }
        self.ticks += 1;
        if self.phase == Phase::Playing {
            if let Some(play) = &mut self.play {
                event = play.tick(exhibition);
                match event {
                    Some(Event::Reach { bird, .. }) => self.reaching = Some(bird),
                    Some(Event::Act { action, .. }) => {
                        self.moving = matches!(action, Action::Move(_)).then_some(0);
                    }
                    None => {}
                }
            }
            // A bird that reaches goes on to move its piece, or its reach
            // comes to nothing.
            let reaches = self
                .play
                .as_ref()
                .is_some_and(|play| play.reaching().is_some());
            if !reaches && self.moving.is_none() {
                self.reaching = None;
            }
            if self.packing && self.moving.is_none() {
                self.enter(Phase::Clearing);
            }
            return event;
        }
        // The duck's walks end in `drive`, which can ask the arena.
        if let Some(total) = self.phase.ticks()
            && self.ticks >= total
            && let Some(next) = self.phase.next()
        {
            self.enter(next);
            if next == Phase::Playing {
                let current = exhibition
                    .current
                    .as_ref()
                    .filter(|current| current.seed == self.session);
                // A match no longer in hand plays nothing, and the visit
                // packs up.
                match current {
                    Some(current) if !self.packing => {
                        self.play = Some(Play::begin(current, Pace::CLOCK));
                    }
                    _ => {
                        self.packing = true;
                    }
                }
            }
            if next == Phase::Leaving {
                self.leave();
            }
        }
        event
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.ticks = 0;
    }

    /// What `bird` is doing at the board now, of `exhibition`'s match in
    /// hand.
    pub fn pose(&self, bird: Bird, exhibition: &Exhibition) -> Pose {
        match self.phase {
            Phase::Arriving => Pose::Arriving,
            Phase::Opening | Phase::Placing => Pose::Setting,
            Phase::Clearing | Phase::Closing => Pose::Packing,
            Phase::Leaving => Pose::Leaving,
            Phase::Playing if self.reaching == Some(bird) => Pose::Moving,
            // A move on its way arrives before either bird takes it in.
            Phase::Playing if self.moving.is_some() => Pose::Waiting,
            Phase::Playing => {
                let current = exhibition
                    .current
                    .as_ref()
                    .filter(|current| current.seed == self.session);
                let ended = current.and_then(|current| Some((current, current.game.outcome()?)));
                match ended {
                    None => Pose::Waiting,
                    Some((_, Outcome::Draw(_))) => Pose::Drawn,
                    Some((
                        current,
                        Outcome::Checkmate { winner } | Outcome::Resignation { winner },
                    )) => {
                        if current.bird(winner) == bird {
                            Pose::Won
                        } else {
                            Pose::Lost
                        }
                    }
                }
            }
        }
    }

    /// What the board shows now, of `exhibition`'s match in hand.
    pub fn show(&self, exhibition: &Exhibition) -> Show {
        let game = exhibition
            .current
            .as_ref()
            .filter(|current| current.seed == self.session)
            .map(|current| &current.game);
        match self.phase {
            Phase::Arriving | Phase::Leaving => {
                return Show::Case(self.duck.case().unwrap_or_else(|| self.stage.case()));
            }
            Phase::Opening => {
                return Show::Unfolding {
                    opened: self.fraction(),
                };
            }
            Phase::Closing => {
                return Show::Unfolding {
                    opened: 1.0 - self.fraction(),
                };
            }
            Phase::Placing | Phase::Playing | Phase::Clearing => {}
        }
        let Some(game) = game else {
            return Show::Unfolding { opened: 1.0 };
        };
        let board = self.stage.board_for(self.white);
        let out = match self.phase {
            Phase::Placing => self.fraction(),
            Phase::Clearing => 1.0 - self.fraction(),
            _ => 1.0,
        };
        let presentation = if self.phase == Phase::Playing {
            Presentation {
                transition: self.moving.and_then(|ticks| {
                    Transition::of_last_move(game, ticks as f32 / MOVE_TICKS as f32)
                }),
                ..Presentation::of(game)
            }
        } else {
            // Setting out and putting away, the board marks nothing.
            Presentation {
                last_move: None,
                mark_turn: false,
                outcome: None,
                hidden: hidden(game.position(), out),
                ..Presentation::of(game)
            }
        };
        Show::Board(board, Box::new(presentation))
    }

    /// What Clock publishes about the visit, of `exhibition`'s match in
    /// hand: its board and the birds of it that are in the scene.
    pub fn state(&self, exhibition: &Exhibition) -> ClockChessState {
        let milli = |value: f32| (value * 1000.0).round() as i32;
        let current = exhibition
            .current
            .as_ref()
            .filter(|current| current.seed == self.session);
        let board = self.stage.board;
        let mut actors = Vec::new();
        if self.duck.present() {
            let at = self.duck.position();
            actors.push(ClockChessActor {
                bird: ClockChessBird::Duck,
                white: current.is_some_and(|current| current.bird(Color::White) == Bird::Duck),
                pose: self.pose(Bird::Duck, exhibition),
                position_milli: [milli(at.x), milli(at.y)],
                facing_right: self.duck.facing() > 0.0,
                carrying: self.duck.case().is_some(),
            });
        }
        ClockChessState {
            visit_id: self.id,
            match_number: current.map_or(0, |current| current.number),
            phase: self.phase,
            phase_tick: self.ticks,
            actions: current.map_or(0, |current| current.game.revision().actions),
            board_milli: [
                milli(board.min.x),
                milli(board.min.y),
                milli(board.max.x),
                milli(board.max.y),
            ],
            actors,
        }
    }
}

/// The squares of `position` whose pieces are not out yet when `out` of
/// them are, from none at 0 to all at 1. Each player's pieces come out from
/// its own back rank, and the two players' in turn.
fn hidden(position: &Position, out: f32) -> Squares {
    let order = |color: Color| {
        let mut squares: Vec<Square> = (0..8)
            .flat_map(|rank| (0..8).filter_map(move |file| Square::new(file, rank)))
            .filter(|&square| {
                position
                    .piece_at(square)
                    .is_some_and(|piece| piece.color == color)
            })
            .collect();
        // From the back rank outward, and the king first among equals.
        squares.sort_by_key(|&square| {
            let rank = match color {
                Color::White => square.rank(),
                Color::Black => 7 - square.rank(),
            };
            let king = position
                .piece_at(square)
                .is_some_and(|piece| piece.kind == PieceKind::King);
            (rank, !king, square.file())
        });
        squares
    };
    let (white, black) = (order(Color::White), order(Color::Black));
    let mut turns = Vec::with_capacity(white.len() + black.len());
    for index in 0..white.len().max(black.len()) {
        turns.extend(white.get(index));
        turns.extend(black.get(index));
    }
    let shown = (turns.len() as f32 * out.clamp(0.0, 1.0)).ceil() as usize;
    turns[shown.min(turns.len())..].iter().copied().collect()
}
