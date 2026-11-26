//! The computer opponent through its public interface: what a host that runs
//! a search may rely on.

use engine_chess::search::{Level, MATE, MAX_DEPTH, MAX_PLAY_NODES, SearchJob, Settings};
use engine_chess::{Action, ActionError, DrawClaim, DrawReason, Game, Move, Outcome, Position};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const MIDDLEGAME: &str = "r1bq1rk1/pp2ppbp/2np1np1/8/3NP3/2N1BP2/PPPQ2PP/R3KB1R w KQ - 3 9";
const ROOK_ENDGAME: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const PAWN_ENDGAME: &str = "8/8/8/4k3/8/8/4P3/4K3 w - - 0 1";
/// Eleven queens among twenty-nine pieces, with captures on every side: a
/// search that followed every capture to the end would be long in coming
/// back.
const ALL_CAPTURES: &str = "q2k2q1/2nqn2b/1n1P1n1b/2rnr2Q/1NQ1QN1Q/3Q3B/2RQR2B/Q2K2Q1 w - - 0 1";
const BACK_RANK: &str = "6k1/5ppp/8/8/8/8/8/R3K3 w Q - 0 1";
const ROOK_MATE: &str = "7k/8/5K2/8/8/8/8/6R1 w - - 0 1";
const STALEMATE_TRAP: &str = "k7/8/1Q6/8/8/8/8/7K w - - 0 1";
const WHITE_LACKS_A_QUEEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB1KBNR w KQkq - 0 1";
const BLACK_LACKS_A_QUEEN: &str = "rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
/// More queens than any game could reach. The rules accept such positions,
/// so a search has to cope with them.
const QUEENS_AGAINST_A_PAWN: &str =
    "qqqqqqqk/qPqqqqqq/qqqqqqqq/qqqqqqqq/qqqq4/8/3p2PP/2Q3QK w - - 0 1";
const QUEENS_ROUND_A_KING: &str =
    "qqq1qqqq/qqqqqqq1/q1q1qqqq/1qqkqqqq/qqqqqqqq/8/p5PP/QQ3NQK w - - 0 1";
/// Any quiet move by Black lets a draw be claimed, so the rest of Black's
/// moves are searched against a value of nothing.
const QUEENS_NEARLY_DRAWN: &str =
    "5bnk/5ppp/8/QQQQQQQQ/QQQQQQQQ/QQQQQQQQ/QQQQQQQQ/KQQQQQQQ b - - 99 60";
/// Thirty-two queens, several of which could mate at once.
const QUEENS_WITH_A_MATE: &str = "6nk/6pp/8/Q7/QQQQQQQQ/QQQQQQQQ/QQQQQQQQ/QQQQKQQQ w - - 0 1";
/// A knight out and back for each player: four half-moves that restore the
/// position.
const SHUFFLE: &str = "g1f3 g8f6 f3g1 f6g8";

fn mv(text: &str) -> Move {
    Move::parse(text).unwrap_or_else(|| panic!("`{text}` is not a move"))
}

fn game(fen: &str, played: &str) -> Game {
    let mut game = Game::new(1, Position::from_fen(fen).unwrap());
    for text in played.split_whitespace() {
        let player = game.position().side_to_move();
        game.apply(game.revision(), player, Action::Move(mv(text)))
            .unwrap_or_else(|error| panic!("{text} was refused: {error}"));
    }
    game
}

/// Runs a search to its decision and takes it in a copy of the game.
fn decide(game: &Game, settings: Settings, seed: u64) -> (Action, Game) {
    let mut job = SearchJob::new(game, settings, seed).expect("the game is not over");
    job.run(u64::MAX);
    let decision = *job.decision().expect("the search finished");
    let mut after = game.clone();
    let player = game.position().side_to_move();
    after
        .apply(decision.revision, player, decision.action)
        .unwrap_or_else(|error| panic!("{:?} was refused: {error}", decision.action));
    (decision.action, after)
}

#[test]
fn a_finished_game_has_nothing_to_search() {
    let mated = game(START, "f2f3 e7e5 g2g4 d8h4");
    assert!(mated.outcome().is_some());
    assert!(SearchJob::new(&mated, Level::Normal.settings(), 0).is_none());
}

