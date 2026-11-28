//! How good a position looks without looking ahead.
//!
//! The piece values, the tables and the terms for pawns and rooks are those
//! of `aortez/cuda-chess`, with the differences that `REFERENCE.md`
//! describes. The term for a lone king is the mop-up evaluation of Chess 4.5,
//! weighted more heavily, as `REFERENCE.md` explains.

use crate::bitboard::{FILE_A, FILE_H, RANK_7, RANK_8, Squares};
use crate::board::{Color, Piece, PieceKind, Square};
use crate::position::Position;

use super::MATE_BOUND;

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

/// The tables of the pieces other than the king, in the order of
/// [`PieceKind::ALL`].
const TABLES: [&Table; 5] = [&PAWN, &KNIGHT, &BISHOP, &ROOK, &QUEEN];

/// What a passed pawn is worth beyond its placement, by the rank it stands
/// on, counted from its owner's back rank.
const PASSED_PAWN: [i32; 8] = [0, 0, 3, 20, 40, 60, 70, 90];
/// What a pawn costs that stands behind another of its own on a file.
const DOUBLED_PAWN: i32 = -11;
/// What a rook is worth on a file with no pawn of either player.
const ROOK_ON_OPEN_FILE: i32 = 9;
/// What a rook is worth on the rank where the other player's pawns start,
/// while there is a pawn or a king for it to hem in.
const ROOK_ON_SEVENTH: i32 = 17;

/// Against a lone king, what each step it stands from the centre is worth to
/// the player with a queen or a rook, counted along files and ranks.
///
/// A step is worth more than the margin of Easy or Normal, so that neither
/// chooses a move that lets the lone king back towards the centre. Worth a
/// tenth as much, it was lost in Normal's margin and most of the endings
/// were drawn. Worth a fifth, nearly half of those with a rook still were.
/// See `REFERENCE.md`.
const LONE_KING_FROM_CENTRE: i32 = 100;
/// And what each step closer the other king stands to it is worth, counted
/// the same way.
const KING_NEARER_LONE_KING: i32 = 40;

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

/// What `piece` is worth on `square` to its owner: its value and the bonus
/// for the square. The king's part depends on the rest of the board and is
/// counted apart, so a king is worth nothing here.
fn worth(piece: Piece, square: Square) -> i32 {
    match TABLES.get(piece.kind.index()) {
        Some(table) => VALUES[piece.kind.index()] + i32::from(table[index(piece.color, square)]),
        None => 0,
    }
}

/// How the [`placement`] of a position changes for White when `piece` is put
/// on `square`, or taken off it if `put` is false.
pub(super) fn placement_change(piece: Piece, (square, put): (Square, bool)) -> i32 {
    let gained = (piece.color == Color::White) == put;
    let worth = worth(piece, square);
    if gained { worth } else { -worth }
}

/// What the pieces of `position` are worth where they stand, White's less
/// Black's, leaving out the kings.
///
/// The search keeps this up from one position to the next with
/// [`placement_change`], which is quicker than counting every piece again.
pub(super) fn placement(position: &Position) -> i32 {
    let mut placement = 0;
    for (color, sign) in [(Color::White, 1), (Color::Black, -1)] {
        for kind in PieceKind::ALL {
            for square in Squares(position.set(color, kind)) {
                placement += sign * worth(Piece { color, kind }, square);
            }
        }
    }
    placement
}

/// The value of `position` in centipawns for the player to move: material,
/// plus where each piece stands. The search does not count the pieces again
/// at every position, so only the tests work a value out from nothing.
#[cfg(test)]
pub(crate) fn evaluate(position: &Position) -> i32 {
    evaluate_placed(position, placement(position))
}

/// The value of `position` in centipawns for the player to move, given its
/// [`placement`].
pub(super) fn evaluate_placed(position: &Position, placement: i32) -> i32 {
    debug_assert_eq!(placement, self::placement(position), "{position}");
    let span = OPENING_PIECES - ENDGAME_PIECES;
    let pieces = position.occupied().count_ones() as i32;
    let opening = (pieces - ENDGAME_PIECES).clamp(0, span);
    let king = |color| {
        let king = index(color, position.king(color));
        let early = i32::from(KING_OPENING[king]);
        let late = i32::from(KING_ENDGAME[king]);
        (early * opening + late * (span - opening)) / span
    };

    let for_white = placement + king(Color::White) - king(Color::Black)
        + structure(position)
        + lone_king(position);
    // No game reaches material worth half of this bound, but a position set
    // up by hand may hold sixty queens. However it looks, it looks like less
    // than a forced mate.
    let for_white = for_white.clamp(1 - MATE_BOUND, MATE_BOUND - 1);
    match position.side_to_move() {
        Color::White => for_white,
        Color::Black => -for_white,
    }
}

