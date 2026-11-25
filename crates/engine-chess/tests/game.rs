//! The interface of a game: the actions it accepts, the ones it refuses
//! without changing, and what a snapshot brings back.

use engine_chess::{
    Action, ActionError, Color, DrawClaim, DrawReason, Ending, Game, Move, Outcome, Position,
    Revision, SNAPSHOT_VERSION, Snapshot, SnapshotError,
};

/// Both knights out and back: four half-moves that restore the position.
const SHUFFLE: &str = "g1f3 g8f6 f3g1 f6g8";
const FOOLS_MATE: &str = "f2f3 e7e5 g2g4 d8h4";
/// Ninety-nine half-moves without a capture or pawn move, White to move.
const NEARLY_FIFTY: &str = "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 99 60";

fn mv(text: &str) -> Move {
    Move::parse(text).unwrap_or_else(|| panic!("`{text}` is not a move"))
}

fn play(game: &mut Game, moves: &str) {
    for text in moves.split_whitespace() {
        let player = game.position().side_to_move();
        game.apply(game.revision(), player, Action::Move(mv(text)))
            .unwrap_or_else(|error| panic!("{text} was refused: {error}"));
    }
}

fn game_after(moves: &str) -> Game {
    let mut game = Game::new(1, Position::initial());
    play(&mut game, moves);
    game
}

fn claim(claim: DrawClaim, intended: Option<&str>) -> Action {
    Action::ClaimDraw {
        claim,
        intended: intended.map(mv),
    }
}

#[test]
fn an_accepted_action_advances_the_revision() {
    let mut game = Game::new(7, Position::initial());
    let before = game.revision();
    assert_eq!(
        before,
        Revision {
            session: 7,
            actions: 0
        }
    );

    let transition = game
        .apply(before, Color::White, Action::Move(mv("e2e4")))
        .unwrap();

    assert_eq!(
        transition.revision,
        Revision {
            session: 7,
            actions: 1
        }
    );
    assert_eq!(transition.revision, game.revision());
    assert_eq!(transition.outcome, None);
    assert_eq!(game.moves(), [mv("e2e4")]);
    assert_eq!(game.positions().len(), 2);
}

#[test]
fn a_refused_action_changes_nothing() {
    let game = game_after("e2e4");
    let current = game.revision();
    let earlier = Revision {
        session: current.session,
        actions: 0,
    };
    let another_game = Revision {
        session: current.session + 1,
        ..current
    };
    let refusals = [
        (
            earlier,
            Color::Black,
            Action::Move(mv("e7e5")),
            ActionError::Stale,
        ),
        (
            another_game,
            Color::Black,
            Action::Move(mv("e7e5")),
            ActionError::Stale,
        ),
        (
            current,
            Color::White,
            Action::Move(mv("d2d4")),
            ActionError::OutOfTurn,
        ),
        (
            current,
            Color::White,
            claim(DrawClaim::Threefold, None),
            ActionError::OutOfTurn,
        ),
        (
            current,
            Color::Black,
            Action::Move(mv("e7e4")),
            ActionError::IllegalMove,
        ),
        (
            current,
            Color::Black,
            claim(DrawClaim::Threefold, None),
            ActionError::ClaimNotValid,
        ),
        (
            current,
            Color::Black,
            claim(DrawClaim::FiftyMoves, Some("g8f6")),
            ActionError::ClaimNotValid,
        ),
        (
            current,
            Color::Black,
            claim(DrawClaim::Threefold, Some("e7e4")),
            ActionError::ClaimNotValid,
        ),
    ];

    for (revision, player, action, error) in refusals {
        let mut attempt = game.clone();
        assert_eq!(
            attempt.apply(revision, player, action),
            Err(error),
            "{action:?}"
        );
        assert_eq!(attempt, game, "{action:?} changed the game");
    }
}

