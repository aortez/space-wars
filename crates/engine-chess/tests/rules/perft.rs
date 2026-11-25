//! Move-generation node counts.

use crate::{INITIAL, engine, report};

/// The largest count the everyday run checks. Larger counts take an
/// optimized build to finish quickly, so they run on request.
const QUICK_LIMIT: u64 = 5_000_000;

/// A position and how many sequences of 1, 2, 3 ... legal half-moves start
/// from it.
struct Case {
    name: &'static str,
    fen: &'static str,
    counts: &'static [u64],
}

/// Checks every count that `selected` accepts.
fn check_counts(selected: impl Fn(u64) -> bool) {
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in CASES {
        for (depth, &expected) in (1..).zip(case.counts) {
            if !selected(expected) {
                continue;
            }
            checked += 1;
            let found = engine::perft(case.fen, depth);
            if found != expected {
                failures.push(format!(
                    "{}: {found} sequences of {depth} half-moves, expected {expected}",
                    case.name
                ));
                // A wrong count makes every deeper count wrong as well.
                break;
            }
        }
    }
    assert!(checked > 0, "no count was selected");
    report(&failures);
}

#[test]
fn counts_up_to_five_million_are_right() {
    check_counts(|count| count <= QUICK_LIMIT);
}

#[test]
#[ignore = "counts up to 193,690,690; run with --ignored in an optimized profile"]
fn counts_above_five_million_are_right() {
    check_counts(|count| count > QUICK_LIMIT);
}

