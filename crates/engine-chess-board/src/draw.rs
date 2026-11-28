//! Turning a presentation into primitives.

use engine_chess::{Color, Outcome, Piece, PieceKind, Position, Square};
use engine_common::{
    RenderCircle, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive, RenderRect,
};

use crate::mask::Corner;
use crate::pieces;
use crate::{Board, Detail, Edge, FRAME, Presentation, Transition};

/// The frame's colour, and the light and the dark squares', for a host that
/// draws the board as it comes and goes in its own way.
pub const FRAME_COLOR: RenderColor = RenderColor::rgb(0.30, 0.21, 0.14);
pub const LIGHT: RenderColor = RenderColor::rgb(0.87, 0.80, 0.66);
pub const DARK: RenderColor = RenderColor::rgb(0.55, 0.42, 0.30);
const OUTLINE: RenderColor = RenderColor::rgb(0.05, 0.04, 0.04);
const TURN: RenderColor = RenderColor::rgb(0.96, 0.72, 0.22);

/// The colours of one side's pieces.
#[derive(Clone, Copy)]
struct Paint {
    outline: RenderColor,
    body: RenderColor,
    marks: RenderColor,
}

const WHITE_PIECES: Paint = Paint {
    outline: OUTLINE,
    body: RenderColor::rgb(0.98, 0.97, 0.93),
    marks: OUTLINE,
};

/// Both sides share the outline, which fills the gaps between a piece's
/// parts, such as a rook's battlements. A black body is a dark grey rather
/// than near black so that those gaps still show: near black, the outline
/// and body merged and compact black pieces lost their shapes.
const BLACK_PIECES: Paint = Paint {
    outline: OUTLINE,
    body: RenderColor::rgb(0.32, 0.30, 0.28),
    marks: RenderColor::rgb(0.80, 0.78, 0.74),
};

const SIDES: [(Color, Paint); 2] = [(Color::White, WHITE_PIECES), (Color::Black, BLACK_PIECES)];

/// The parts of a piece, which are drawn in this order.
#[derive(Clone, Copy)]
enum Part {
    Outline,
    Body,
    Marks,
}

/// A colour that tints a square, and how far it moves the square's colour
/// toward it.
type Tint = (RenderColor, f32);

const LAST_MOVE: Tint = (RenderColor::rgb(0.93, 0.86, 0.36), 0.5);
const SELECTED: Tint = (RenderColor::rgb(0.42, 0.74, 0.43), 0.6);
const REACHABLE: Tint = (RenderColor::rgb(0.42, 0.74, 0.43), 0.35);
const CHECK: Tint = (RenderColor::rgb(0.92, 0.25, 0.18), 0.6);
const LOST: Tint = (RenderColor::rgb(0.85, 0.12, 0.10), 0.8);
const DRAWN: Tint = (RenderColor::rgb(0.55, 0.64, 0.86), 0.55);

/// A dot is its square's colour moved this far toward the outline's.
const DOT_SHADE: f32 = 0.35;
/// The radius of a dot, in squares.
const DOT_RADIUS: f32 = 0.16;

/// The most polygons that draw one piece: its outline, its body and up to
/// three marks.
pub(crate) const PIECE_POLYGONS: usize = 5;

/// The most primitives [`draw`] adds: the frame, the mark of the turn, the
/// light squares as one, the 32 dark squares, a tint and a dot on every
/// square, and 32 pieces.
pub const MAX_PRIMITIVES: usize = 3 + 32 + 64 + 64 + 32 * PIECE_POLYGONS;

/// Draws `presentation` on `board` into `frame`, on layer `z`. Every
/// primitive is a filled polygon or circle and lies within
/// [`Board::bounds`]. No more than [`MAX_PRIMITIVES`] are added.
pub fn draw(frame: &mut RenderFrame, z: i32, board: &Board, presentation: &Presentation) {
    let motion = presentation.motion();
    // Until a move arrives, the board shows the position it left: whose
    // turn it was, and the check it answered. Its result waits too.
    let shown = motion.map_or(&presentation.position, |transition| &transition.before);
    let outcome = match motion {
        Some(_) => None,
        None => presentation.outcome,
    };
    let mut painter = Painter { frame, z, board };
    painter.rect(board.bounds(), FRAME_COLOR);
    if presentation.mark_turn && outcome.is_none() {
        painter.turn(shown.side_to_move());
    }
    let colors = square_colors(board, presentation, shown, outcome);
    painter.squares(&colors);
    if board.detail == Detail::Full
        && let Some(selection) = presentation.selection
    {
        for square in squares().filter(|&square| selection.destinations.contains(square)) {
            if shown.piece_at(square).is_none() {
                painter.dot(square, mix(colors[index(square)], (OUTLINE, DOT_SHADE)));
            }
        }
    }
    let (standing, moving) = placements(board, presentation, motion, shown, outcome);
    painter.pieces(&standing);
    for placed in moving {
        painter.pieces(&[placed]);
    }
}