#[test]
fn a_finished_game_accepts_nothing_more() {
    let game = game_after(FOOLS_MATE);
    assert_eq!(
        game.outcome(),
        Some(Outcome::Checkmate {
            winner: Color::Black
        })
    );
    assert!(game.legal_moves().is_empty());
    assert!(!game.can_claim(DrawClaim::Threefold));

    for (player, action) in [
        (Color::White, Action::Move(mv("e1f2"))),
        (Color::White, claim(DrawClaim::FiftyMoves, None)),
        (Color::White, Action::Resign),
        (Color::Black, Action::Resign),
    ] {
        let mut attempt = game.clone();
        assert_eq!(
            attempt.apply(game.revision(), player, action),
            Err(ActionError::GameOver)
        );
        assert_eq!(attempt, game);
    }
}

#[test]
fn either_player_may_resign_whoever_is_to_move() {
    for (resigner, winner) in [(Color::White, Color::Black), (Color::Black, Color::White)] {
        let mut game = game_after("e2e4");
        let position = game.position().clone();

        let transition = game
            .apply(game.revision(), resigner, Action::Resign)
            .unwrap();

        assert_eq!(transition.outcome, Some(Outcome::Resignation { winner }));
        assert_eq!(game.outcome(), transition.outcome);
        assert_eq!(*game.position(), position);
    }
}

#[test]
fn a_claimable_draw_does_not_end_the_game_until_it_is_claimed() {
    let mut game = game_after(&[SHUFFLE, SHUFFLE].join(" "));
    assert_eq!(game.outcome(), None);
    assert!(game.can_claim(DrawClaim::Threefold));

    // Playing on is allowed, and the claim lapses with the position.
    play(&mut game, "e2e4");
    assert_eq!(game.outcome(), None);
    assert!(!game.can_claim(DrawClaim::Threefold));
}

#[test]
fn a_claim_as_the_position_stands_draws_the_game() {
    let mut game = game_after(&[SHUFFLE, SHUFFLE].join(" "));

    let transition = game
        .apply(
            game.revision(),
            Color::White,
            claim(DrawClaim::Threefold, None),
        )
        .unwrap();

    assert_eq!(
        transition.outcome,
        Some(Outcome::Draw(DrawReason::ThreefoldClaim))
    );
    assert_eq!(game.outcome(), transition.outcome);
}

#[test]
fn a_claim_with_an_intended_move_draws_without_playing_the_move() {
    let mut game = game_after(&[SHUFFLE, "g1f3 g8f6 f3g1"].join(" "));
    let position = game.position().clone();
    assert!(!game.can_claim(DrawClaim::Threefold));

    let transition = game
        .apply(
            game.revision(),
            Color::Black,
            claim(DrawClaim::Threefold, Some("f6g8")),
        )
        .unwrap();

    assert_eq!(
        transition.outcome,
        Some(Outcome::Draw(DrawReason::ThreefoldClaim))
    );
    assert_eq!(*game.position(), position);
    assert_eq!(game.moves().len(), 7);
}

#[test]
fn the_fifty_move_claim_needs_a_hundredth_quiet_half_move() {
    let start = || Game::new(1, Position::from_fen(NEARLY_FIFTY).unwrap());

    for (action, expected) in [
        (
            claim(DrawClaim::FiftyMoves, None),
            Err(ActionError::ClaimNotValid),
        ),
        (
            claim(DrawClaim::FiftyMoves, Some("e2e3")),
            Err(ActionError::ClaimNotValid),
        ),
        (
            claim(DrawClaim::FiftyMoves, Some("a1a7")),
            Err(ActionError::ClaimNotValid),
        ),
        (
            claim(DrawClaim::FiftyMoves, Some("e1c1")),
            Ok(Some(Outcome::Draw(DrawReason::FiftyMoveClaim))),
        ),
    ] {
        let mut game = start();
        let outcome = game
            .apply(game.revision(), Color::White, action)
            .map(|transition| transition.outcome);
        assert_eq!(outcome, expected, "{action:?}");
    }

    let mut game = start();
    play(&mut game, "a1b1");
    assert_eq!(game.outcome(), None);
    assert!(game.can_claim(DrawClaim::FiftyMoves));
}

