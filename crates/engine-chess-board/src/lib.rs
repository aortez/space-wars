//! Draws chess positions for the scenarios that show them: the squares, the
//! pieces and the marks that explain a game, as primitives of a
//! [`RenderFrame`](engine_common::RenderFrame).
//!
//! A host lays out a [`Board`] in its world and draws a [`Presentation`] of
//! its game on it each frame with [`draw`]. Drawing only adds to the frame:
//! it never plays a move or changes a game, and the same board and
//! presentation always give the same primitives. Input stays with the host,
//! which can ask the board which square lies under a point.
//!
//! The pieces are drawn in code from masks of cells, as filled polygons that
//! every renderer draws alike, so no font is needed. There are two sets:
//! full pieces sixteen cells high and compact ones ten cells high. A board
//! chooses between them by the size its squares would have on the shortest
//! screen the game runs on, the HyperPixel, 480 pixels tall:
//!
//! | A square on that screen | Pieces |
//! | --- | --- |
//! | 16 pixels or more | Full |
//! | 10 to 16 pixels | Compact |
//! | under 10 pixels | None: [`Board::fit`] reports the area too small |
//!
//! Rendered captures set these limits, and `docs/chess-board.md` shows them.
//! At 10 pixels a compact piece has a pixel for each cell, and every kind of
//! piece in either colour can still be told from the others in captures
//! through the CPU raster and the vector backends. An earlier set eight
//! cells high could not be read at 8 pixels. At 16 pixels full pieces have a
//! pixel for each cell and read better than compact ones.

#![forbid(unsafe_code)]

mod draw;
pub mod fixture;
mod mask;
mod pieces;
#[cfg(test)]
mod tests;

use std::fmt;

use engine_chess::{Color, Game, Move, Outcome, Position, Revision, Square};
use engine_common::{RenderPoint, RenderRect};

pub use draw::{DARK as DARK_SQUARE, FRAME_COLOR, LIGHT as LIGHT_SQUARE, MAX_PRIMITIVES, draw};

/// The height in pixels of the shortest screen the game runs on. A host
/// knows only how much of its world its camera shows, so a board judges its
/// squares by the share of that height they take, which on any taller screen
/// is more pixels.
const SHORTEST_SCREEN: f32 = 480.0;

/// The fewest pixels a side that a square needs on that screen for compact
/// pieces, and for full ones.
const COMPACT_PIXELS: f32 = 10.0;
const FULL_PIXELS: f32 = 16.0;

/// The width of the frame around the squares, in squares.
const FRAME: f32 = 0.25;

/// The side of a board, frame included, in squares.
const SIDE: f32 = 8.0 + 2.0 * FRAME;

/// The share of a board's side that its frame takes on each side.
pub const FRAME_SHARE: f32 = FRAME / SIDE;

/// Which way a board lies on the screen: which player's side is nearer its
/// bottom, or, turned a quarter for players seen sitting at its left and
/// right, which player's side is at its left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Orientation {
    #[default]
    WhiteAtBottom,
    BlackAtBottom,
    /// White's back rank runs up the left side, with a1 at its top, as White
    /// sees the board from the left.
    WhiteAtLeft,
    /// White's back rank runs up the right side, with a1 at its bottom, as
    /// White sees the board from the right.
    WhiteAtRight,
}

/// A side of a board's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

impl Orientation {
    /// The side of the frame nearer the pieces `color` starts with.
    pub(crate) fn edge(self, color: Color) -> Edge {
        let white = match self {
            Self::WhiteAtBottom => Edge::Bottom,
            Self::BlackAtBottom => Edge::Top,
            Self::WhiteAtLeft => Edge::Left,
            Self::WhiteAtRight => Edge::Right,
        };
        match (color, white) {
            (Color::White, edge) => edge,
            (Color::Black, Edge::Bottom) => Edge::Top,
            (Color::Black, Edge::Top) => Edge::Bottom,
            (Color::Black, Edge::Left) => Edge::Right,
            (Color::Black, Edge::Right) => Edge::Left,
        }
    }
}

/// How finely a board draws its pieces and marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Detail {
    /// Pieces sixteen cells high, and a dot on each empty square that a
    /// selected piece may reach.
    Full,
    /// Pieces ten cells high, and a tint on each square that a selected
    /// piece may reach.
    Compact,
}

