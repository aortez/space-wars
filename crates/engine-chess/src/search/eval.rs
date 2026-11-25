//! How good a position looks without looking ahead.
//!
//! The piece values and tables are those of `aortez/cuda-chess`, with two
//! corrections that `REFERENCE.md` describes.

use crate::bitboard::Squares;
use crate::board::{Color, PieceKind, Square};
use crate::position::Position;

/// What each kind of piece is worth in centipawns. The king is never
/// captured, so it has no value.
pub(super) const VALUES: [i32; 6] = [100, 330, 350, 520, 980, 0];

/// A bonus for each square, read from the side of the piece's owner: the
/// first row is the owner's back rank, and each row runs from the a-file to
/// the h-file.
type Table = [i8; 64];

#[rustfmt::skip]
const PAWN: Table = [
     0,  0,  0,   0,   0,  0,  0,  0,
    -6, -4,  1, -24, -24,  1, -4, -6,
    -4, -4,  1,   5,   5,  1, -4, -4,
    -6, -4,  5,  10,  10,  5, -4, -6,
    -6, -4,  2,   8,   8,  2, -4, -6,
    -6, -4,  1,   2,   2,  1, -4, -6,
    -6, -4,  1,   1,   1,  1, -4, -6,
     0,  0,  0,   0,   0,  0,  0,  0,
];

#[rustfmt::skip]
const KNIGHT: Table = [
    -8, -12, -8, -8, -8, -8, -12, -8,
    -8,   0,  1,  2,  2,  1,   0, -8,
    -8,   0,  4,  4,  4,  4,   0, -8,
    -8,   0,  4,  8,  8,  4,   0, -8,
    -8,   0,  4,  8,  8,  4,   0, -8,
    -8,   0,  4,  4,  4,  4,   0, -8,
    -8,   0,  0,  0,  0,  0,   0, -8,
    -8,  -8, -8, -8, -8, -8,  -8, -8,
];

#[rustfmt::skip]
const BISHOP: Table = [
    -4, -4, -12, -4, -4, -12, -4, -4,
    -4,  2,   1,  1,  1,   1,  2, -4,
    -4,  1,   2,  4,  4,   2,  1, -4,
    -4,  0,   4,  6,  6,   4,  0, -4,
    -4,  0,   4,  6,  6,   4,  0, -4,
    -4,  0,   2,  4,  4,   2,  0, -4,
    -4,  0,   0,  0,  0,   0,  0, -4,
    -4, -4,  -4, -4, -4,  -4, -4, -4,
];

#[rustfmt::skip]
const ROOK: Table = [
     0, 0, 0, 2, 2, 0, 0,  0,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
     0, 0, 0, 0, 0, 0, 0,  0,
];

#[rustfmt::skip]
const QUEEN: Table = [
    -5, -5, -5, -5, -5, -5, -5, -5,
     0,  0,  1,  1,  1,  1,  0,  0,
     0,  0,  1,  2,  2,  1,  0,  0,
     0,  0,  2,  3,  3,  2,  0,  0,
     0,  0,  2,  3,  3,  2,  0,  0,
     0,  0,  1,  2,  2,  1,  0,  0,
     0,  0,  1,  1,  1,  1,  0,  0,
     0,  0,  0,  0,  0,  0,  0,  0,
];

/// The king while the board is full: castled, behind its pawns.
#[rustfmt::skip]
const KING_OPENING: Table = [
      0,  20,  30, -30,   0, -20,  30,  20,
    -15, -15, -20, -20, -20, -20, -15, -15,
    -40, -40, -40, -40, -40, -40, -40, -40,
    -40, -40, -40, -40, -40, -40, -40, -40,
    -40, -40, -40, -40, -40, -40, -40, -40,
    -40, -40, -40, -40, -40, -40, -40, -40,
    -40, -40, -40, -40, -40, -40, -40, -40,
    -40, -40, -40, -40, -40, -40, -40, -40,
];

/// The king once few pieces are left: towards the centre.
#[rustfmt::skip]
const KING_ENDGAME: Table = [
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 1, 1, 1, 1, 0, 0,
    0, 0, 1, 2, 2, 1, 0, 0,
    0, 0, 1, 2, 2, 1, 0, 0,
    0, 0, 1, 1, 1, 1, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
];

const PIECES: [(PieceKind, &Table); 5] = [
    (PieceKind::Pawn, &PAWN),
    (PieceKind::Knight, &KNIGHT),
    (PieceKind::Bishop, &BISHOP),
    (PieceKind::Rook, &ROOK),
    (PieceKind::Queen, &QUEEN),
];

/// The board counts as full with this many pieces on it and as empty of
/// everything but an endgame with [`ENDGAME_PIECES`]. In between, the two
/// king tables are blended.
const OPENING_PIECES: i32 = 32;
const ENDGAME_PIECES: i32 = 12;

fn index(color: Color, square: Square) -> usize {
    let rank = match color {
        Color::White => square.rank(),
        Color::Black => 7 - square.rank(),
    };
    usize::from(rank) * 8 + usize::from(square.file())
}

/// The value of `position` in centipawns for the player to move: material,
/// plus where each piece stands.
pub(crate) fn evaluate(position: &Position) -> i32 {
    let span = OPENING_PIECES - ENDGAME_PIECES;
    let pieces = position.occupied().count_ones() as i32;
    let opening = (pieces - ENDGAME_PIECES).clamp(0, span);

    let mut score = 0;
    for color in [Color::White, Color::Black] {
        let mut side = 0;
        for (kind, table) in PIECES {
            let set = position.set(color, kind);
            side += VALUES[kind.index()] * set.count_ones() as i32;
            for square in Squares(set) {
                side += i32::from(table[index(color, square)]);
            }
        }
        let king = index(color, position.king(color));
        let early = i32::from(KING_OPENING[king]);
        let late = i32::from(KING_ENDGAME[king]);
        side += (early * opening + late * (span - opening)) / span;

        if color == position.side_to_move() {
            score += side;
        } else {
            score -= side;
        }
    }
    score
}