const CASES: &[Case] = &[
    // The initial position.
    Case {
        name: "initial-position",
        fen: INITIAL,
        counts: &[20, 400, 8_902, 197_281, 4_865_609, 119_060_324],
    },
    // Wiki position 2, "Kiwipete": both kings may castle on either side, and en
    // passant captures appear from the second half-move.
    Case {
        name: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        counts: &[48, 2_039, 97_862, 4_085_603, 193_690_690],
    },
    // Wiki position 3: a rook endgame. An en passant capture that would clear
    // the rank between a rook and a king is illegal.
    Case {
        name: "rook-endgame",
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        counts: &[14, 191, 2_812, 43_238, 674_624, 11_030_083, 178_633_661],
    },
    // Wiki position 4: White starts in check with pawns about to promote on
    // both sides.
    Case {
        name: "promotions-and-checks",
        fen: "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        counts: &[6, 264, 9_467, 422_333, 15_833_292],
    },
    // Wiki position 4 with the colours swapped. The counts are the same, so a
    // difference points at a rule applied to one colour only.
    Case {
        name: "promotions-and-checks-mirrored",
        fen: "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
        counts: &[6, 264, 9_467, 422_333, 15_833_292],
    },
    // Wiki position 5: the d7 pawn promotes by capturing on c8.
    Case {
        name: "promotion-by-capture",
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        counts: &[44, 1_486, 62_379, 2_103_487, 89_941_194],
    },
    // Wiki position 6: a quiet symmetrical middlegame.
    Case {
        name: "symmetrical-middlegame",
        fen: "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        counts: &[46, 2_079, 89_890, 3_894_594, 164_075_551],
    },
    // Black answers a bishop check with six king moves, one interposition and
    // one capture.
    Case {
        name: "answer-a-bishop-check",
        fen: "r6r/1b2k1bq/8/8/7B/8/8/R3K2R b KQ - 3 2",
        counts: &[8],
    },
    // The pawn that has just advanced two squares gives check. Capturing it en
    // passant is one of the eight replies.
    Case {
        name: "capture-the-checking-pawn-en-passant",
        fen: "8/8/8/2k5/2pP4/8/B7/4K3 b - d3 0 3",
        counts: &[8],
    },
    // An opening position after 1. a3 Na6.
    Case {
        name: "opening-after-a3-na6",
        fen: "r1bqkbnr/pppppppp/n7/8/8/P7/1PPPPPPP/RNBQKBNR w KQkq - 2 2",
        counts: &[19],
    },
    // Black is in check with both castling rights. The five replies are two
    // king moves, two pawn captures and one interposition.
    Case {
        name: "no-castling-out-of-check",
        fen: "r3k2r/p1pp1pb1/bn2Qnp1/2qPN3/1p2P3/2N5/PPPBBPPP/R3K2R b KQkq - 3 2",
        counts: &[5],
    },
    // The d7 pawn is pinned by the queen on e6 and may only capture it.
    Case {
        name: "pinned-pawn-captures-its-pinner",
        fen: "2kr3r/p1ppqpb1/bn2Qnp1/3PN3/1p2P3/2N5/PPPBBPPP/R3K2R b KQ - 3 2",
        counts: &[44],
    },
    // White's queen is pinned on the a5-e1 diagonal and may only move along it.
    // The d7 pawn promotes by push or by capture.
    Case {
        name: "pinned-queen-keeps-to-the-pin",
        fen: "rnb2k1r/pp1Pbppp/2p5/q7/2B5/8/PPPQNnPP/RNB1K2R w KQ - 3 9",
        counts: &[39],
    },
    // The c5 pawn is pinned on its file and may still advance.
    Case {
        name: "file-pinned-pawn-advances",
        fen: "2r5/3pk3/8/2P5/8/2K5/8/8 w - - 5 4",
        counts: &[9],
    },
    // After ...d5 the capture cxd6 en passant would clear the fifth rank
    // between White's king and the rook, so it is illegal.
    Case {
        name: "en-passant-opens-the-rank",
        fen: "3k4/3p4/8/K1P4r/8/8/8/8 b - - 0 1",
        counts: &[18, 92, 1_670, 10_138, 185_429, 1_134_888],
    },
    // After d4 the capture ...cxd3 en passant would take the c4 pawn off the
    // bishop's diagonal to Black's king, so it is illegal.
    Case {
        name: "en-passant-by-a-pinned-pawn",
        fen: "8/8/4k3/8/2p5/8/B2P2K1/8 w - - 0 1",
        counts: &[13, 102, 1_266, 10_276, 135_655, 1_015_133],
    },
    // ...cxd3 en passant removes the pawn from the bishop's diagonal and gives
    // discovered check.
    Case {
        name: "en-passant-discovers-check",
        fen: "8/8/1k6/2b5/2pP4/8/5K2/8 b - d3 0 1",
        counts: &[15, 126, 1_928, 13_931, 206_379, 1_440_467],
    },
    // Castling kingside puts the rook on the file of Black's king.
    Case {
        name: "kingside-castling-gives-check",
        fen: "5k2/8/8/8/8/8/8/4K2R w K - 0 1",
        counts: &[15, 66, 1_198, 6_399, 120_330, 661_072],
    },
    // Castling queenside puts the rook on the file of Black's king.
    Case {
        name: "queenside-castling-gives-check",
        fen: "3k4/8/8/8/8/8/8/R3K3 w Q - 0 1",
        counts: &[16, 71, 1_286, 7_418, 141_077, 803_711],
    },
    // Black's bishops attack both white rooks on their home squares, and the
    // queenside rooks face each other. Capturing a rook ends the right to
    // castle with it.
    Case {
        name: "captured-rooks-lose-castling-rights",
        fen: "r3k2r/1b4bq/8/8/8/8/7B/R3K2R w KQkq - 0 1",
        counts: &[26, 1_141, 27_826, 1_274_206],
    },
    // Each queen attacks squares the other king would cross or reach, so
    // castling is prevented until the attack ends.
    Case {
        name: "queens-attack-the-castling-squares",
        fen: "r3k2r/8/3Q4/8/8/5q2/8/R3K2R b KQkq - 0 1",
        counts: &[44, 1_494, 50_509, 1_720_476],
    },
    // White is in check and can answer by promoting: the pawn captures the rook
    // or interposes on e8.
    Case {
        name: "promote-out-of-check",
        fen: "2K2r2/4P3/8/8/8/8/8/3k4 w - - 0 1",
        counts: &[11, 133, 1_442, 19_174, 266_199, 3_821_001],
    },
    // Black's knight shields the queen's diagonal to White's king, so its moves
    // give discovered check.
    Case {
        name: "discovered-checks",
        fen: "8/8/1P2K3/8/2n5/1q6/8/5k2 b - - 0 1",
        counts: &[29, 165, 5_160, 31_961, 1_004_658],
    },
    // Promoting to a queen or rook on b8 checks along the eighth rank.
    Case {
        name: "promotion-gives-check",
        fen: "4k3/1P6/8/8/8/8/K7/8 w - - 0 1",
        counts: &[9, 40, 472, 2_661, 38_983, 217_342],
    },
    // Promoting to a knight on a8 gives check where a queen would not.
    Case {
        name: "underpromotion-gives-check",
        fen: "8/P1k5/K7/8/8/8/8/8 w - - 0 1",
        counts: &[6, 27, 273, 1_329, 18_135, 92_683],
    },
    // Black's king confines White's king to the corner. Stalemates arise from
    // the second half-move.
    Case {
        name: "stalemate-in-the-corner",
        fen: "K1k5/8/P7/8/8/8/8/8 w - - 0 1",
        counts: &[2, 6, 13, 63, 382, 2_217],
    },
    // Promoting to a queen stalemates at once. Other lines reach checkmate.
    Case {
        name: "promotion-stalemates-or-mates",
        fen: "8/k1P5/8/1K6/8/8/8/8 w - - 0 1",
        counts: &[10, 25, 268, 926, 10_857, 43_261, 567_584],
    },
    // Black's knight shields the queen's file to White's king, so some of its
    // moves give double check.
    Case {
        name: "double-check",
        fen: "8/8/2k5/5q2/5n2/8/5K2/8 b - - 0 1",
        counts: &[37, 183, 6_559, 23_527],
    },
];