/// A board's place in a host's world: where it stands, how large its squares
/// are and which way up it is. A frame a quarter of a square wide runs round
/// the squares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Board {
    /// The lower left corner of the frame.
    origin: RenderPoint,
    /// The side of a square, in world units.
    square: f32,
    orientation: Orientation,
    detail: Detail,
}

/// Why no board fits in an area: in that view it would be too small to read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TooSmall {
    /// The side, in world units, of the smallest board that the view could
    /// show, frame included. Infinite when the view's height is not a
    /// positive, finite number.
    pub needed: f32,
}

impl fmt::Display for TooSmall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a chess board needs a side of at least {}",
            self.needed
        )
    }
}

impl std::error::Error for TooSmall {}

impl Board {
    /// The largest board that fits in `area`, centred in it. `view_height`
    /// is the height of the world that the host's camera shows, its
    /// [`Camera2::height`](engine_common::Camera2::height). The share of
    /// that height a square takes decides the board's [`Detail`], or that no
    /// board would be legible.
    pub fn fit(
        area: RenderRect,
        view_height: f32,
        orientation: Orientation,
    ) -> Result<Self, TooSmall> {
        if !(view_height.is_finite() && view_height > 0.0) {
            return Err(TooSmall {
                needed: f32::INFINITY,
            });
        }
        let needed = |pixels: f32| view_height * pixels / SHORTEST_SCREEN * SIDE;
        let finite = [area.min.x, area.min.y, area.max.x, area.max.y]
            .iter()
            .all(|value| value.is_finite());
        let side = area.width().min(area.height());
        let detail = match side {
            _ if !finite => None,
            side if side >= needed(FULL_PIXELS) => Some(Detail::Full),
            side if side >= needed(COMPACT_PIXELS) => Some(Detail::Compact),
            _ => None,
        };
        let Some(detail) = detail else {
            return Err(TooSmall {
                needed: needed(COMPACT_PIXELS),
            });
        };
        Ok(Self {
            origin: RenderPoint::new(
                area.min.x + (area.width() - side) / 2.0,
                area.min.y + (area.height() - side) / 2.0,
            ),
            square: side / SIDE,
            orientation,
            detail,
        })
    }

    /// The area the board covers, frame included. Everything [`draw`] adds
    /// lies within it.
    pub fn bounds(&self) -> RenderRect {
        let side = self.square * SIDE;
        RenderRect {
            min: self.origin,
            max: RenderPoint::new(self.origin.x + side, self.origin.y + side),
        }
    }

    /// The side of a square, in world units.
    pub fn square_side(&self) -> f32 {
        self.square
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn detail(&self) -> Detail {
        self.detail
    }

    /// The lower left corner of the squares, inside the frame.
    fn corner(&self) -> RenderPoint {
        let frame = self.square * FRAME;
        RenderPoint::new(self.origin.x + frame, self.origin.y + frame)
    }

    /// Where `square` lies: its column from the left and its row from the
    /// bottom.
    fn place(&self, square: Square) -> (u8, u8) {
        let (file, rank) = (square.file(), square.rank());
        match self.orientation {
            Orientation::WhiteAtBottom => (file, rank),
            Orientation::BlackAtBottom => (7 - file, 7 - rank),
            Orientation::WhiteAtLeft => (rank, 7 - file),
            Orientation::WhiteAtRight => (7 - rank, file),
        }
    }

    /// The area that `square` covers.
    pub fn square_rect(&self, square: Square) -> RenderRect {
        let (column, row) = self.place(square);
        let corner = self.corner();
        let min = RenderPoint::new(
            corner.x + f32::from(column) * self.square,
            corner.y + f32::from(row) * self.square,
        );
        RenderRect {
            min,
            max: RenderPoint::new(min.x + self.square, min.y + self.square),
        }
    }

    /// The square under `point`, or `None` beyond the squares, on the frame
    /// as well. A point on the line between two squares belongs to the one
    /// to its right or above it on the screen.
    pub fn square_at(&self, point: RenderPoint) -> Option<Square> {
        let corner = self.corner();
        let column = ((point.x - corner.x) / self.square).floor();
        let row = ((point.y - corner.y) / self.square).floor();
        if !((0.0..8.0).contains(&column) && (0.0..8.0).contains(&row)) {
            return None;
        }
        let (column, row) = (column as u8, row as u8);
        match self.orientation {
            Orientation::WhiteAtBottom => Square::new(column, row),
            Orientation::BlackAtBottom => Square::new(7 - column, 7 - row),
            Orientation::WhiteAtLeft => Square::new(7 - row, column),
            Orientation::WhiteAtRight => Square::new(row, 7 - column),
        }
    }
}

/// A set of squares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Squares(u64);

impl Squares {
    pub const EMPTY: Self = Self(0);