#[test]
fn every_level_plays_a_legal_move_within_its_budget() {
    for level in Level::ALL {
        let settings = level.settings();
        for fen in [START, KIWIPETE, MIDDLEGAME, ROOK_ENDGAME, PAWN_ENDGAME] {
            let game = game(fen, "");
            let mut job = SearchJob::new(&game, settings, 11).unwrap();
            let memory = job.memory_bytes();
            job.run(u64::MAX);
            let decision = *job.decision().unwrap();

            assert!(decision.nodes <= settings.nodes, "{level:?} {fen}");
            assert!(decision.depth >= 1, "{level:?} {fen}");
            assert_eq!(decision.revision, game.revision());
            assert_eq!(job.memory_bytes(), memory, "{level:?} {fen}");
            let player = game.position().side_to_move();
            let mut after = game.clone();
            assert!(
                after
                    .apply(decision.revision, player, decision.action)
                    .is_ok(),
                "{level:?} {fen}: {:?}",
                decision.action
            );
        }
    }
}

#[test]
fn the_memory_of_each_level_is_bounded() {
    let game = game(START, "");
    let megabyte = 1 << 20;
    for (level, most) in [
        (Level::Easy, megabyte / 4),
        (Level::Normal, 2 * megabyte),
        (Level::Hard, 17 * megabyte),
    ] {
        let job = SearchJob::new(&game, level.settings(), 0).unwrap();
        assert!(
            job.memory_bytes() <= most,
            "{level:?}: {}",
            job.memory_bytes()
        );
    }
}

#[test]
fn settings_beyond_the_limits_are_brought_back_to_them() {
    let game = game(KIWIPETE, "");
    let settings = Settings {
        depth: u8::MAX,
        nodes: 3_000,
        quiescence: u8::MAX,
        table_bits: u8::MAX,
        margin: u16::MAX,
    };
    let mut job = SearchJob::new(&game, settings, 0).unwrap();
    assert!(job.memory_bytes() <= 65 << 20);
    job.run(u64::MAX);
    assert_eq!(job.decision().unwrap().nodes, 3_000);

    // What a player asks for is also held to a number of positions, so that
    // no choice of settings makes a move take unbounded time or memory.
    let asked = Settings {
        nodes: u64::MAX,
        ..settings
    }
    .for_play();
    assert_eq!(asked.nodes, MAX_PLAY_NODES);
    let job = SearchJob::new(&game, asked, 0).unwrap();
    assert!(job.memory_bytes() <= 65 << 20);

    // Asked for a deeper search than there is, with positions to spare, a
    // search stops at the deepest. The pawns block each other and the kings
    // cannot pass them, so the deepest search is soon done.
    let blocked = self::game("8/8/8/k7/p7/P7/K7/8 w - - 0 1", "");
    let deep = Settings {
        depth: u8::MAX,
        nodes: u64::MAX,
        ..Level::Normal.settings()
    };
    let mut job = SearchJob::new(&blocked, deep, 0).unwrap();
    job.run(u64::MAX);
    assert_eq!(job.decision().unwrap().depth, MAX_DEPTH);

    // Asked for no depth at all, it searches one half-move.
    let shallow = Settings {
        depth: 0,
        ..Level::Easy.settings()
    };
    let mut job = SearchJob::new(&game, shallow, 0).unwrap();
    job.run(u64::MAX);
    assert_eq!(job.decision().unwrap().depth, 1);
}

#[test]
fn the_search_leaves_the_game_alone_and_its_decision_goes_stale() {
    // The job holds nothing of the game, which is free to change under it.
    let mut game = game(KIWIPETE, "");
    let mut job = SearchJob::new(&game, Level::Normal.settings(), 5).unwrap();
    job.run(u64::MAX);

    // The game moves on before the decision arrives.
    let decision = *job.decision().unwrap();
    let player = game.position().side_to_move();
    game.apply(game.revision(), player, Action::Move(mv("a2a3")))
        .unwrap();
    let late = game.clone();
    assert_eq!(
        game.apply(decision.revision, player, decision.action),
        Err(ActionError::Stale)
    );
    assert_eq!(game, late);
}

