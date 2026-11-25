//! Reading and writing positions as Forsyth-Edwards Notation.

use std::fmt;

use crate::bitboard::{RANK_1, RANK_8};
use crate::board::{Color, PieceKind, Square};
use crate::position::{
    BLACK_KING_SIDE, BLACK_QUEEN_SIDE, Position, WHITE_KING_SIDE, WHITE_QUEEN_SIDE,
};

/// Why a FEN was refused. A FEN is refused when it is malformed and also when
/// it describes a position that no game could reach in the ways checked here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenError {
    /// The text does not have six fields.
    Fields,
    /// The placement is not eight ranks of eight squares.
    Placement,
    /// A side has no king, or more than one.
    Kings,
    /// A pawn stands on the first or the last rank.
    PawnOnBackRank,
    /// The player to move is neither `w` nor `b`.
    SideToMove,
    /// The castling field is malformed, or grants a right whose king or rook
    /// is not on its home square.
    Castling,
    /// The en passant field is malformed, or names a square that no pawn has
    /// just crossed.
    EnPassant,
    /// A counter is not a number, or the move number is zero.
    Counter,
    /// The player who has just moved is in check.
    OpponentInCheck,
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Fields => "a FEN has six fields",
            Self::Placement => "the placement is not eight ranks of eight squares",
            Self::Kings => "each side has exactly one king",
            Self::PawnOnBackRank => "a pawn stands on the first or the last rank",
            Self::SideToMove => "the player to move is w or b",
            Self::Castling => "a castling right needs its king and rook at home",
            Self::EnPassant => "no pawn has just crossed the en passant square",
            Self::Counter => "a counter is not a number, or the move number is zero",
            Self::OpponentInCheck => "the player who has just moved is in check",
        })
    }
}

impl std::error::Error for FenError {}

impl Position {
    /// Reads a position from a FEN with all six fields.
    ///
    /// An en passant square is kept only when a pawn may legally capture
    /// there, so a FEN that names one may read back without it.
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let fields: Vec<&str> = fen.split_whitespace().collect();
        let [placement, side, castling, en_passant, halfmove, fullmove] = fields[..] else {
            return Err(FenError::Fields);
        };

        let mut position = Self {
            by_color: [0; 2],
            by_kind: [0; 6],
            side_to_move: match side {
                "w" => Color::White,
                "b" => Color::Black,
                _ => return Err(FenError::SideToMove),
            },
            castling: 0,
            en_passant: None,
            halfmove_clock: halfmove.parse().map_err(|_| FenError::Counter)?,
            fullmove_number: fullmove.parse().map_err(|_| FenError::Counter)?,
            hash: 0,
        };
        if position.fullmove_number == 0 {
            return Err(FenError::Counter);
        }

        position.place(placement)?;
        for color in [Color::White, Color::Black] {
            if position.set(color, PieceKind::King).count_ones() != 1 {
                return Err(FenError::Kings);
            }
        }
        if position.by_kind[PieceKind::Pawn.index()] & (RANK_1 | RANK_8) != 0 {
            return Err(FenError::PawnOnBackRank);
        }
        let us = position.side_to_move;
        if position.is_attacked(position.king(us.opposite()), us) {
            return Err(FenError::OpponentInCheck);
        }

