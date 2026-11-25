//! Results, draw claims and move rights that depend on the moves played.

use std::collections::BTreeSet;

use crate::engine::{self, Claims, GameResult, Replay};
use crate::{INITIAL, report};

/// A game and what must hold in the position it reaches.
struct Game {
    name: &'static str,
    start: &'static str,
    /// The moves played from `start`, separated by spaces.
    play: &'static str,
    result: GameResult,
    /// The draw claims open to the player to move, where the case states them.
    claims: Option<Expected>,
    /// Moves that must be legal, separated by spaces.
    legal: &'static str,
    /// Moves that must not be legal, separated by spaces.
    illegal: &'static str,
}

/// The draw claims open to the player to move in a game that is still on.
struct Expected {
    /// The claims that are valid as the position stands.
    now: Claims,
    /// Every move the player may name as the intended move of a threefold
    /// claim, separated by spaces.
    with_threefold: &'static str,
    /// The same for a fifty-move claim.
    with_fifty: &'static str,
}

/// A game that states nothing beyond its result.
const PLAIN: Game = Game {
    name: "",
    start: "",
    play: "",
    result: GameResult::Ongoing,
    claims: None,
    legal: "",
    illegal: "",
};

const NOTHING: Claims = Claims {
    threefold: false,
    fifty: false,
};
const THREEFOLD: Claims = Claims {
    threefold: true,
    fifty: false,
};
const FIFTY: Claims = Claims {
    threefold: false,
    fifty: true,
};

/// Compares the moves expected to support a claim with the legal moves whose
/// claims `supports` accepts.
fn check_claims_with(
    game: &Game,
    claim: &str,
    expected: &str,
    found: &Replay,
    supports: impl Fn(&Claims) -> bool,
    failures: &mut Vec<String>,
) {
    let expected: BTreeSet<&str> = expected.split_whitespace().collect();
    let found: BTreeSet<&str> = found
        .claims_with
        .iter()
        .filter(|(_, claims)| supports(claims))
        .map(|(mv, _)| mv.as_str())
        .collect();
    if found != expected {
        failures.push(format!(
            "{}: a {claim} claim may name {found:?}, expected {expected:?}",
            game.name
        ));
    }
}

#[test]
fn games_reach_the_expected_results_claims_and_rights() {
    let mut failures = Vec::new();
    for game in GAMES {
        if game.result != GameResult::Ongoing {
            assert!(
                game.claims.is_none() && game.legal.is_empty() && game.illegal.is_empty(),
                "{}: claims or move rights stated for a game that is over",
                game.name
            );
        }

        let moves: Vec<&str> = game.play.split_whitespace().collect();
        let found = engine::replay(game.start, &moves);
        if found.result != game.result {
            failures.push(format!(
                "{}: the game is {:?}, expected {:?}",
                game.name, found.result, game.result
            ));
            continue;
        }

        if let Some(expected) = &game.claims {
            if found.claims_now != expected.now {
                failures.push(format!(
                    "{}: claims as the position stands are {:?}, expected {:?}",
                    game.name, found.claims_now, expected.now
                ));
            }
            check_claims_with(
                game,
                "threefold",
                expected.with_threefold,
                &found,
                |claims| claims.threefold,
                &mut failures,
            );
            check_claims_with(
                game,
                "fifty-move",
                expected.with_fifty,
                &found,
                |claims| claims.fifty,
                &mut failures,
            );
        }
        for mv in game.legal.split_whitespace() {
            if !found.legal_moves.contains(mv) {
                failures.push(format!("{}: {mv} should be legal", game.name));
            }
        }
        for mv in game.illegal.split_whitespace() {
            if found.legal_moves.contains(mv) {
                failures.push(format!("{}: {mv} should not be legal", game.name));
            }
        }
    }
    report(&failures);
}