#[test]
fn a_search_may_be_stopped_before_it_starts_while_deepening_and_among_captures() {
    let game = game(KIWIPETE, "");
    let player = game.position().side_to_move();
    let legal = |action: Action| {
        let mut after = game.clone();
        after.apply(game.revision(), player, action).is_ok()
    };
    let mut job = SearchJob::new(&game, Level::Normal.settings(), 0).unwrap();

    // Before any step there is a move, though no value for it.
    let unstarted = job.best_so_far();
    assert_eq!((unstarted.depth, unstarted.score), (0, None));
    assert_eq!(unstarted.nodes, 0);
    assert!(legal(unstarted.action));

    // Among captures, past the full search.
    while !job.progress().quiescence {
        assert!(!job.is_finished(), "the search never followed a capture");
        job.step();
    }
    assert!(!job.is_finished() && job.decision().is_none());
    assert!(legal(job.best_so_far().action));

    // Between two depths.
    while job.progress().depth < 2 {
        assert!(!job.is_finished(), "the search never finished two depths");
        job.step();
    }
    let deepening = job.best_so_far();
    assert!(!job.is_finished());
    assert_eq!(deepening.depth, 2);
    assert!(deepening.score.is_some());
    assert!(legal(deepening.action));

    // Taking the decision early does not disturb the search.
    let mut undisturbed = SearchJob::new(&game, Level::Normal.settings(), 0).unwrap();
    undisturbed.run(u64::MAX);
    job.run(u64::MAX);
    assert_eq!(job.decision(), undisturbed.decision());
}

#[test]
fn a_search_may_be_handed_to_another_thread() {
    let game = game(KIWIPETE, "");
    let mut job = SearchJob::new(&game, Level::Normal.settings(), 2).unwrap();
    let mut here = job.clone();
    here.run(u64::MAX);

    let elsewhere = std::thread::spawn(move || {
        job.run(u64::MAX);
        *job.decision().unwrap()
    });
    assert_eq!(elsewhere.join().unwrap(), *here.decision().unwrap());
}

#[test]
fn the_same_game_settings_seed_and_steps_give_the_same_decision() {
    let game = game(START, "e2e4 e7e5");
    for level in Level::ALL {
        let decide = |seed, steps| {
            let mut job = SearchJob::new(&game, level.settings(), seed).unwrap();
            job.run(steps);
            job.best_so_far()
        };
        for steps in [0, 50, 2_000, u64::MAX] {
            assert_eq!(decide(9, steps), decide(9, steps), "{level:?} {steps}");
        }
    }
}

#[test]
fn the_seed_chooses_between_moves_that_are_nearly_as_good() {
    let game = game(START, "");
    let easy = Level::Easy.settings();
    let mut chosen: Vec<Action> = (0..40).map(|seed| decide(&game, easy, seed).0).collect();
    chosen.sort_by_key(|action| format!("{action:?}"));
    chosen.dedup();
    assert!(chosen.len() >= 3, "{chosen:?}");

    // One move wins a queen, so there is nothing to choose between.
    let game = self::game("4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1", "");
    for seed in 0..40 {
        let mut job = SearchJob::new(&game, easy, seed).unwrap();
        job.run(u64::MAX);
        let decision = job.decision().unwrap();
        assert_eq!(decision.action, Action::Move(mv("e4d5")));
        assert_eq!((decision.candidates, decision.seed), (1, seed));
    }
}

#[test]
fn a_mate_in_one_is_played_at_every_level() {
    let game = game(BACK_RANK, "");
    for level in Level::ALL {
        let (action, after) = decide(&game, level.settings(), 4);
        assert_eq!(action, Action::Move(mv("a1a8")), "{level:?}");
        assert!(matches!(after.outcome(), Some(Outcome::Checkmate { .. })));
    }
}

#[test]
fn a_mate_with_the_hundredth_quiet_half_move_is_a_mate_and_not_a_draw() {
    // Any other quiet move would let Black claim the fifty-move draw, and
    // White could claim it now by naming one.
    let game = game("6k1/5ppp/8/8/8/8/8/R3K3 w Q - 99 60", "");
    assert!(game.can_claim_with(DrawClaim::FiftyMoves, mv("a1a2")));
    for level in Level::ALL {
        let (action, after) = decide(&game, level.settings(), 0);
        assert_eq!(action, Action::Move(mv("a1a8")), "{level:?}");
        assert!(matches!(after.outcome(), Some(Outcome::Checkmate { .. })));
    }
}

