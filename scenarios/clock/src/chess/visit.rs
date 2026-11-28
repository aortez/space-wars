//! One visit to the board, from the packed case's arrival to its departure.
//!
//! A [`Visit`] is admitted with its [`Stage`] found and the arena's claim
//! granted. Its board comes packed in its case, opens out, and the pieces of
//! the match in hand come out onto their squares, each player's from its own
//! back rank outward. Then the birds play, as [`Play`] paces them, until the
//! host asks the visit to pack up. No action is searched for or taken after
//! that: the pieces go back, the board folds into its case, and the case
//! goes. The match keeps every action its game accepted.
//!
//! What the board shows always comes from the match's game as it stands, so
//! the pieces set out, those in play and a move in motion are of one
//! revision. Packed, it shows none.
//!
//! In the arena the board is one fixed body in the chess board's identity,
//! shaped as it is drawn: the case while it is packed, and the board's
//! square from the moment it opens until it has folded up again. The case
//! opens only while the board's place is still clear of every other body.
//! Where something has come to stand there since the stage was found, the
//! case goes as it came.

use engine_chess::{Action, Color, PieceKind, Position, Square};
use engine_chess_board::{Board, Presentation, Squares, Transition};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld,
};

use super::stage::{Area, Side, Stage};
use super::{Bird, Event, Exhibition, Match, Pace, Play};
use crate::arena::ids::canonical;

#[cfg(test)]
mod tests;

/// What a visit is doing with its board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The board comes, packed in its case.
    Arriving,
    /// The case opens out into the board.
    Opening,
    /// The pieces come out onto their squares.
    Placing,
    /// The birds play.
    Playing,
    /// The pieces go back.
    Clearing,
    /// The board folds back into its case.
    Closing,
    /// The board goes, packed.
    Leaving,
}

impl Phase {
    /// How many ticks the phase lasts. Play lasts until the visit packs up.
    pub const fn ticks(self) -> Option<u64> {
        match self {
            Self::Arriving | Self::Leaving => Some(30),
            Self::Opening | Self::Closing => Some(45),
            Self::Placing => Some(120),
            Self::Playing => None,
            Self::Clearing => Some(90),
        }
    }

    const fn next(self) -> Option<Self> {
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
    pub const fn packed(self) -> bool {
        matches!(self, Self::Arriving | Self::Leaving)
    }
}

/// How long a move takes to travel to its square, in ticks.
pub const MOVE_TICKS: u64 = 30;

/// What a visit's board shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Show {
    /// The case, packed.
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
    /// The host asked the visit to pack up.
    packing: bool,
    /// The board's shape in the arena, as last driven.
    shape: Option<Shape>,
}

impl Visit {
    /// The visit admitted to `current`, the match in hand, on `stage`. The
    /// arena's claim was granted, and the packed case goes into `world`.
    pub fn new(stage: Stage, current: &Match, world: &mut PhysicsWorld) -> Self {
        let white = match current.bird(Color::White) {
            Bird::Duck => stage.duck,
            Bird::Crow => stage.duck.other(),
        };
        let mut visit = Self {
            stage,
            session: current.seed,
            white,
            phase: Phase::Arriving,
            ticks: 0,
            play: None,
            moving: None,
            packing: false,
            shape: None,
        };
        visit.drive(world);
        visit
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

    /// The side the bird playing `color` sits at.
    pub fn side(&self, color: Color) -> Side {
        match color {
            Color::White => self.white,
            Color::Black => self.white.other(),
        }
    }

    /// Whether the board has gone, packed, and the visit with it.
    pub fn done(&self) -> bool {
        self.phase == Phase::Leaving && Some(self.ticks) >= Phase::Leaving.ticks()
    }

    /// Asks the visit to pack up: no action is searched for or taken from
    /// now on. A move on its way still arrives, and a board still being set
    /// up is put away from where it got to. False if it was asked already.
    pub fn pack(&mut self) -> bool {
        if self.packing {
            return false;
        }
        self.packing = true;
        self.play = None;
        let undo = match self.phase {
            Phase::Arriving => Phase::Leaving,
            Phase::Opening => Phase::Closing,
            Phase::Placing => Phase::Clearing,
            _ => return true,
        };
        let (done, total) = (self.fraction(), undo.ticks().expect("a timed phase"));
        self.phase = undo;
        self.ticks = total - (done * total as f32).round() as u64;
        true
    }

    /// How far through its phase the visit is, from 0 to 1. Play is always
    /// at its start.
    fn fraction(&self) -> f32 {
        self.phase.ticks().map_or(0.0, |total| {
            (self.ticks as f32 / total as f32).clamp(0.0, 1.0)
        })
    }

    /// Gives the board in `world` the shape it has now, and opens the case
    /// once it has arrived, if the board's place is still clear. The host
    /// calls this before the arena's step.
    pub fn drive(&mut self, world: &mut PhysicsWorld) {
        if self.phase == Phase::Arriving && Some(self.ticks) >= Phase::Arriving.ticks() {
            let own = [PhysicsId::new(canonical::CHESS_BOARD.start)];
            if self.stage.board.is_clear_in(world, &own) {
                self.enter(Phase::Opening);
            } else {
                self.packing = true;
                self.enter(Phase::Leaving);
            }
        }
        let shape = if self.phase.packed() {
            Shape::Case
        } else {
            Shape::Board
        };
        if self.shape == Some(shape) {
            return;
        }
        let entity = PhysicsId::new(canonical::CHESS_BOARD.start);
        if self.shape.is_some() {
            assert!(world.remove_entity(entity), "the board is in the arena");
        }
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
        self.shape = Some(shape);
    }

    /// Takes one tick of the visit, which plays a tick at the board while
    /// the birds play there, and returns what they did in it. The host calls
    /// this for each tick that moves time, after the arena's step.
    pub fn tick(&mut self, exhibition: &mut Exhibition) -> Option<Event> {
        let mut event = None;
        if let Some(moving) = &mut self.moving {
            *moving += 1;
            if *moving >= MOVE_TICKS {
                self.moving = None;
            }
        }
        if self.phase == Phase::Playing {
            self.ticks += 1;
            if let Some(play) = &mut self.play {
                event = play.tick(exhibition);
                if let Some(Event::Act { action, .. }) = event {
                    self.moving = matches!(action, Action::Move(_)).then_some(0);
                }
            }
            if self.packing && self.moving.is_none() {
                self.enter(Phase::Clearing);
            }
            return event;
        }
        self.ticks += 1;
        // The case opens in `drive`, which can ask the arena about its place.
        if self.phase != Phase::Arriving
            && Some(self.ticks) >= self.phase.ticks()
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
        }
        event
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.ticks = 0;
    }

    /// What the board shows now, of `exhibition`'s match in hand.
    pub fn show(&self, exhibition: &Exhibition) -> Show {
        let game = exhibition
            .current
            .as_ref()
            .filter(|current| current.seed == self.session)
            .map(|current| &current.game);
        match self.phase {
            Phase::Arriving | Phase::Leaving => return Show::Case(self.stage.case()),
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