const GAMES: &[Game] = &[
    // The initial position has appeared twice. Nothing can be claimed.
    Game {
        name: "twofold-is-not-enough",
        start: INITIAL,
        play: "g1f3 g8f6 f3g1 f6g8",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // Black's ...Ng8 would bring the initial position about for the third time,
    // so Black may claim by naming that move before playing it.
    Game {
        name: "threefold-by-the-intended-move",
        start: INITIAL,
        play: "g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "f6g8",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The initial position has appeared three times. The player to move may
    // claim, and the game goes on if nobody does.
    Game {
        name: "threefold-on-the-board",
        start: INITIAL,
        play: "g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "g1f3",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The position with all four knights developed is reached three times by
    // three different move orders.
    Game {
        name: "threefold-by-different-move-orders",
        start: INITIAL,
        play: "g1f3 g8f6 b1c3 b8c6 f3g1 f6g8 c3b1 c6b8 b1c3 b8c6 g1f3 g8f6 c3b1 c6b8 f3g1 f6g8 \
               g1f3 b8c6 b1c3 g8f6",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The position recurs only every 44 half-moves, so the three occurrences
    // span 88. A history that keeps only recent positions would miss them.
    Game {
        name: "threefold-across-a-long-history",
        start: "6k1/R7/8/8/8/8/8/K7 w - - 0 1",
        play: "a1b1 g8h8 b1c1 h8g8 c1d1 g8h8 d1e1 h8g8 e1f1 g8h8 f1g1 h8g8 g1h1 g8h8 h1h2 h8g8 \
               h2h3 g8h8 h3h4 h8g8 h4h5 g8h8 h5g5 h8g8 g5f5 g8h8 f5e5 h8g8 e5d5 g8h8 d5c5 h8g8 \
               c5b5 g8h8 b5a5 h8g8 a5a4 g8h8 a4a3 h8g8 a3a2 g8h8 a2a1 h8g8 a1b1 g8h8 b1c1 h8g8 \
               c1d1 g8h8 d1e1 h8g8 e1f1 g8h8 f1g1 h8g8 g1h1 g8h8 h1h2 h8g8 h2h3 g8h8 h3h4 h8g8 \
               h4h5 g8h8 h5g5 h8g8 g5f5 g8h8 f5e5 h8g8 e5d5 g8h8 d5c5 h8g8 c5b5 g8h8 b5a5 h8g8 \
               a5a4 g8h8 a4a3 h8g8 a3a2 g8h8 a2a1 h8g8",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "a1a2 a1b1",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The initial position has appeared four times. Without a claim the game is
    // still on.
    Game {
        name: "fourfold-still-needs-a-claim",
        start: INITIAL,
        play: "g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "g1f3",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The fifth occurrence of a position draws the game without a claim.
    Game {
        name: "fivefold-ends-the-game",
        start: INITIAL,
        play: "g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8",
        result: GameResult::FivefoldRepetition,
        ..PLAIN
    },
    // The placement has appeared three times, but in the first the kings could
    // still castle. Only two occurrences count.
    Game {
        name: "lost-castling-rights-make-a-new-position",
        start: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        play: "e1e2 e8e7 e2e1 e7e8 e1e2 e8e7 e2e1 e7e8",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The same shuffle once more: three occurrences now share the same castling
    // rights.
    Game {
        name: "threefold-after-castling-rights-are-gone",
        start: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        play: "e1e2 e8e7 e2e1 e7e8 e1e2 e8e7 e2e1 e7e8 e1e2 e8e7 e2e1 e7e8",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "e1e2",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The placement has appeared three times with Black to move, but the first
    // time Black could capture en passant. Only two occurrences count.
    Game {
        name: "en-passant-right-makes-a-new-position",
        start: "4k3/8/8/8/1p6/8/P7/4K3 w - - 0 1",
        play: "a2a4 e8e7 e1e2 e7e8 e2e1 e8d8 e1d1 d8e8 d1e1",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // One more return to the placement: three occurrences without the capture.
    Game {
        name: "threefold-after-the-en-passant-right-lapses",
        start: "4k3/8/8/8/1p6/8/P7/4K3 w - - 0 1",
        play: "a2a4 e8e7 e1e2 e7e8 e2e1 e8d8 e1d1 d8e8 d1e1 e8e7 e1e2 e7e8 e2e1",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "e8e7",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The pawn advanced two squares, but no pawn could capture it, so the
    // position right after the advance counts as the first of three.
    Game {
        name: "double-push-with-no-capturer-repeats",
        start: "4k3/8/8/8/8/8/P7/4K3 w - - 0 1",
        play: "a2a4 e8e7 e1e2 e7e8 e2e1 e8e7 e1e2 e7e8 e2e1",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "e8e7",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // A pawn stands beside the advanced pawn, but capturing would expose its
    // king, so no move was lost and the first occurrence counts.
    Game {
        name: "illegal-en-passant-capture-repeats",
        start: "8/8/8/8/k3p2R/8/3P4/6K1 w - - 0 1",
        play: "d2d4 a4a5 g1g2 a5a4 g2g1 a4a5 g1g2 a5a4 g2g1",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: THREEFOLD,
            with_threefold: "a4a5",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The placement has appeared three times, twice with White to move and once
    // with Black to move.
    Game {
        name: "side-to-move-makes-a-new-position",
        start: "4k3/p7/8/8/8/8/P7/4K3 w - - 0 1",
        play: "e1d2 e8d8 d2d1 d8e8 d1e1 e8d7 e1d1 d7d8 d1e1 d8e8",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // Ninety-nine half-moves have passed without a capture or pawn move. Any
    // quiet move completes the fifty, castling included; the pawn moves and the
    // capture do not.
    Game {
        name: "fifty-moves-by-the-intended-move",
        start: "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 99 60",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "a1a2 a1a3 a1a4 a1a5 a1a6 a1b1 a1c1 a1d1 e1c1 e1d1 e1d2 e1f1 e1f2",
        }),
        ..PLAIN
    },
    // The hundredth quiet half-move has been played. The player to move may
    // claim, and the game goes on if nobody does.
    Game {
        name: "fifty-moves-on-the-board",
        start: "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 99 60",
        play: "a1b1",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: FIFTY,
            with_threefold: "",
            with_fifty: "e8d7 e8d8 e8e7 e8f7 e8f8",
        }),
        ..PLAIN
    },
    // A pawn move ends the run of quiet moves.
    Game {
        name: "pawn-move-restarts-the-count",
        start: "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 99 60",
        play: "e2e4",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // A capture ends the run of quiet moves.
    Game {
        name: "capture-restarts-the-count",
        start: "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 99 60",
        play: "a1a7",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The hundred-and-fiftieth quiet half-move draws the game without a claim.
    Game {
        name: "seventy-five-moves-end-the-game",
        start: "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 149 100",
        play: "a1b1",
        result: GameResult::SeventyFiveMoves,
        ..PLAIN
    },
    // A pawn move on the hundred-and-fiftieth half-move restarts the count.
    Game {
        name: "pawn-move-avoids-the-seventy-five-move-draw",
        start: "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 149 100",
        play: "e2e3",
        result: GameResult::Ongoing,
        claims: Some(Expected {
            now: NOTHING,
            with_threefold: "",
            with_fifty: "",
        }),
        ..PLAIN
    },
    // The hundred-and-fiftieth quiet half-move gives checkmate, which stands.
    Game {
        name: "checkmate-outranks-seventy-five-moves",
        start: "7k/8/6K1/8/8/8/8/R7 w - - 149 100",
        play: "a1a8",
        result: GameResult::WhiteWinsByCheckmate,
        ..PLAIN
    },
    // Fool's mate, played from the initial position.
    Game {
        name: "fools-mate",
        start: INITIAL,
        play: "f2f3 e7e5 g2g4 d8h4",
        result: GameResult::BlackWinsByCheckmate,
        ..PLAIN
    },
    // Scholar's mate, played from the initial position.
    Game {
        name: "scholars-mate",
        start: INITIAL,
        play: "e2e4 e7e5 f1c4 b8c6 d1h5 g8f6 h5f7",
        result: GameResult::WhiteWinsByCheckmate,
        ..PLAIN
    },
    // The queen move leaves Black without a move.
    Game {
        name: "stalemate-by-a-queen-move",
        start: "7k/8/6K1/8/8/8/8/5Q2 w - - 0 1",
        play: "f1f7",
        result: GameResult::Stalemate,
        ..PLAIN
    },
    // Capturing the last pawn leaves king against king.
    Game {
        name: "capture-leaves-bare-kings",
        start: "7k/8/8/8/8/8/1p6/K7 w - - 0 1",
        play: "a1b2",
        result: GameResult::DeadPosition,
        ..PLAIN
    },
    // King and bishop against king cannot mate.
    Game {
        name: "promotion-to-a-bishop-is-dead",
        start: "7k/8/8/8/8/8/p7/4K3 b - - 0 1",
        play: "a2a1b",
        result: GameResult::DeadPosition,
        ..PLAIN
    },
    // King and queen against king can mate.
    Game {
        name: "promotion-to-a-queen-plays-on",
        start: "7k/8/8/8/8/8/p7/4K3 b - - 0 1",
        play: "a2a1q",
        result: GameResult::Ongoing,
        ..PLAIN
    },
    // From the old engine's TestRegressions: a two-square pawn advance lets the
    // neighbouring pawn capture en passant.
    Game {
        name: "en-passant-right-after-a-double-push",
        start: INITIAL,
        play: "e2e4 a7a6 e4e5 d7d5",
        result: GameResult::Ongoing,
        legal: "e5d6",
        ..PLAIN
    },
    // The capture is only available on the move right after the advance.
    Game {
        name: "en-passant-right-lasts-one-move",
        start: INITIAL,
        play: "e2e4 a7a6 e4e5 d7d5 a2a3 a6a5",
        result: GameResult::Ongoing,
        illegal: "e5d6",
        ..PLAIN
    },
    // The same for Black.
    Game {
        name: "black-en-passant-right-after-a-double-push",
        start: INITIAL,
        play: "a2a3 e7e5 a3a4 e5e4 d2d4",
        result: GameResult::Ongoing,
        legal: "e4d3",
        ..PLAIN
    },
    // The same for Black.
    Game {
        name: "black-en-passant-right-lasts-one-move",
        start: INITIAL,
        play: "a2a3 e7e5 a3a4 e5e4 d2d4 a7a6 a4a5",
        result: GameResult::Ongoing,
        illegal: "e4d3",
        ..PLAIN
    },
    // The a1 rook left and came back. White keeps only the kingside right.
    Game {
        name: "rook-move-ends-castling-on-its-side",
        start: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        play: "a1a2 a8a7 a2a1 a7a8",
        result: GameResult::Ongoing,
        legal: "e1g1",
        illegal: "e1c1",
        ..PLAIN
    },
    // The a8 rook left and came back. Black keeps only the kingside right.
    Game {
        name: "black-rook-move-ends-castling-on-its-side",
        start: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        play: "a1a2 a8a7 a2a1 a7a8 e1d1",
        result: GameResult::Ongoing,
        legal: "e8g8",
        illegal: "e8c8",
        ..PLAIN
    },
    // The king left and came back. White may not castle on either side.
    Game {
        name: "king-move-ends-castling",
        start: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        play: "e1e2 e8e7 e2e1 e7e8",
        result: GameResult::Ongoing,
        legal: "e1e2",
        illegal: "e1g1 e1c1",
        ..PLAIN
    },
    // The h1 rook was captured where it stood. With h1 empty again and the
    // squares between free, White still may not castle kingside.
    Game {
        name: "captured-rook-ends-castling",
        start: "r3k2r/8/8/8/8/8/6b1/R3K2R b KQkq - 0 1",
        play: "g2h1 a1b1 h1e4",
        result: GameResult::Ongoing,
        legal: "e1f1",
        illegal: "e1g1 e1c1",
        ..PLAIN
    },
    // The h1 rook was captured where it stood and a second white rook has taken
    // its place. The kingside right went with the first rook; the queenside
    // right is untouched.
    Game {
        name: "another-rook-does-not-restore-castling",
        start: "r3k2r/8/8/8/8/7R/6b1/R3K2R b KQkq - 0 1",
        play: "g2h1 h3h1 e8d8",
        result: GameResult::Ongoing,
        legal: "e1c1",
        illegal: "e1g1",
        ..PLAIN
    },
];