#[test]
fn a_mate_in_two_is_found_and_scored_by_its_distance() {
    let game = game(ROOK_MATE, "");
    for level in [Level::Normal, Level::Hard] {
        let mut job = SearchJob::new(&game, level.settings(), 0).unwrap();
        job.run(u64::MAX);
        let decision = job.decision().unwrap();
        assert_eq!(decision.action, Action::Move(mv("f6f7")), "{level:?}");
        // Three half-moves: the king steps up, the other king steps aside,
        // and the rook mates.
        assert_eq!(decision.score, Some(MATE - 3), "{level:?}");
    }
}

#[test]
fn a_player_who_is_winning_does_not_stalemate() {
    let game = game(STALEMATE_TRAP, "");
    for level in Level::ALL {
        for seed in 0..5 {
            let (action, after) = decide(&game, level.settings(), seed);
            assert_ne!(
                after.outcome(),
                Some(Outcome::Draw(DrawReason::Stalemate)),
                "{level:?}: {action:?}"
            );
        }
    }
}

#[test]
fn a_player_who_is_losing_claims_a_draw_that_is_there() {
    // The position has now occurred three times, with White to move.
    let repeated = [SHUFFLE, SHUFFLE].join(" ");
    let game = game(WHITE_LACKS_A_QUEEN, &repeated);
    assert!(game.can_claim(DrawClaim::Threefold));
    for level in Level::ALL {
        let (action, after) = decide(&game, level.settings(), 0);
        assert_eq!(
            action,
            Action::ClaimDraw {
                claim: DrawClaim::Threefold,
                intended: None
            },
            "{level:?}"
        );
        assert_eq!(
            after.outcome(),
            Some(Outcome::Draw(DrawReason::ThreefoldClaim))
        );
    }
}

#[test]
fn a_player_who_is_losing_claims_by_naming_the_move_that_repeats() {
    // Black's knight can go home and bring the position about a third time.
    let nearly = [SHUFFLE, "g1f3 g8f6 f3g1"].join(" ");
    let game = game(BLACK_LACKS_A_QUEEN, &nearly);
    assert!(!game.can_claim(DrawClaim::Threefold));
    for level in Level::ALL {
        let (action, _) = decide(&game, level.settings(), 0);
        assert_eq!(
            action,
            Action::ClaimDraw {
                claim: DrawClaim::Threefold,
                intended: Some(mv("f6g8"))
            },
            "{level:?}"
        );
    }
}

#[test]
fn a_decision_always_names_the_move_it_would_play() {
    // With a claim, the move is the one it would play without the claim.
    let claimed = game(WHITE_LACKS_A_QUEEN, &[SHUFFLE, SHUFFLE].join(" "));
    // With a move, it is that move.
    let played = game(KIWIPETE, "");
    for (game, claims) in [(&claimed, true), (&played, false)] {
        let mut job = SearchJob::new(game, Level::Normal.settings(), 0).unwrap();
        // Before any step, and after the last.
        for steps in [0, u64::MAX] {
            job.run(steps);
            let decision = job.best_so_far();
            assert!(game.legal_moves().contains(&decision.choice));
            // Nothing is claimed before anything has been searched.
            if !claims || steps == 0 {
                assert_eq!(decision.action, Action::Move(decision.choice));
            }
        }
        let claimed = matches!(job.decision().unwrap().action, Action::ClaimDraw { .. });
        assert_eq!(claimed, claims);
    }
}

#[test]
fn a_player_who_is_winning_plays_on_and_does_not_repeat() {
    // The same claim is open to Black, who is a queen ahead.
    let nearly = [SHUFFLE, "g1f3 g8f6 f3g1"].join(" ");
    let game = game(WHITE_LACKS_A_QUEEN, &nearly);
    assert!(game.can_claim_with(DrawClaim::Threefold, mv("f6g8")));
    for level in Level::ALL {
        for seed in 0..5 {
            let (action, after) = decide(&game, level.settings(), seed);
            assert!(matches!(action, Action::Move(_)), "{level:?}: {action:?}");
            // White is not handed the claim either.
            assert_ne!(action, Action::Move(mv("f6g8")), "{level:?}");
            assert!(!after.can_claim(DrawClaim::Threefold), "{level:?}");
        }
    }
}

