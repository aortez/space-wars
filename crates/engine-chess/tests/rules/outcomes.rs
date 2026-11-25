//! Results of single positions.

use crate::engine::{self, GameResult};
use crate::{INITIAL, report};

/// A position, whether the game is over there and why, and whether the
/// player to move is in check.
struct Case {
    name: &'static str,
    fen: &'static str,
    result: GameResult,
    check: bool,
}

#[test]
fn positions_have_the_expected_results() {
    let mut failures = Vec::new();
    for case in CASES {
        let found = engine::replay(case.fen, &[]);
        if found.result != case.result {
            failures.push(format!(
                "{}: the game is {:?}, expected {:?}",
                case.name, found.result, case.result
            ));
        }
        if found.check != case.check {
            failures.push(format!(
                "{}: check is {}, expected {}",
                case.name, found.check, case.check
            ));
        }
    }
    report(&failures);
}

const CASES: &[Case] = &[
    // The game has not started.
    Case {
        name: "initial-position",
        fen: INITIAL,
        result: GameResult::Ongoing,
        check: false,
    },
    // The king is in check and can capture the rook.
    Case {
        name: "check-is-not-checkmate",
        fen: "4k3/8/8/8/8/8/4r3/4K3 w - - 0 1",
        result: GameResult::Ongoing,
        check: true,
    },
    // It looks like checkmate, but the checking pawn can be captured en
    // passant.
    Case {
        name: "en-passant-is-the-only-reply",
        fen: "8/8/7R/k7/1Pp5/P1N5/8/4K3 b - b3 0 1",
        result: GameResult::Ongoing,
        check: true,
    },
    // The same placement without the en passant right is checkmate.
    Case {
        name: "checkmate-without-the-en-passant-right",
        fen: "8/8/7R/k7/1Pp5/P1N5/8/4K3 b - - 0 1",
        result: GameResult::WhiteWinsByCheckmate,
        check: true,
    },
    // Fool's mate.
    Case {
        name: "fools-mate",
        fen: "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        result: GameResult::BlackWinsByCheckmate,
        check: true,
    },
    // Scholar's mate.
    Case {
        name: "scholars-mate",
        fen: "r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4",
        result: GameResult::WhiteWinsByCheckmate,
        check: true,
    },
    // The king is shut in by its own pawns.
    Case {
        name: "back-rank-mate",
        fen: "R5k1/5ppp/8/8/8/8/8/4K3 b - - 0 1",
        result: GameResult::WhiteWinsByCheckmate,
        check: true,
    },
    // The king is shut in by its own pieces and a knight gives check.
    Case {
        name: "smothered-mate",
        fen: "6rk/5Npp/8/8/8/8/8/4K3 b - - 0 1",
        result: GameResult::WhiteWinsByCheckmate,
        check: true,
    },
    // Black has no legal move and is not in check.
    Case {
        name: "stalemate-by-queen",
        fen: "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1",
        result: GameResult::Stalemate,
        check: false,
    },
    // Black has no legal move and is not in check.
    Case {
        name: "stalemate-by-pawn",
        fen: "5k2/5P2/5K2/8/8/8/8/8 b - - 0 1",
        result: GameResult::Stalemate,
        check: false,
    },
    // White has no legal move and is not in check.
    Case {
        name: "stalemate-of-white",
        fen: "8/8/8/8/8/5k2/5p2/5K2 w - - 0 1",
        result: GameResult::Stalemate,
        check: false,
    },
    // White still has a knight, but it is pinned and the king has no square.
    Case {
        name: "stalemate-with-a-pinned-knight",
        fen: "8/8/8/8/8/2n5/2k5/KN5r w - - 0 1",
        result: GameResult::Stalemate,
        check: false,
    },
    // King against king.
    Case {
        name: "bare-kings",
        fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // King and bishop against king.
    Case {
        name: "bishop-against-bare-king",
        fen: "4k3/8/8/8/8/8/8/4KB2 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // King against king and bishop.
    Case {
        name: "bare-king-against-bishop",
        fen: "4kb2/8/8/8/8/8/8/4K3 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // King and knight against king.
    Case {
        name: "knight-against-bare-king",
        fen: "4k3/8/8/8/8/8/8/4KN2 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // King against king and knight.
    Case {
        name: "bare-king-against-knight",
        fen: "4kn2/8/8/8/8/8/8/4K3 b - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // The lone knight gives check, but the material still cannot mate.
    Case {
        name: "knight-check-with-dead-material",
        fen: "4k3/8/5N2/8/8/8/8/4K3 b - - 0 1",
        result: GameResult::DeadPosition,
        check: true,
    },
    // Both bishops stand on light squares.
    Case {
        name: "one-bishop-each-on-light-squares",
        fen: "2b1k3/8/8/8/8/8/8/4KB2 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // One side has two bishops, both on light squares.
    Case {
        name: "two-bishops-on-light-squares",
        fen: "4k3/8/8/8/8/8/8/1B2KB2 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // All three bishops stand on light squares.
    Case {
        name: "three-bishops-on-light-squares",
        fen: "2b1k3/8/8/8/8/8/8/1B2KB2 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // Both bishops stand on dark squares.
    Case {
        name: "one-bishop-each-on-dark-squares",
        fen: "4kb2/8/8/8/8/8/8/2B1K3 w - - 0 1",
        result: GameResult::DeadPosition,
        check: false,
    },
    // Bishops on opposite colours can still take part in a mate.
    Case {
        name: "one-bishop-each-on-opposite-colours",
        fen: "4kb2/8/8/8/8/8/8/4KB2 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // Two bishops on opposite colours can force mate.
    Case {
        name: "bishop-pair-on-both-colours",
        fen: "4k3/8/8/8/8/8/8/2B1KB2 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // Two knights cannot force mate, but mate is possible.
    Case {
        name: "two-knights-against-bare-king",
        fen: "4k3/8/8/8/8/8/8/3NKN2 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // A knight each: mate is possible.
    Case {
        name: "knight-against-knight",
        fen: "4kn2/8/8/8/8/8/8/4KN2 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // Bishop against knight: mate is possible.
    Case {
        name: "bishop-against-knight",
        fen: "4kn2/8/8/8/8/8/8/4KB2 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // Bishop and knight can force mate.
    Case {
        name: "bishop-and-knight",
        fen: "4k3/8/8/8/8/8/8/4KBN1 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // A pawn can promote.
    Case {
        name: "lone-pawn",
        fen: "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // A rook can force mate.
    Case {
        name: "lone-rook",
        fen: "4k3/8/8/8/8/8/8/4K2R w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
    // A queen can force mate.
    Case {
        name: "lone-queen",
        fen: "4k3/8/8/8/8/8/8/3QK3 w - - 0 1",
        result: GameResult::Ongoing,
        check: false,
    },
];