/// The pieces of one player that are judged by more than where they stand,
/// with that player's king.
struct Side {
    pawns: u64,
    rooks: u64,
    king: u64,
}

impl Side {
    fn of(position: &Position, color: Color) -> Self {
        Self {
            pawns: position.set(color, PieceKind::Pawn),
            rooks: position.set(color, PieceKind::Rook),
            king: position.set(color, PieceKind::King),
        }
    }

    /// The same pieces on a board turned top to bottom, where Black's pawns
    /// move up it as White's do.
    fn turned(&self) -> Self {
        Self {
            pawns: self.pawns.swap_bytes(),
            rooks: self.rooks.swap_bytes(),
            king: self.king.swap_bytes(),
        }
    }
}

/// Every square further up the board than a square of `set`, on its file.
fn above(set: u64) -> u64 {
    let mut fill = set << 8;
    fill |= fill << 8;
    fill |= fill << 16;
    fill |= fill << 32;
    fill
}

/// Every square further down the board than a square of `set`, on its file.
fn below(set: u64) -> u64 {
    let mut fill = set >> 8;
    fill |= fill >> 8;
    fill |= fill >> 16;
    fill |= fill >> 32;
    fill
}

/// What the pawns and rooks of `own` are worth beyond their placement,
/// against `their` pieces. Both stand on a board where the pawns of `own`
/// move up it.
fn standing(own: &Side, their: &Side) -> i32 {
    let mut score = 0;
    // A pawn is passed if no pawn of the other player stands ahead of it on
    // its file or on a file beside it. Of two on one file, only the one in
    // front can be.
    let beside = their.pawns | (their.pawns << 1) & !FILE_A | (their.pawns >> 1) & !FILE_H;
    let behind_own = below(own.pawns);
    let passed = own.pawns & !below(beside) & !behind_own;
    for square in Squares(passed) {
        score += PASSED_PAWN[usize::from(square.rank())];
    }
    score += DOUBLED_PAWN * (own.pawns & behind_own).count_ones() as i32;

    let pawns = own.pawns | their.pawns;
    let open = !(pawns | above(pawns) | below(pawns));
    score += ROOK_ON_OPEN_FILE * (own.rooks & open).count_ones() as i32;
    if their.king & RANK_8 != 0 || their.pawns & RANK_7 != 0 {
        score += ROOK_ON_SEVENTH * (own.rooks & RANK_7).count_ones() as i32;
    }
    score
}

/// What the pawns and rooks are worth beyond their placement, White's less
/// Black's.
fn structure(position: &Position) -> i32 {
    let white = Side::of(position, Color::White);
    let black = Side::of(position, Color::Black);
    standing(&white, &black) - standing(&black.turned(), &white.turned())
}

/// How far `square` stands from the four squares in the middle of the
/// board, counted along files and ranks: none on them, six in a corner.
fn from_centre(square: Square) -> i32 {
    let off = |line: u8| i32::from(line.max(7 - line)) - 4;
    off(square.file()) + off(square.rank())
}

/// How far apart two squares are, counted along files and ranks.
fn apart(one: Square, other: Square) -> i32 {
    i32::from(one.file().abs_diff(other.file()) + one.rank().abs_diff(other.rank()))
}

/// Where one player has only the king and the other a queen or a rook, what
/// the other player gains, White's less Black's, for driving the lone king
/// away from the centre and bringing their own king up to it.
///
/// The mate is won at the edge, and can be ten moves or more away, beyond
/// what a search can see. Without this a search finds nothing to choose
/// between the moves that lead there and those that do not.
pub(super) fn lone_king(position: &Position) -> i32 {
    for (strong, sign) in [(Color::White, 1), (Color::Black, -1)] {
        let weak = strong.opposite();
        if position.by_color[weak.index()] != position.set(weak, PieceKind::King) {
            continue;
        }
        if position.set(strong, PieceKind::Queen) | position.set(strong, PieceKind::Rook) == 0 {
            return 0;
        }
        let lone = position.king(weak);
        // The kings never stand side by side, so they are 2 to 14 apart.
        let nearer = 14 - apart(lone, position.king(strong));
        return sign * (LONE_KING_FROM_CENTRE * from_centre(lone) + KING_NEARER_LONE_KING * nearer);
    }
    0
}