#[test]
fn a_snapshot_restores_the_game_with_its_history() {
    let game = game_after(&[SHUFFLE, "g1f3 g8f6 f3g1"].join(" "));
    let snapshot = game.snapshot();
    assert_eq!(snapshot.version, SNAPSHOT_VERSION);
    assert_eq!(snapshot.start, Position::initial().to_string());

    let restored = Game::restore(9, &snapshot).unwrap();

    assert_eq!(restored.positions(), game.positions());
    assert_eq!(restored.moves(), game.moves());
    assert_eq!(restored.outcome(), game.outcome());
    assert_eq!(restored.legal_moves(), game.legal_moves());
    assert_eq!(
        restored.revision(),
        Revision {
            session: 9,
            actions: 7
        }
    );
    // The earlier positions came back too: the next repeat is still the third.
    assert!(restored.can_claim_with(DrawClaim::Threefold, mv("f6g8")));
    assert_eq!(restored.snapshot(), snapshot);
}

#[test]
fn a_snapshot_keeps_how_the_game_ended() {
    let mut resigned = game_after("e2e4");
    resigned
        .apply(resigned.revision(), Color::White, Action::Resign)
        .unwrap();
    let mut claimed = game_after(&[SHUFFLE, "g1f3 g8f6 f3g1"].join(" "));
    claimed
        .apply(
            claimed.revision(),
            Color::Black,
            claim(DrawClaim::Threefold, Some("f6g8")),
        )
        .unwrap();
    let mated = game_after(FOOLS_MATE);

    assert_eq!(
        resigned.snapshot().ending,
        Some(Ending::Resigned(Color::White))
    );
    assert_eq!(mated.snapshot().ending, None);
    for game in [resigned, claimed, mated] {
        let restored = Game::restore(1, &game.snapshot()).unwrap();
        assert_eq!(restored, game);
    }
}

#[test]
fn a_snapshot_that_could_not_have_happened_is_refused() {
    let good = game_after("e2e4 e7e5").snapshot();
    let after_mate = game_after(FOOLS_MATE).snapshot();
    let refused = [
        (
            Snapshot {
                version: SNAPSHOT_VERSION + 1,
                ..good.clone()
            },
            SnapshotError::Version(SNAPSHOT_VERSION + 1),
        ),
        (
            Snapshot {
                moves: vec![mv("e2e4"), mv("e7e5"), mv("e4e5")],
                ..good.clone()
            },
            SnapshotError::Move(2),
        ),
        (
            Snapshot {
                moves: [after_mate.moves.clone(), vec![mv("e1f2")]].concat(),
                ..after_mate.clone()
            },
            SnapshotError::Move(4),
        ),
        (
            Snapshot {
                ending: Some(Ending::DrawClaimed {
                    claim: DrawClaim::Threefold,
                    intended: None,
                }),
                ..good.clone()
            },
            SnapshotError::Ending,
        ),
        (
            Snapshot {
                ending: Some(Ending::Resigned(Color::White)),
                ..after_mate
            },
            SnapshotError::Ending,
        ),
    ];
    for (snapshot, error) in refused {
        assert_eq!(Game::restore(1, &snapshot), Err(error), "{snapshot:?}");
    }

    let bad_start = Snapshot {
        start: "8/8/8/8/8/8/8/8 w - - 0 1".to_string(),
        ..good
    };
    assert!(matches!(
        Game::restore(1, &bad_start),
        Err(SnapshotError::Start(_))
    ));
}

#[test]
fn the_same_actions_give_the_same_game() {
    let moves = "e2e4 c7c5 g1f3 d7d6 d2d4 c5d4 f3d4 g8f6 b1c3 a7a6";
    let first = game_after(moves);
    let second = game_after(moves);

    assert_eq!(first, second);
    assert_eq!(first.position().hash(), second.position().hash());
    assert_eq!(
        first.position().to_string(),
        "rnbqkb1r/1p2pppp/p2p1n2/8/3NP3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 6"
    );
}
