//! Sets of squares as 64-bit words: one bit per square, a1 lowest.
//!
//! Attacks are found by shifting whole sets, with no lookup tables. A sliding
//! piece's attacks are filled one step at a time until they meet a piece.

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

/// The squares that `pieces` reach by sliding one way: every empty square in
/// their path and the first occupied one.
fn slide(pieces: u64, occupied: u64, step: impl Fn(u64) -> u64) -> u64 {
    let mut reached = 0;
    let mut front = step(pieces);
    while front != 0 {
        reached |= front;
        front = step(front & !occupied);
    }
    reached
}

pub(crate) fn rook_attacks(rooks: u64, occupied: u64) -> u64 {
    slide(rooks, occupied, north)
        | slide(rooks, occupied, south)
        | slide(rooks, occupied, east)
        | slide(rooks, occupied, west)
}

pub(crate) fn bishop_attacks(bishops: u64, occupied: u64) -> u64 {
    slide(bishops, occupied, north_east)
        | slide(bishops, occupied, north_west)
        | slide(bishops, occupied, south_east)
        | slide(bishops, occupied, south_west)
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
