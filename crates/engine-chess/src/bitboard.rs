//! Sets of squares as 64-bit words: one bit per square, a1 lowest.
//!
//! The attacks of knights, kings and pawns are found by shifting whole sets.
//! A sliding piece's are read from tables of the squares that lie each way
//! from each square, and cut off at the first piece in the way.

use crate::board::{Color, Square};

pub(crate) const FILE_A: u64 = 0x0101_0101_0101_0101;
pub(crate) const FILE_H: u64 = FILE_A << 7;
pub(crate) const RANK_1: u64 = 0xff;
pub(crate) const RANK_2: u64 = RANK_1 << 8;
pub(crate) const RANK_7: u64 = RANK_1 << 48;
pub(crate) const RANK_8: u64 = RANK_1 << 56;
pub(crate) const LIGHT_SQUARES: u64 = 0x55aa_55aa_55aa_55aa;

pub(crate) fn north(set: u64) -> u64 {
    set << 8
}

pub(crate) fn south(set: u64) -> u64 {
    set >> 8
}

// A step east or west must not wrap round the edge of the board.
fn east(set: u64) -> u64 {
    (set << 1) & !FILE_A
}

fn west(set: u64) -> u64 {
    (set >> 1) & !FILE_H
}

fn north_east(set: u64) -> u64 {
    (set << 9) & !FILE_A
}

fn north_west(set: u64) -> u64 {
    (set << 7) & !FILE_H
}

fn south_east(set: u64) -> u64 {
    (set >> 7) & !FILE_A
}

fn south_west(set: u64) -> u64 {
    (set >> 9) & !FILE_H
}

/// The squares that lie one way from each square, as far as the edge of the
/// board. `files` and `ranks` give the way as a step in each.
const fn rays(files: i8, ranks: i8) -> [u64; 64] {
    let mut table = [0; 64];
    let mut square = 0;
    while square < 64 {
        let mut file = (square % 8) as i8 + files;
        let mut rank = (square / 8) as i8 + ranks;
        while 0 <= file && file < 8 && 0 <= rank && rank < 8 {
            table[square] |= 1 << (rank * 8 + file);
            file += files;
            rank += ranks;
        }
        square += 1;
    }
    table
}

static NORTH: [u64; 64] = rays(0, 1);
static SOUTH: [u64; 64] = rays(0, -1);
static EAST: [u64; 64] = rays(1, 0);
static WEST: [u64; 64] = rays(-1, 0);
static NORTH_EAST: [u64; 64] = rays(1, 1);
static NORTH_WEST: [u64; 64] = rays(-1, 1);
static SOUTH_EAST: [u64; 64] = rays(1, -1);
static SOUTH_WEST: [u64; 64] = rays(-1, -1);

/// The union of four tables.
const fn joined(tables: [&[u64; 64]; 4]) -> [u64; 64] {
    let mut joined = [0; 64];
    let mut square = 0;
    while square < 64 {
        joined[square] =
            tables[0][square] | tables[1][square] | tables[2][square] | tables[3][square];
        square += 1;
    }
    joined
}

static STRAIGHTS: [u64; 64] = joined([&NORTH, &SOUTH, &EAST, &WEST]);
static DIAGONALS: [u64; 64] = joined([&NORTH_EAST, &NORTH_WEST, &SOUTH_EAST, &SOUTH_WEST]);

/// The other squares of the rank and file of `square`: where a rook would
/// have to stand to attack it, whatever stood between.
pub(crate) fn straights(square: Square) -> u64 {
    STRAIGHTS[square.index()]
}

/// The other squares of the two diagonals of `square`.
pub(crate) fn diagonals(square: Square) -> u64 {
    DIAGONALS[square.index()]
}

/// The squares that a piece on `square` reaches along `ray`, which must run
/// towards higher squares: every empty square in its path and the first
/// occupied one.
fn reach_up(ray: &[u64; 64], square: Square, occupied: u64) -> u64 {
    let path = ray[square.index()];
    // The nearest piece in the path hides what lies beyond it. With no piece
    // there, h8 stands in for one: no ray of this kind leaves it.
    let nearest = ((path & occupied) | 1 << 63).trailing_zeros();
    path ^ ray[nearest as usize]
}

/// The same along a ray that runs towards lower squares, where a1 stands in.
fn reach_down(ray: &[u64; 64], square: Square, occupied: u64) -> u64 {
    let path = ray[square.index()];
    let nearest = 63 - ((path & occupied) | 1).leading_zeros();
    path ^ ray[nearest as usize]
}

