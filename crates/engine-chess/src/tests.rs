//! Properties of positions that the reference suite cannot see from outside.

use crate::{FenError, Move, PieceKind, Position, Square};

/// Positions that between them castle, capture en passant and promote within
/// three half-moves.
const BUSY: [&str; 4] = [
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
];

/// Calls `check` on every position within `depth` half-moves of `position`.
fn walk(position: &Position, depth: u32, check: &mut impl FnMut(&Position)) {
    check(position);
    if depth == 0 {
        return;
    }
    for (_, next) in position.successors() {
        walk(&next, depth - 1, check);
    }
}

#[test]
fn the_stored_hash_equals_a_hash_worked_out_from_nothing() {
    for fen in BUSY {
        let mut visited = 0;
        walk(&Position::from_fen(fen).unwrap(), 3, &mut |position| {
            assert_eq!(position.hash(), position.computed_hash(), "{position}");
            visited += 1;
        });
        assert!(visited > 2_000, "{fen}: only {visited} positions");
    }
}

#[test]
fn a_position_reads_back_from_the_fen_it_writes() {
    for fen in BUSY {
        walk(&Position::from_fen(fen).unwrap(), 2, &mut |position| {
            let written = position.to_string();
            assert_eq!(
                Position::from_fen(&written).as_ref(),
                Ok(position),
                "{written}"
            );
        });
    }
}

#[test]
fn an_en_passant_square_is_kept_only_while_a_capture_is_legal() {
    let cases = [
        // A pawn stands beside the advanced pawn and may capture it.
        ("4k3/8/8/8/2pPp3/8/8/4K3 b - d3 0 1", Some("d3")),
        // No pawn stands beside it.
        ("4k3/8/8/8/4P3/8/8/4K3 b - e3 0 1", None),
        // The only capture would expose the capturer's king.
        ("8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1", None),
    ];
    for (fen, kept) in cases {
        let position = Position::from_fen(fen).unwrap();
        assert_eq!(
            position.en_passant(),
            kept.and_then(Square::from_name),
            "{fen}"
        );
        let field = position.to_string();
        let field = field.split(' ').nth(3).unwrap().to_string();
        assert_eq!(field, kept.unwrap_or("-"), "{fen}");
    }
}

#[test]
fn a_double_step_past_no_capturer_leaves_no_en_passant_square() {
    let position = Position::initial()
        .play(Move::parse("e2e4").unwrap())
        .unwrap();
    assert_eq!(position.en_passant(), None);
    assert_eq!(
        position.to_string(),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
    );
}

#[test]
fn a_fen_that_no_game_could_reach_is_refused() {
    let refused = [
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0",
            FenError::Fields,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP w KQkq - 0 1",
            FenError::Placement,
        ),
        (
            "rnbqkbnr/pppppppp/9/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            FenError::Placement,
        ),
        (
            "rnbqkbnr/pppppppp/44/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            FenError::Placement,
        ),
        (
            "rnbqkbnr/pppppppp/7/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            FenError::Placement,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQXBNR w KQkq - 0 1",
            FenError::Placement,
        ),
        (
            "rnbq1bnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1",
            FenError::Kings,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKKBNR w kq - 0 1",
            FenError::Kings,
        ),
        (
            "rnbqkbnP/ppppppp1/8/8/8/8/PPPPPPP1/RNBQKBNR w KQq - 0 1",
            FenError::PawnOnBackRank,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1",
            FenError::SideToMove,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQxq - 0 1",
            FenError::Castling,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KK - 0 1",
            FenError::Castling,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBN1 w K - 0 1",
            FenError::Castling,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQ1KNR w Q - 0 1",
            FenError::Castling,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e9 0 1",
            FenError::EnPassant,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e6 0 1",
            FenError::EnPassant,
        ),
        (
            "rnbqkbnr/pppp1ppp/8/4p3/8/8/PPPPPPPP/RNBQKBNR w KQkq e3 0 2",
            FenError::EnPassant,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - x 1",
            FenError::Counter,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 0",
            FenError::Counter,
        ),
        ("4k3/8/8/8/8/8/8/4K2r b - - 0 1", FenError::OpponentInCheck),
    ];
    for (fen, error) in refused {
        assert_eq!(Position::from_fen(fen), Err(error), "{fen}");
    }
}

#[test]
fn moves_read_back_from_the_text_they_write() {
    for text in ["e2e4", "e1g1", "a7a8q", "h2g1n", "b7b8r", "c2c1b"] {
        assert_eq!(Move::parse(text).unwrap().to_string(), text);
    }
    assert_eq!(
        Move::parse("a7b8n"),
        Some(Move {
            from: Square::new(0, 6).unwrap(),
            to: Square::new(1, 7).unwrap(),
            promotion: Some(PieceKind::Knight),
        })
    );
    for text in [
        "", "e2", "e2e", "e2e9", "i2e4", "e2e4qq", "e7e8k", "e7e8p", "e7e8Q", "é2e4",
    ] {
        assert_eq!(Move::parse(text), None, "{text}");
    }
}

#[test]
fn counters_at_their_limit_do_not_overflow() {
    let fen = format!("4k3/8/8/8/8/8/8/4K2R b - - {0} {0}", u32::MAX);
    let position = Position::from_fen(&fen).unwrap();
    let next = position.play(Move::parse("e8d8").unwrap()).unwrap();
    assert_eq!(next.halfmove_clock(), u32::MAX);
    assert_eq!(next.fullmove_number(), u32::MAX);
}

#[test]
fn passing_hands_over_the_move_and_changes_nothing_else() {
    let mut passed = 0;
    for fen in BUSY {
        walk(&Position::from_fen(fen).unwrap(), 2, &mut |position| {
            if position.is_check() {
                return;
            }
            let next = position.pass();
            assert_eq!(next.side_to_move(), position.side_to_move().opposite());
            assert_eq!(next.en_passant(), None, "{position}");
            assert_eq!(next.hash(), next.computed_hash(), "{position}");
            assert!(next.pieces().eq(position.pieces()), "{position}");
            assert_eq!(next.halfmove_clock(), position.halfmove_clock() + 1);
            // It reads back like any other position: the castling rights
            // are as they were, and the player who passed is not in check.
            let written = next.to_string();
            assert_eq!(
                Position::from_fen(&written).as_ref(),
                Ok(&next),
                "{written}"
            );
            // A second pass brings back the position, less a chance to
            // capture en passant that has lapsed.
            let back = next.pass();
            if position.en_passant().is_none() {
                assert!(back.repeats(position), "{position}");
            }
            assert_eq!(back.fullmove_number(), position.fullmove_number() + 1);
            passed += 1;
        });
    }
    assert!(passed > 3_000, "only {passed} positions");
}

#[test]
fn a_player_with_only_pawns_and_a_king_has_no_pieces() {
    let pieces = |fen: &str| Position::from_fen(fen).unwrap().mover_has_pieces();
    assert!(pieces(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    ));
    // White has a knight and Black has pawns only.
    assert!(pieces("4k3/pppp4/8/8/8/8/PPPP4/1N2K3 w - - 0 1"));
    assert!(!pieces("4k3/pppp4/8/8/8/8/PPPP4/1N2K3 b - - 0 1"));
    assert!(!pieces("8/8/8/4k3/8/8/4P3/4K3 w - - 0 1"));
}
