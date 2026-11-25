//! The positions the search is in the middle of, one frame for each
//! half-move below the root.

use crate::board::Square;
use crate::moves::Move;
use crate::position::{MoveSink, Position};

/// More moves than any position reached in a game allows: the most known is
/// 218.
const CAPACITY: usize = 256;

const NO_MOVE: Move = Move {
    from: Square::from_index(0),
    to: Square::from_index(0),
    promotion: None,
};

#[derive(Clone, Copy)]
struct Scored {
    mv: Move,
    /// Moves with higher scores are tried first.
    score: i16,
}

/// The moves of one position, in a fixed amount of memory.
#[derive(Clone)]
pub(super) struct MoveList {
    moves: [Scored; CAPACITY],
    len: usize,
    /// How many of the moves have been taken.
    taken: usize,
}

impl MoveList {
    fn new() -> Self {
        Self {
            moves: [Scored {
                mv: NO_MOVE,
                score: 0,
            }; CAPACITY],
            len: 0,
            taken: 0,
        }
    }

    pub(super) fn clear(&mut self) {
        self.len = 0;
        self.taken = 0;
    }

    /// Scores every move, dropping those that `score` gives no score.
    pub(super) fn score(&mut self, mut score: impl FnMut(Move) -> Option<i16>) {
        let mut kept = 0;
        for index in 0..self.len {
            let mv = self.moves[index].mv;
            if let Some(score) = score(mv) {
                self.moves[kept] = Scored { mv, score };
                kept += 1;
            }
        }
        self.len = kept;
    }

    /// Takes the move with the highest score of those not yet taken.
    pub(super) fn take_best(&mut self) -> Option<Move> {
        let rest = self.taken..self.len;
        let best = rest.clone().max_by_key(|&index| {
            // Among equal scores, the move generated first.
            (self.moves[index].score, std::cmp::Reverse(index))
        })?;
        // The moves passed over keep their order.
        self.moves[self.taken..=best].rotate_right(1);
        self.taken += 1;
        Some(self.moves[self.taken - 1].mv)
    }

    /// Drops the move taken last, which proved not to be legal.
    pub(super) fn discard_last(&mut self) {
        self.moves[self.taken - 1..self.len].rotate_left(1);
        self.taken -= 1;
        self.len -= 1;
    }

    /// The moves taken so far and not discarded, in the order they were
    /// taken.
    pub(super) fn tried(&self) -> impl Iterator<Item = Move> + '_ {
        self.moves[..self.taken].iter().map(|scored| scored.mv)
    }
}

impl MoveSink for MoveList {
    fn add(&mut self, mv: Move) {
        // A position set up with more moves than any game allows loses the
        // surplus: the search may then miss a move, but never invents one.
        if self.len < CAPACITY {
            self.moves[self.len] = Scored { mv, score: 0 };
            self.len += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    /// Pushed, and not yet looked at.
    Fresh,
    /// The player to move has passed, and the reply is being searched.
    Passing,
    /// Its moves are being tried.
    Open,
    /// Its value is known.
    Done(i32),
}

/// One position on the path from the root, and how far its search has got.
#[derive(Clone)]
pub(super) struct Frame {
    pub(super) position: Position,
    pub(super) moves: MoveList,
    pub(super) state: State,
    /// The window: a value of `alpha` or less, or of `beta` or more, need
    /// only be known as a bound.
    pub(super) alpha: i32,
    pub(super) beta: i32,
    /// `alpha` as it was when the frame was entered.
    pub(super) alpha_at_entry: i32,
    pub(super) best: i32,
    pub(super) best_move: Option<Move>,
    /// The move the table holds for the position, to try first.
    pub(super) table_move: Option<Move>,
    /// The move whose reply is being searched.
    pub(super) current: Move,
    /// Half-moves of full search left below this position.
    pub(super) depth: u8,
    /// Half-moves past the full search, where only captures are followed.
    pub(super) quiet: u8,
    /// How many legal moves have been tried.
    pub(super) legal: u16,
    pub(super) in_check: bool,
    /// Whether the reply to `current` is being searched in a window too
    /// narrow to give its value, to see whether the value matters.
    pub(super) probing: bool,
    /// Whether the reply to `current` is being searched less deeply than its
    /// turn, to see whether the move is worth the full depth.
    pub(super) reduced: bool,
}

impl Frame {
    pub(super) fn new(position: Position) -> Self {
        Self {
            position,
            moves: MoveList::new(),
            state: State::Fresh,
            alpha: 0,
            beta: 0,
            alpha_at_entry: 0,
            best: 0,
            best_move: None,
            table_move: None,
            current: NO_MOVE,
            depth: 0,
            quiet: 0,
            legal: 0,
            in_check: false,
            probing: false,
            reduced: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_come_out_best_first_and_in_generated_order_among_equals() {
        let mut list = MoveList::new();
        let moves: Vec<Move> = ["a2a3", "b2b3", "c2c3", "d2d3", "e2e3"]
            .into_iter()
            .map(|text| Move::parse(text).unwrap())
            .collect();
        for &mv in &moves {
            list.add(mv);
        }
        // The fourth move is dropped; the second and fifth score highest.
        let scores = [Some(1), Some(7), Some(1), None, Some(7)];
        let mut next = scores.into_iter();
        list.score(|_| next.next().unwrap());

        let taken: Vec<Move> = std::iter::from_fn(|| list.take_best()).collect();
        assert_eq!(taken, [moves[1], moves[4], moves[0], moves[2]]);
    }

    #[test]
    fn a_discarded_move_is_not_among_those_tried() {
        let mut list = MoveList::new();
        let moves: Vec<Move> = ["a2a3", "b2b3", "c2c3", "d2d3"]
            .into_iter()
            .map(|text| Move::parse(text).unwrap())
            .collect();
        for &mv in &moves {
            list.add(mv);
        }
        assert_eq!(list.tried().count(), 0);
        assert_eq!(list.take_best(), Some(moves[0]));
        assert_eq!(list.take_best(), Some(moves[1]));
        list.discard_last();
        assert_eq!(list.take_best(), Some(moves[2]));
        assert_eq!(list.tried().collect::<Vec<_>>(), [moves[0], moves[2]]);
        assert_eq!(list.take_best(), Some(moves[3]));
        assert_eq!(list.take_best(), None);
        assert_eq!(
            list.tried().collect::<Vec<_>>(),
            [moves[0], moves[2], moves[3]]
        );
    }

    #[test]
    fn a_full_list_takes_no_more_moves() {
        let mut list = MoveList::new();
        for _ in 0..CAPACITY + 5 {
            list.add(NO_MOVE);
        }
        assert_eq!(list.len, CAPACITY);
    }
}