#[test]
fn the_fifty_move_rule_is_claimed_by_the_loser_and_avoided_by_the_winner() {
    // Ninety-nine quiet half-moves have passed and White is a rook ahead.
    let winning = game("4k3/p7/8/8/8/8/4P3/R3K3 w Q - 99 60", "");
    let losing = game("4k3/p7/8/8/8/8/4P3/R3K3 b Q - 99 60", "");
    for level in Level::ALL {
        // A capture or a pawn move starts the count again.
        let (action, after) = decide(&winning, level.settings(), 0);
        assert!(matches!(action, Action::Move(_)), "{level:?}: {action:?}");
        assert_eq!(
            after.position().halfmove_clock(),
            0,
            "{level:?}: {action:?}"
        );

        let (action, after) = decide(&losing, level.settings(), 0);
        assert!(
            matches!(
                action,
                Action::ClaimDraw {
                    claim: DrawClaim::FiftyMoves,
                    intended: Some(_)
                }
            ),
            "{level:?}: {action:?}"
        );
        assert_eq!(
            after.outcome(),
            Some(Outcome::Draw(DrawReason::FiftyMoveClaim))
        );
    }
}

#[test]
fn a_board_full_of_queens_is_searched_like_any_other() {
    for level in Level::ALL {
        // A few thousand positions are enough to look two half-moves deep,
        // where material this lopsided is met with either player to move.
        let settings = Settings {
            nodes: level.settings().nodes.min(20_000),
            ..level.settings()
        };
        // The search answers, and the game accepts what it answers.
        for fen in [
            QUEENS_AGAINST_A_PAWN,
            QUEENS_ROUND_A_KING,
            QUEENS_NEARLY_DRAWN,
        ] {
            decide(&game(fen, ""), settings, 0);
        }

        // However much material stands on the board, it looks like less
        // than a forced mate, so the mate is played.
        let game = game(QUEENS_WITH_A_MATE, "");
        let mut job = SearchJob::new(&game, settings, 0).unwrap();
        job.run(u64::MAX);
        let decision = *job.decision().unwrap();
        assert_eq!(decision.score, Some(MATE - 1), "{level:?}");
        let mut after = game.clone();
        after
            .apply(
                decision.revision,
                game.position().side_to_move(),
                decision.action,
            )
            .unwrap();
        assert!(
            matches!(after.outcome(), Some(Outcome::Checkmate { .. })),
            "{level:?}: {:?}",
            decision.action
        );
    }
}

#[test]
fn a_position_full_of_captures_stays_within_every_limit() {
    let game = game(ALL_CAPTURES, "");
    let player = game.position().side_to_move();
    for settings in [
        Level::Normal.settings(),
        Settings {
            depth: u8::MAX,
            nodes: 60_000,
            quiescence: u8::MAX,
            table_bits: 12,
            margin: 0,
        },
    ] {
        let mut job = SearchJob::new(&game, settings, 0).unwrap();
        let memory = job.memory_bytes();
        let mut deepest = 0;
        while !job.is_finished() {
            job.step();
            deepest = deepest.max(job.progress().ply);
        }
        let decision = *job.decision().unwrap();
        assert!(decision.nodes <= settings.nodes);
        assert!(deepest >= 9, "the captures were followed: {deepest}");
        assert_eq!(job.memory_bytes(), memory);
        let mut after = game.clone();
        assert!(
            after
                .apply(decision.revision, player, decision.action)
                .is_ok()
        );
    }

    // With fewer positions than there are moves to try, not even one
    // half-move is searched in full, and the search still answers with a
    // legal move.
    let few = Settings {
        nodes: 30,
        ..Level::Normal.settings()
    };
    assert!(game.legal_moves().len() > 30);
    let mut job = SearchJob::new(&game, few, 0).unwrap();
    job.run(u64::MAX);
    let decision = *job.decision().unwrap();
    assert_eq!((decision.nodes, decision.depth), (few.nodes, 0));
    let mut after = game.clone();
    assert!(
        after
            .apply(decision.revision, player, decision.action)
            .is_ok()
    );
}