    fn bit(square: Square) -> u64 {
        1 << (square.rank() * 8 + square.file())
    }

    pub fn insert(&mut self, square: Square) {
        self.0 |= Self::bit(square);
    }

    pub fn contains(self, square: Square) -> bool {
        self.0 & Self::bit(square) != 0
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl FromIterator<Square> for Squares {
    fn from_iter<I: IntoIterator<Item = Square>>(squares: I) -> Self {
        let mut set = Self::EMPTY;
        for square in squares {
            set.insert(square);
        }
        set
    }
}

/// A piece a player has picked up, and the squares where it may go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub square: Square,
    pub destinations: Squares,
}

impl Selection {
    /// The piece on `square` of the player to move in `game`, and the
    /// squares its legal moves reach.
    pub fn of(game: &Game, square: Square) -> Self {
        Self {
            square,
            destinations: game
                .legal_moves()
                .into_iter()
                .filter(|mv| mv.from == square)
                .map(|mv| mv.to)
                .collect(),
        }
    }
}

/// A move on its way across the board, for a host that shows moves in
/// motion. The host keeps the time: it says how far the move has gone, and
/// drops the transition when the move arrives.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// The position the move was played from.
    pub before: Position,
    pub mv: Move,
    /// The revision the move led to. A transition is drawn only in a
    /// presentation of that revision, whose position the move reaches from
    /// `before`. Otherwise the board is drawn as it stands.
    pub revision: Revision,
    /// How far the move has gone, from 0 as it starts to 1 as it arrives.
    pub progress: f32,
}

impl Transition {
    /// The last action of `game` in motion, if it was a move.
    pub fn of_last_move(game: &Game, progress: f32) -> Option<Self> {
        let revision = game.revision();
        let moves = game.moves();
        let positions = game.positions();
        // Only a claim or a resignation adds an action without a move, and
        // either ends the game.
        if moves.len() != revision.actions as usize {
            return None;
        }
        Some(Self {
            before: positions[positions.len().checked_sub(2)?].clone(),
            mv: *moves.last()?,
            revision,
            progress,
        })
    }
}

/// What a board shows: a position, and the marks that explain it.
#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub position: Position,
    /// The revision of the game whose position this is.
    pub revision: Revision,
    /// The move that reached the position, marked on the squares it left
    /// and reached.
    pub last_move: Option<Move>,
    pub selection: Option<Selection>,
    /// Whether to mark the frame on the side of the player to move, while
    /// the game is on.
    pub mark_turn: bool,
    /// How the game ended. A king that lost is tipped over, and after a draw
    /// both kings' squares are marked.
    pub outcome: Option<Outcome>,
    pub transition: Option<Transition>,
    /// Squares whose pieces are left off, as while a host sets the board up
    /// or packs it away. A piece in motion is left off with the square it
    /// left. The squares themselves and their marks are drawn as usual.
    pub hidden: Squares,
}

impl Presentation {
    /// `game` as it stands, its last move and the player to move marked.
    pub fn of(game: &Game) -> Self {
        Self {
            position: game.position().clone(),
            revision: game.revision(),
            last_move: game.moves().last().copied(),
            selection: None,
            mark_turn: true,
            outcome: game.outcome(),
            transition: None,
            hidden: Squares::EMPTY,
        }
    }

    /// The transition, if it belongs here: made for this revision, by a move
    /// that reaches this position from its own.
    fn motion(&self) -> Option<&Transition> {
        let transition = self.transition.as_ref()?;
        let arrives = transition.before.play(transition.mv);
        (transition.revision == self.revision
            && arrives.as_ref() == Some(&self.position)
            && transition.progress < 1.0)
            .then_some(transition)
    }
}
