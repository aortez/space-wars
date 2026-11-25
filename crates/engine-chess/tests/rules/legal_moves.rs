//! Complete sets of legal moves.

use std::collections::BTreeSet;

use crate::engine::{self, GameResult};
use crate::{INITIAL, report};

/// A position and every move that is legal in it.
struct Case {
    name: &'static str,
    fen: &'static str,
    /// The legal moves, and no others, separated by spaces.
    moves: &'static str,
}

#[test]
fn legal_moves_are_exactly_the_expected_ones() {
    let mut failures = Vec::new();
    for case in CASES {
        let expected: BTreeSet<&str> = case.moves.split_whitespace().collect();
        let found = engine::replay(case.fen, &[]);
        if found.result != GameResult::Ongoing {
            failures.push(format!(
                "{}: the game is {:?}, expected it to be ongoing",
                case.name, found.result
            ));
            continue;
        }
        let found: BTreeSet<&str> = found.legal_moves.iter().map(String::as_str).collect();
        if found != expected {
            failures.push(format!(
                "{}: missing {:?}, not expected {:?}",
                case.name,
                expected.difference(&found).collect::<Vec<_>>(),
                found.difference(&expected).collect::<Vec<_>>()
            ));
        }
    }
    report(&failures);
}

const CASES: &[Case] = &[
    // The twenty opening moves.
    Case {
        name: "initial-position",
        fen: INITIAL,
        moves: "a2a3 a2a4 b1a3 b1c3 b2b3 b2b4 c2c3 c2c4 d2d3 d2d4 e2e3 e2e4 f2f3 f2f4 g1f3 g1h3 \
                g2g3 g2g4 h2h3 h2h4",
    },
    // Nothing prevents castling on either side.
    Case {
        name: "castle-either-side-white",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 a1b1 a1c1 a1d1 e1c1 e1d1 e1d2 e1e2 e1f1 e1f2 \
                e1g1 h1f1 h1g1 h1h2 h1h3 h1h4 h1h5 h1h6 h1h7 h1h8",
    },
    // Nothing prevents castling on either side.
    Case {
        name: "castle-either-side-black",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1",
        moves: "a8a1 a8a2 a8a3 a8a4 a8a5 a8a6 a8a7 a8b8 a8c8 a8d8 e8c8 e8d7 e8d8 e8e7 e8f7 e8f8 \
                e8g8 h8f8 h8g8 h8h1 h8h2 h8h3 h8h4 h8h5 h8h6 h8h7",
    },
    // Only the rook crosses b1, so an attack on it does not prevent castling.
    Case {
        name: "castle-queenside-with-b1-attacked",
        fen: "1r2k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 a1b1 a1c1 a1d1 e1c1 e1d1 e1d2 e1e2 e1f1 e1f2",
    },
    // An attack on the rook does not prevent castling.
    Case {
        name: "castle-queenside-with-the-rook-attacked",
        fen: "r3k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 a1b1 a1c1 a1d1 e1c1 e1d1 e1d2 e1e2 e1f1 e1f2",
    },
    // The king may not castle onto an attacked square.
    Case {
        name: "no-castling-onto-attacked-c1",
        fen: "2r1k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 a1b1 a1c1 a1d1 e1d1 e1d2 e1e2 e1f1 e1f2",
    },
    // The king may not castle across an attacked square.
    Case {
        name: "no-castling-across-attacked-d1",
        fen: "3rk3/8/8/8/8/8/8/R3K3 w Q - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 a1b1 a1c1 a1d1 e1e2 e1f1 e1f2",
    },
    // The king may not castle while in check.
    Case {
        name: "no-castling-out-of-check",
        fen: "4r1k1/8/8/8/8/8/8/R3K2R w KQ - 0 1",
        moves: "e1d1 e1d2 e1f1 e1f2",
    },
    // An attack on the rook does not prevent castling.
    Case {
        name: "castle-kingside-with-the-rook-attacked",
        fen: "4k2r/8/8/8/8/8/8/4K2R w K - 0 1",
        moves: "e1d1 e1d2 e1e2 e1f1 e1f2 e1g1 h1f1 h1g1 h1h2 h1h3 h1h4 h1h5 h1h6 h1h7 h1h8",
    },
    // The king may not castle onto an attacked square.
    Case {
        name: "no-castling-onto-attacked-g1",
        fen: "4k1r1/8/8/8/8/8/8/4K2R w K - 0 1",
        moves: "e1d1 e1d2 e1e2 e1f1 e1f2 h1f1 h1g1 h1h2 h1h3 h1h4 h1h5 h1h6 h1h7 h1h8",
    },
    // The king may not castle across an attacked square.
    Case {
        name: "no-castling-across-attacked-f1",
        fen: "4kr2/8/8/8/8/8/8/4K2R w K - 0 1",
        moves: "e1d1 e1d2 e1e2 h1f1 h1g1 h1h2 h1h3 h1h4 h1h5 h1h6 h1h7 h1h8",
    },
    // A pawn attacks f1, the square the king would cross.
    Case {
        name: "no-castling-across-a-pawn-attack",
        fen: "4k3/8/8/8/8/8/6p1/4K2R w K - 0 1",
        moves: "e1d1 e1d2 e1e2 e1f2 h1f1 h1g1 h1h2 h1h3 h1h4 h1h5 h1h6 h1h7 h1h8",
    },
    // Every square between king and rook must be empty, including b1, which the
    // king does not cross.
    Case {
        name: "no-castling-past-a-piece-on-b1",
        fen: "4k3/8/8/8/8/8/8/RN2K3 w Q - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 b1a3 b1c3 b1d2 e1d1 e1d2 e1e2 e1f1 e1f2",
    },
    // King and rooks stand on their home squares, but the position grants no
    // castling rights.
    Case {
        name: "no-castling-without-the-right",
        fen: "4k3/8/8/8/8/8/8/R3K2R w - - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 a1b1 a1c1 a1d1 e1d1 e1d2 e1e2 e1f1 e1f2 h1f1 \
                h1g1 h1h2 h1h3 h1h4 h1h5 h1h6 h1h7 h1h8",
    },
    // Only the kingside right remains.
    Case {
        name: "castle-only-where-the-right-remains",
        fen: "4k3/8/8/8/8/8/8/R3K2R w K - 0 1",
        moves: "a1a2 a1a3 a1a4 a1a5 a1a6 a1a7 a1a8 a1b1 a1c1 a1d1 e1d1 e1d2 e1e2 e1f1 e1f2 e1g1 \
                h1f1 h1g1 h1h2 h1h3 h1h4 h1h5 h1h6 h1h7 h1h8",
    },
    // Only the rook crosses b8, so an attack on it does not prevent castling.
    Case {
        name: "black-castle-queenside-with-b8-attacked",
        fen: "r3k3/8/8/8/8/8/8/1R2K3 b q - 0 1",
        moves: "a8a1 a8a2 a8a3 a8a4 a8a5 a8a6 a8a7 a8b8 a8c8 a8d8 e8c8 e8d7 e8d8 e8e7 e8f7 e8f8",
    },
    // The king may not castle onto an attacked square.
    Case {
        name: "black-no-castling-onto-attacked-c8",
        fen: "r3k3/8/8/8/8/8/8/2R1K3 b q - 0 1",
        moves: "a8a1 a8a2 a8a3 a8a4 a8a5 a8a6 a8a7 a8b8 a8c8 a8d8 e8d7 e8d8 e8e7 e8f7 e8f8",
    },
    // The king may not castle across an attacked square.
    Case {
        name: "black-no-castling-across-attacked-d8",
        fen: "r3k3/8/8/8/8/8/8/3RK3 b q - 0 1",
        moves: "a8a1 a8a2 a8a3 a8a4 a8a5 a8a6 a8a7 a8b8 a8c8 a8d8 e8e7 e8f7 e8f8",
    },
    // An attack on the rook does not prevent castling.
    Case {
        name: "black-castle-kingside-with-the-rook-attacked",
        fen: "4k2r/8/8/8/8/8/8/4K2R b k - 0 1",
        moves: "e8d7 e8d8 e8e7 e8f7 e8f8 e8g8 h8f8 h8g8 h8h1 h8h2 h8h3 h8h4 h8h5 h8h6 h8h7",
    },
    // The king may not castle onto an attacked square.
    Case {
        name: "black-no-castling-onto-attacked-g8",
        fen: "4k2r/8/8/8/8/8/8/4K1R1 b k - 0 1",
        moves: "e8d7 e8d8 e8e7 e8f7 e8f8 h8f8 h8g8 h8h1 h8h2 h8h3 h8h4 h8h5 h8h6 h8h7",
    },
    // The king may not castle across an attacked square.
    Case {
        name: "black-no-castling-across-attacked-f8",
        fen: "4k2r/8/8/8/8/8/8/4KR2 b k - 0 1",
        moves: "e8d7 e8d8 e8e7 h8f8 h8g8 h8h1 h8h2 h8h3 h8h4 h8h5 h8h6 h8h7",
    },
    // Black has just played ...f5. White may capture it en passant, but not the
    // d5 pawn, which arrived a move earlier.
    Case {
        name: "en-passant-capture",
        fen: "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
        moves: "a2a3 a2a4 b1a3 b1c3 b2b3 b2b4 c2c3 c2c4 d1e2 d1f3 d1g4 d1h5 d2d3 d2d4 e1e2 e5e6 \
                e5f6 f1a6 f1b5 f1c4 f1d3 f1e2 f2f3 f2f4 g1e2 g1f3 g1h3 g2g3 g2g4 h2h3 h2h4",
    },
    // Pawns on both sides of the advanced pawn may capture it.
    Case {
        name: "en-passant-by-either-neighbour",
        fen: "4k3/8/8/8/2pPp3/8/8/4K3 b - d3 0 1",
        moves: "c4c3 c4d3 e4d3 e4e3 e8d7 e8d8 e8e7 e8f7 e8f8",
    },
    // Capturing en passant would remove both pawns from the fourth rank and
    // expose Black's king to the queen.
    Case {
        name: "en-passant-would-open-the-rank",
        fen: "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        moves: "a4a3 a4a5 a4b3 a4b4 a4b5 e4e3",
    },
    // The pawn that gives check may be captured en passant.
    Case {
        name: "en-passant-captures-the-checking-pawn",
        fen: "8/8/8/2k5/3Pp3/8/8/4K3 b - d3 0 1",
        moves: "c5b4 c5b5 c5b6 c5c4 c5c6 c5d4 c5d5 c5d6 e4d3",
    },
    // Black's king has no flight square, and the only way out of check is to
    // capture the checking pawn en passant.
    Case {
        name: "en-passant-is-the-only-reply",
        fen: "8/8/7R/k7/1Pp5/P1N5/8/4K3 b - b3 0 1",
        moves: "c4b3",
    },
    // The d4 pawn is pinned on the g1-a7 diagonal. Capturing en passant keeps
    // it on that diagonal; advancing would leave it.
    Case {
        name: "en-passant-along-the-pin",
        fen: "8/8/1k6/8/3pP3/8/8/4K1B1 b - e3 0 1",
        moves: "b6a5 b6a6 b6a7 b6b5 b6b7 b6c5 b6c6 b6c7 d4e3",
    },
    // The d4 pawn is pinned on its file. It may advance, but capturing en
    // passant would leave the file.
    Case {
        name: "no-en-passant-off-the-pinning-file",
        fen: "3k4/8/8/8/3pP3/8/8/3RK3 b - e3 0 1",
        moves: "d4d3 d8c7 d8c8 d8d7 d8e7 d8e8",
    },
    // The position names an en passant square, but no pawn stands next to the
    // advanced pawn.
    Case {
        name: "en-passant-square-with-no-capturer",
        fen: "4k3/8/8/8/4P3/8/8/4K3 b - e3 0 1",
        moves: "e8d7 e8d8 e8e7 e8f7 e8f8",
    },
    // From the old engine's TestLegalMoveGen: the king may go to e4 but not to
    // c4, which the b5 pawn attacks, nor onto the rook's rank.
    Case {
        name: "king-may-not-step-into-attack",
        fen: "5k2/6pp/p2q4/1p6/3P4/2PKP2Q/PP3r2/3R4 w - - 0 2",
        moves: "a2a3 a2a4 b2b3 b2b4 c3c4 d1a1 d1b1 d1c1 d1d2 d1e1 d1f1 d1g1 d1h1 d3e4 d4d5 e3e4 \
                h3c8 h3d7 h3e6 h3f1 h3f3 h3f5 h3g2 h3g3 h3g4 h3h1 h3h2 h3h4 h3h5 h3h6 h3h7",
    },
    // Four white pieces are pinned. The knight and the bishop cannot move, the
    // rook keeps to its file and the pawn may only capture the queen that pins
    // it.
    Case {
        name: "pinned-pieces",
        fen: "4r1k1/8/8/b7/4R3/2N3q1/5P2/r1B1K3 w - - 0 1",
        moves: "e1d1 e1d2 e1e2 e1f1 e4e2 e4e3 e4e5 e4e6 e4e7 e4e8 f2g3",
    },
    // In double check only the king may move. Capturing the knight or blocking
    // the rook leaves the other check standing.
    Case {
        name: "double-check",
        fen: "4r1k1/8/8/8/8/5n2/6P1/3QKB2 w - - 0 1",
        moves: "e1f2",
    },
    // A rook check can be blocked or stepped out of, but not castled out of.
    Case {
        name: "answer-a-rook-check",
        fen: "4r1k1/8/8/8/8/3B4/8/R3K3 w Q - 0 1",
        moves: "d3e2 d3e4 e1d1 e1d2 e1f1 e1f2",
    },
    // A knight check is answered by capturing the knight or moving the king.
    Case {
        name: "knight-check-cannot-be-blocked",
        fen: "6k1/8/8/8/8/5n2/4P3/4K3 w - - 0 1",
        moves: "e1d1 e1f1 e1f2 e2f3",
    },
    // The rook checks along the first rank. It attacks d1 outright, and f1
    // through e1 once the king has left.
    Case {
        name: "king-cannot-retreat-along-the-checking-line",
        fen: "4k3/8/8/8/8/8/8/r3K3 w - - 0 1",
        moves: "e1d2 e1e2 e1f2",
    },
    // A pawn reaching the last rank becomes a queen, rook, bishop or knight,
    // whether it pushes or captures.
    Case {
        name: "promotion-by-push-and-by-capture",
        fen: "1n2k3/P7/8/8/8/8/8/4K3 w - - 0 1",
        moves: "a7a8b a7a8n a7a8q a7a8r a7b8b a7b8n a7b8q a7b8r e1d1 e1d2 e1e2 e1f1 e1f2",
    },
    // A pawn reaching the last rank becomes a queen, rook, bishop or knight,
    // whether it pushes or captures.
    Case {
        name: "black-promotion-by-push-and-by-capture",
        fen: "4k3/8/8/8/8/8/p7/1N2K3 b - - 0 1",
        moves: "a2a1b a2a1n a2a1q a2a1r a2b1b a2b1n a2b1q a2b1r e8d7 e8d8 e8e7 e8f7 e8f8",
    },
    // A pawn cannot push onto or through an occupied square.
    Case {
        name: "blocked-pawn",
        fen: "4k3/8/8/8/8/4p3/4P3/4K3 w - - 0 1",
        moves: "e1d1 e1f1",
    },
    // The single push is free, but the piece on e4 stops the double push.
    Case {
        name: "double-push-blocked-on-the-fourth-rank",
        fen: "4k3/8/8/8/4n3/8/4P3/4K3 w - - 0 1",
        moves: "e1d1 e1f1 e2e3",
    },
    // The pawn may capture on either side but cannot push into the pawn ahead.
    Case {
        name: "pawn-captures-diagonally",
        fen: "4k3/8/8/8/8/3ppp2/4P3/4K3 w - - 0 1",
        moves: "e1d1 e1f1 e2d3 e2f3",
    },
];