/// A piece and where it is drawn: the lower left corner of the square it
/// fills, which for a piece in motion lies between two squares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    pub(crate) piece: Piece,
    pub(crate) at: RenderPoint,
    pub(crate) tipped: bool,
}

/// The pieces that stand on their squares, and those in motion, drawn over
/// them.
pub(crate) fn placements(
    board: &Board,
    presentation: &Presentation,
    motion: Option<&Transition>,
    shown: &Position,
    outcome: Option<Outcome>,
) -> (Vec<Placed>, Vec<Placed>) {
    let loser = match outcome {
        Some(Outcome::Checkmate { winner } | Outcome::Resignation { winner }) => {
            Some(winner.opposite())
        }
        _ => None,
    };
    let carried = motion.map_or_else(Vec::new, |transition| {
        carried(transition, &presentation.position)
    });
    let standing = squares()
        .filter(|&square| carried.iter().all(|&(_, from, _)| from != square))
        .filter(|&square| !presentation.hidden.contains(square))
        .filter_map(|square| {
            let piece = shown.piece_at(square)?;
            Some(Placed {
                piece,
                at: board.square_rect(square).min,
                tipped: piece.kind == PieceKind::King && Some(piece.color) == loser,
            })
        })
        .collect();
    let progress = motion.map_or(0.0, |transition| transition.progress.clamp(0.0, 1.0));
    let moving = carried
        .into_iter()
        .filter(|&(_, from, _)| !presentation.hidden.contains(from))
        .map(|(piece, from, to)| {
            let (from, to) = (board.square_rect(from).min, board.square_rect(to).min);
            Placed {
                piece,
                at: RenderPoint::new(
                    from.x + (to.x - from.x) * progress,
                    from.y + (to.y - from.y) * progress,
                ),
                tipped: false,
            }
        })
        .collect();
    (standing, moving)
}

/// The pieces a move carries, each with the squares it leaves and reaches:
/// the piece that moves, drawn as it was until it arrives, and the rook that
/// a castling king brings along. They are read from the positions before and
/// after the move, so they are what the rules did. A pawn taken en passant
/// leaves a square the move does not name, and stands there until the move
/// arrives, as a piece taken on the square the move reaches does.
fn carried(transition: &Transition, after: &Position) -> Vec<(Piece, Square, Square)> {
    let before = &transition.before;
    let mv = transition.mv;
    let Some(mover) = before.piece_at(mv.from) else {
        return Vec::new();
    };
    let mut carried = vec![(mover, mv.from, mv.to)];
    let others = squares().filter(|&square| square != mv.from && square != mv.to);
    let mut left = None;
    let mut reached = None;
    for square in others {
        match (before.piece_at(square), after.piece_at(square)) {
            (Some(piece), None) => left = Some((piece, square)),
            (None, Some(_)) => reached = Some(square),
            _ => {}
        }
    }
    if let (Some((rook, from)), Some(to)) = (left, reached) {
        carried.push((rook, from, to));
    }
    carried
}

/// The colour of each square, by index: light or dark, unless something
/// tints it. When several things would, the first of these wins: the end of
/// the game, a check, the selected square, a square it may reach and the last
/// move.
fn square_colors(
    board: &Board,
    presentation: &Presentation,
    shown: &Position,
    outcome: Option<Outcome>,
) -> [RenderColor; 64] {
    let mut tints: [Option<Tint>; 64] = [None; 64];
    let mut tint = |square: Square, color: Tint| {
        tints[index(square)].get_or_insert(color);
    };
    match outcome {
        Some(Outcome::Checkmate { winner } | Outcome::Resignation { winner }) => {
            tint(shown.king(winner.opposite()), LOST);
        }
        Some(Outcome::Draw(_)) => {
            tint(shown.king(Color::White), DRAWN);
            tint(shown.king(Color::Black), DRAWN);
        }
        None if shown.is_check() => tint(shown.king(shown.side_to_move()), CHECK),
        None => {}
    }
    if let Some(selection) = presentation.selection {
        tint(selection.square, SELECTED);
        for square in squares().filter(|&square| selection.destinations.contains(square)) {
            if board.detail == Detail::Compact || shown.piece_at(square).is_some() {
                tint(square, REACHABLE);
            }
        }
    }
    if let Some(mv) = presentation.last_move {
        tint(mv.from, LAST_MOVE);
        tint(mv.to, LAST_MOVE);
    }
    let mut colors = [LIGHT; 64];
    for square in squares() {
        let base = if is_dark(square) { DARK } else { LIGHT };
        colors[index(square)] = tints[index(square)].map_or(base, |tint| mix(base, tint));
    }
    colors
}

struct Painter<'a> {
    frame: &'a mut RenderFrame,
    z: i32,
    board: &'a Board,
}

