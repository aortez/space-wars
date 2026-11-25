//! Which quiet moves have lately been good enough to stop a search at.
//!
//! A move that refuted one line often refutes its neighbours, so the search
//! tries quiet moves in the order of how well they have done so far.

use crate::board::Color;
use crate::moves::Move;

/// A score for each player's quiet moves, by the squares they go between.
#[derive(Clone)]
pub(super) struct History {
    scores: Vec<i16>,
}

impl History {
    /// No score rises above this or falls below its negative.
    pub(super) const LIMIT: i16 = 8_000;
    /// The memory a history holds.
    pub(super) const BYTES: usize = 2 * 64 * 64 * std::mem::size_of::<i16>();

    pub(super) fn new() -> Self {
        Self {
            scores: vec![0; 2 * 64 * 64],
        }
    }

    fn slot(side: Color, mv: Move) -> usize {
        (side.index() * 64 + mv.from.index()) * 64 + mv.to.index()
    }

    pub(super) fn score(&self, side: Color, mv: Move) -> i16 {
        self.scores[Self::slot(side, mv)]
    }

    /// Moves the score of `mv` up if it was `good` enough to stop at and
    /// down if it was tried and was not, by `weight` at most, which lies
    /// between zero and [`Self::LIMIT`].
    ///
    /// The nearer a score stands to the limit it is moving towards, the less
    /// it moves, so it never passes the limit and old results fade as new
    /// ones arrive.
    pub(super) fn record(&mut self, side: Color, mv: Move, weight: i16, good: bool) {
        let score = &mut self.scores[Self::slot(side, mv)];
        let (current, weight) = (i32::from(*score), i32::from(weight));
        let towards = if good { weight } else { -weight };
        *score = (current + towards - current * weight / i32::from(Self::LIMIT)) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(text: &str) -> Move {
        Move::parse(text).unwrap()
    }

    #[test]
    fn a_score_follows_results_and_stays_within_the_limit() {
        let mut history = History::new();
        let (good, bad) = (mv("g1f3"), mv("b1c3"));
        assert_eq!(history.score(Color::White, good), 0);

        let mut last = 0;
        for weight in [1, 16, 400, 6_400, History::LIMIT, 6_400, 16, 1] {
            for _ in 0..50 {
                history.record(Color::White, good, weight, true);
                history.record(Color::White, bad, weight, false);
                let (up, down) = (
                    history.score(Color::White, good),
                    history.score(Color::White, bad),
                );
                assert!(up >= last && up <= History::LIMIT, "{up} after {last}");
                assert_eq!(down, -up);
                last = up;
            }
        }
        assert_eq!(last, History::LIMIT);

        // A move that stops doing well comes down again.
        history.record(Color::White, good, 400, false);
        assert!(history.score(Color::White, good) < History::LIMIT - 400);
        // The other player's moves, and other moves, are untouched.
        assert_eq!(history.score(Color::Black, good), 0);
        assert_eq!(history.score(Color::White, mv("f3g1")), 0);
    }
}