        position.grant_castling(castling)?;
        if en_passant != "-" {
            let square = Square::from_name(en_passant).ok_or(FenError::EnPassant)?;
            position.en_passant = position.crossed_square(square)?;
        }
        position.hash = position.computed_hash();
        Ok(position)
    }

    fn place(&mut self, placement: &str) -> Result<(), FenError> {
        let ranks: Vec<&str> = placement.split('/').collect();
        if ranks.len() != 8 {
            return Err(FenError::Placement);
        }
        for (rank, text) in (0..8).rev().zip(ranks) {
            let mut file = 0;
            let mut after_digit = false;
            for symbol in text.chars() {
                if let Some(empty) = symbol.to_digit(10) {
                    // Two counts in a row would hide a miscount.
                    if after_digit || !(1..=8).contains(&empty) {
                        return Err(FenError::Placement);
                    }
                    file += empty as u8;
                    after_digit = true;
                    continue;
                }
                let kind = PieceKind::from_letter(symbol.to_ascii_lowercase())
                    .ok_or(FenError::Placement)?;
                let color = if symbol.is_ascii_uppercase() {
                    Color::White
                } else {
                    Color::Black
                };
                let square = Square::new(file, rank).ok_or(FenError::Placement)?;
                self.toggle(color, kind, square);
                file += 1;
                after_digit = false;
            }
            if file != 8 {
                return Err(FenError::Placement);
            }
        }
        Ok(())
    }

    fn grant_castling(&mut self, castling: &str) -> Result<(), FenError> {
        if castling == "-" {
            return Ok(());
        }
        for symbol in castling.chars() {
            let (right, color, rook_file) = match symbol {
                'K' => (WHITE_KING_SIDE, Color::White, 7),
                'Q' => (WHITE_QUEEN_SIDE, Color::White, 0),
                'k' => (BLACK_KING_SIDE, Color::Black, 7),
                'q' => (BLACK_QUEEN_SIDE, Color::Black, 0),
                _ => return Err(FenError::Castling),
            };
            let rank = if color == Color::White { 0 } else { 7 };
            let home = |file| Square::new(file, rank).expect("a file on the board").bit();
            let king_at_home = self.set(color, PieceKind::King) == home(4);
            let rook_at_home = self.set(color, PieceKind::Rook) & home(rook_file) != 0;
            if self.castling & right != 0 || !king_at_home || !rook_at_home {
                return Err(FenError::Castling);
            }
            self.castling |= right;
        }
        Ok(())
    }

    /// Checks that a pawn has just crossed `square`, and returns the square if
    /// it may be captured there.
    fn crossed_square(&self, square: Square) -> Result<Option<Square>, FenError> {
        let them = self.side_to_move.opposite();
        // The pawn left its home rank, crossed the square and stands beyond.
        let (crossed_rank, home_rank, pawn_rank) = match them {
            Color::White => (2, 1, 3),
            Color::Black => (5, 6, 4),
        };
        let on_file = |rank| {
            Square::new(square.file(), rank)
                .expect("a rank on the board")
                .bit()
        };
        let pawn_beyond = self.set(them, PieceKind::Pawn) & on_file(pawn_rank) != 0;
        let path_empty = self.occupied() & (on_file(crossed_rank) | on_file(home_rank)) == 0;
        if square.rank() != crossed_rank || !pawn_beyond || !path_empty {
            return Err(FenError::EnPassant);
        }
        Ok(self.capturable_en_passant(square))
    }
}

impl fmt::Display for Position {
    /// Writes the position as FEN.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                let square = Square::new(file, rank).expect("a square on the board");
                let Some(piece) = self.piece_at(square) else {
                    empty += 1;
                    continue;
                };
                if empty > 0 {
                    write!(f, "{empty}")?;
                    empty = 0;
                }
                let letter = piece.kind.letter();
                match piece.color {
                    Color::White => write!(f, "{}", letter.to_ascii_uppercase())?,
                    Color::Black => write!(f, "{letter}")?,
                }
            }
            if empty > 0 {
                write!(f, "{empty}")?;
            }
            if rank > 0 {
                f.write_str("/")?;
            }
        }

        f.write_str(match self.side_to_move {
            Color::White => " w ",
            Color::Black => " b ",
        })?;
        if self.castling == 0 {
            f.write_str("-")?;
        }
        for (right, symbol) in [
            (WHITE_KING_SIDE, "K"),
            (WHITE_QUEEN_SIDE, "Q"),
            (BLACK_KING_SIDE, "k"),
            (BLACK_QUEEN_SIDE, "q"),
        ] {
            if self.castling & right != 0 {
                f.write_str(symbol)?;
            }
        }
        match self.en_passant {
            Some(square) => write!(f, " {square}")?,
            None => f.write_str(" -")?,
        }
        write!(f, " {} {}", self.halfmove_clock, self.fullmove_number)
    }
}
