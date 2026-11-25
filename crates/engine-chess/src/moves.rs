//! Moves as the players state them.

use std::fmt;

use crate::board::{PieceKind, Square};

/// A move: the square a piece leaves, the square it reaches, and what a pawn
/// becomes on the last rank. Castling is the king's move of two squares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub promotion: Option<PieceKind>,
}

impl Move {
    /// Parses coordinate notation: `e2e4`, or `e7e8q` for a promotion.
    pub fn parse(text: &str) -> Option<Self> {
        if !text.is_ascii() || !(4..=5).contains(&text.len()) {
            return None;
        }
        let from = Square::from_name(&text[..2])?;
        let to = Square::from_name(&text[2..4])?;
        let promotion = match text[4..].chars().next() {
            None => None,
            Some(letter) => {
                let kind = PieceKind::from_letter(letter)?;
                if matches!(kind, PieceKind::Pawn | PieceKind::King) {
                    return None;
                }
                Some(kind)
            }
        };
        Some(Self {
            from,
            to,
            promotion,
        })
    }

    /// The move as one number, different for every move, which is quicker
    /// to compare than the move itself. No move has the number `u16::MAX`.
    pub(crate) fn code(self) -> u16 {
        let promotion = self.promotion.map_or(0, |kind| kind.index() + 1);
        (self.from.index() | self.to.index() << 6 | promotion << 12) as u16
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.from, self.to)?;
        if let Some(kind) = self.promotion {
            write!(f, "{}", kind.letter())?;
        }
        Ok(())
    }
}