/// The squares a rook on `square` attacks with the pieces of `occupied` on
/// the board.
pub(crate) fn rook_attacks(square: Square, occupied: u64) -> u64 {
    reach_up(&NORTH, square, occupied)
        | reach_up(&EAST, square, occupied)
        | reach_down(&SOUTH, square, occupied)
        | reach_down(&WEST, square, occupied)
}

/// The squares a bishop on `square` attacks with the pieces of `occupied` on
/// the board.
pub(crate) fn bishop_attacks(square: Square, occupied: u64) -> u64 {
    reach_up(&NORTH_EAST, square, occupied)
        | reach_up(&NORTH_WEST, square, occupied)
        | reach_down(&SOUTH_EAST, square, occupied)
        | reach_down(&SOUTH_WEST, square, occupied)
}

pub(crate) fn knight_attacks(knights: u64) -> u64 {
    let one_aside = east(knights) | west(knights);
    let two_aside = east(east(knights)) | west(west(knights));
    north(north(one_aside)) | south(south(one_aside)) | north(two_aside) | south(two_aside)
}

pub(crate) fn king_attacks(kings: u64) -> u64 {
    let beside = east(kings) | west(kings);
    let row = beside | kings;
    beside | north(row) | south(row)
}

/// The squares that pawns of `color` attack.
pub(crate) fn pawn_attacks(pawns: u64, color: Color) -> u64 {
    match color {
        Color::White => north_east(pawns) | north_west(pawns),
        Color::Black => south_east(pawns) | south_west(pawns),
    }
}

/// The squares of a set, lowest first.
pub(crate) struct Squares(pub(crate) u64);

impl Iterator for Squares {
    type Item = Square;

    fn next(&mut self) -> Option<Square> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Some(Square::from_index(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The squares that `pieces` reach by sliding one way, found a step at a
    /// time: every empty square in their path and the first occupied one.
    fn slide(pieces: u64, occupied: u64, step: impl Fn(u64) -> u64) -> u64 {
        let mut reached = 0;
        let mut front = step(pieces);
        while front != 0 {
            reached |= front;
            front = step(front & !occupied);
        }
        reached
    }

    #[test]
    fn a_sliding_piece_reaches_what_it_would_a_step_at_a_time() {
        // Boards from empty to full, by a fixed sequence of numbers.
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut boards = vec![0, u64::MAX];
        for _ in 0..500 {
            let (one, two, three) = (next(), next(), next());
            boards.extend([one, one & two, one & two & three, one | two]);
        }

        for index in 0..64 {
            let square = Square::from_index(index);
            let piece = square.bit();
            for &occupied in &boards {
                let rook = slide(piece, occupied, north)
                    | slide(piece, occupied, south)
                    | slide(piece, occupied, east)
                    | slide(piece, occupied, west);
                assert_eq!(
                    rook_attacks(square, occupied),
                    rook,
                    "{square} {occupied:#x}"
                );
                let bishop = slide(piece, occupied, north_east)
                    | slide(piece, occupied, north_west)
                    | slide(piece, occupied, south_east)
                    | slide(piece, occupied, south_west);
                assert_eq!(
                    bishop_attacks(square, occupied),
                    bishop,
                    "{square} {occupied:#x}"
                );
            }
        }
    }

    #[test]
    fn on_an_empty_board_a_rook_reaches_fourteen_squares_from_anywhere() {
        for index in 0..64 {
            let square = Square::from_index(index);
            assert_eq!(rook_attacks(square, 0).count_ones(), 14, "{square}");
            assert_eq!(rook_attacks(square, 0) & square.bit(), 0, "{square}");
            // A bishop reaches seven from a corner and thirteen from the
            // middle.
            let bishop = bishop_attacks(square, 0).count_ones();
            assert!((7..=13).contains(&bishop), "{square}: {bishop}");
        }
        let corner = Square::from_index(0);
        assert_eq!(bishop_attacks(corner, 0), 0x8040_2010_0804_0200);
    }

    #[test]
    fn the_lines_through_a_square_are_what_a_rook_and_a_bishop_reach_on_an_empty_board() {
        for index in 0..64 {
            let square = Square::from_index(index);
            assert_eq!(straights(square), rook_attacks(square, 0), "{square}");
            assert_eq!(diagonals(square), bishop_attacks(square, 0), "{square}");
        }
    }
}