impl Painter<'_> {
    fn push(&mut self, primitive: RenderPrimitive) {
        self.frame.push_primitive(self.z, primitive);
    }

    fn rect(&mut self, rect: RenderRect, color: RenderColor) {
        let RenderRect { min, max } = rect;
        self.push(RenderPrimitive::Polygon(RenderPolygon::filled(
            vec![
                min,
                RenderPoint::new(max.x, min.y),
                max,
                RenderPoint::new(min.x, max.y),
            ],
            color,
        )));
    }

    /// Lights the frame along the side of the player to move.
    fn turn(&mut self, side: Color) {
        let board = self.board;
        let RenderRect { min, max } = board.bounds();
        let frame = board.square * FRAME;
        let corner = board.corner();
        let squares = 8.0 * board.square;
        let (min, max) = match board.orientation.edge(side) {
            Edge::Bottom => (
                RenderPoint::new(corner.x, min.y),
                RenderPoint::new(corner.x + squares, min.y + frame),
            ),
            Edge::Top => (
                RenderPoint::new(corner.x, max.y - frame),
                RenderPoint::new(corner.x + squares, max.y),
            ),
            Edge::Left => (
                RenderPoint::new(min.x, corner.y),
                RenderPoint::new(min.x + frame, corner.y + squares),
            ),
            Edge::Right => (
                RenderPoint::new(max.x - frame, corner.y),
                RenderPoint::new(max.x, corner.y + squares),
            ),
        };
        self.rect(RenderRect { min, max }, TURN);
    }

    /// The light squares as one area, then each square of another colour,
    /// those of a colour together.
    fn squares(&mut self, colors: &[RenderColor; 64]) {
        let corner = self.board.corner();
        let side = 8.0 * self.board.square;
        let all = RenderRect {
            min: corner,
            max: RenderPoint::new(corner.x + side, corner.y + side),
        };
        self.rect(all, LIGHT);
        let mut drawn = [false; 64];
        for square in squares() {
            let color = colors[index(square)];
            if color == LIGHT || drawn[index(square)] {
                continue;
            }
            for other in squares().filter(|&other| colors[index(other)] == color) {
                drawn[index(other)] = true;
                let rect = self.board.square_rect(other);
                self.rect(rect, color);
            }
        }
    }

    fn dot(&mut self, square: Square, color: RenderColor) {
        let RenderRect { min, max } = self.board.square_rect(square);
        let center = RenderPoint::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);
        let radius = DOT_RADIUS * self.board.square;
        self.push(RenderPrimitive::Circle(RenderCircle::filled(
            center, radius, color,
        )));
    }

    /// Draws pieces in layers: the outlines, then the bodies, then the
    /// marks, each side's together within a layer, so that runs of one
    /// colour stay together.
    fn pieces(&mut self, placed: &[Placed]) {
        let detail = self.board.detail;
        let shaped: Vec<_> = placed
            .iter()
            .map(|placed| {
                let shape = pieces::shape(detail, placed.piece.kind, placed.tipped);
                (placed, shape)
            })
            .collect();
        for part in [Part::Outline, Part::Body, Part::Marks] {
            for (color, paint) in SIDES {
                for (placed, shape) in shaped.iter().filter(|(p, _)| p.piece.color == color) {
                    let (polygons, fill) = match part {
                        Part::Outline => (&shape.outline, paint.outline),
                        Part::Body => (&shape.body, paint.body),
                        Part::Marks => (&shape.marks, paint.marks),
                    };
                    self.polygons(polygons, shape.cells, placed.at, fill);
                }
            }
        }
    }

    /// Draws polygons whose corners are in cells from the top left of a
    /// square whose lower left corner is `at`.
    fn polygons(
        &mut self,
        polygons: &[Vec<Corner>],
        cells: u8,
        at: RenderPoint,
        color: RenderColor,
    ) {
        let cell = self.board.square / f32::from(cells);
        for polygon in polygons {
            let points = polygon
                .iter()
                .map(|&(x, y)| {
                    RenderPoint::new(
                        at.x + f32::from(x) * cell,
                        at.y + f32::from(cells - y) * cell,
                    )
                })
                .collect();
            self.push(RenderPrimitive::Polygon(RenderPolygon::filled(
                points, color,
            )));
        }
    }
}

/// Every square, from a1 along each rank to h8.
pub(crate) fn squares() -> impl Iterator<Item = Square> {
    (0..8).flat_map(|rank| (0..8).filter_map(move |file| Square::new(file, rank)))
}

pub(crate) fn index(square: Square) -> usize {
    usize::from(square.rank()) * 8 + usize::from(square.file())
}

/// Whether a square is dark, as a1 is.
fn is_dark(square: Square) -> bool {
    (square.file() + square.rank()).is_multiple_of(2)
}

fn mix(base: RenderColor, (toward, amount): Tint) -> RenderColor {
    let blend = |from: f32, to: f32| from + (to - from) * amount;
    RenderColor::rgb(
        blend(base.r, toward.r),
        blend(base.g, toward.g),
        blend(base.b, toward.b),
    )
}

#[cfg(test)]
mod tests;
