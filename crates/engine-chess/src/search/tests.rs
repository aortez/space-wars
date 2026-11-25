//! The search against a plain one that tries every move and skips nothing.
//!
//! Skipping lines, ordering moves, narrow windows and the table are all
//! meant to save work without changing the answer, so the two must agree
//! exactly. Two things do change the answer by design, and each is switched
//! off where it would: an entry the table kept from a deeper search, and a
//! repeated position met again by another path while the table is in use.
//!
//! Two more give up on lines without searching them in full: a player may
//! pass to show a position good enough, and moves tried late are searched
//! less deeply at first. Nothing can show their answers right. The tests
//! switch them off for the comparison, and test apart from that when each
//! may happen, that a search in progress never breaks those conditions, and
//! that neither changes anything where it may not happen. Whether they make
//! the search play better is for games to say: see `REFERENCE.md`.

use std::collections::HashMap;

use super::eval::evaluate;
use super::exchange::tests::plain_exchange;
use super::*;
use crate::{Color, Game, Position};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const ROOK_ENDGAME: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const PROMOTIONS: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const TACTICS: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const PAWN_ENDGAME: &str = "8/8/8/4k3/8/8/4P3/4K3 w - - 0 1";
const PAWN_RACE: &str = "8/P6k/8/8/8/8/p6K/8 w - - 0 1";
const EN_PASSANT: &str = "4k3/8/8/8/2pPp3/8/8/4K3 b - d3 0 1";
const ROOK_MATE: &str = "7k/8/5K2/8/8/8/8/6R1 w - - 0 1";
const BACK_RANK: &str = "6k1/5ppp/8/8/8/8/8/R3K3 w Q - 0 1";
const STALEMATE_TRAP: &str = "k7/8/1Q6/8/8/8/8/7K w - - 0 1";
/// Three quiet half-moves from the fifty-move rule.
const NEARLY_FIFTY: &str = "4k3/p7/8/8/8/8/4P3/R3K3 w Q - 97 60";
/// The rook mates with the hundredth quiet half-move.
const MATE_ON_THE_HUNDREDTH: &str = "6k1/5ppp/8/8/8/8/8/R3K3 w Q - 99 60";
/// Once Black's king steps to e7, promoting to a knight forks it and the
/// queen, where promoting to a queen wins nothing.
const KNIGHT_PROMOTION: &str = "8/q1P5/4k3/8/8/8/8/4K3 b - - 0 1";
const QUEEN_ENDGAME: &str = "8/8/8/3k4/8/8/8/KQ6 w - - 0 1";
const ROOK_AGAINST_KING: &str = "8/8/8/4k3/8/8/8/R3K3 w - - 0 1";
/// A rook checks on d8, is taken, and the other rook takes back with mate.
/// Captures after the check show the mate two half-moves before the full
/// search reaches it.
const DOUBLED_ROOKS: &str = "r5k1/5ppp/8/8/8/8/3R1PPP/3R2K1 w - - 0 1";
/// A knight out and back for each player: four half-moves that restore the
/// position.
const SHUFFLE: &str = "g1f3 g8f6 f3g1 f6g8";

/// The search with nothing that changes an answer, as in play but for that.
const WITHOUT_TABLE: Rules = Rules {
    deeper_entries: true,
    repetition: true,
    passing: false,
    reducing: false,
};
const WITH_TABLE: Rules = Rules {
    deeper_entries: false,
    repetition: false,
    ..WITHOUT_TABLE
};
/// Every use of the table allowed, as in play, and nothing else that changes
/// an answer.
const TABLE_AS_IN_PLAY: Rules = WITHOUT_TABLE;
/// Passing allowed and nothing else that gives up on a line.
const PASSING: Rules = Rules {
    passing: true,
    ..WITHOUT_TABLE
};
/// Late moves searched less deeply and nothing else that gives up on a line.
const REDUCING: Rules = Rules {
    reducing: true,
    ..WITHOUT_TABLE
};
/// The search as it runs in play.
const IN_PLAY: Rules = Rules {
    passing: true,
    reducing: true,
    ..WITHOUT_TABLE
};

fn settings(depth: u8, quiescence: u8, table_bits: u8, margin: u16) -> Settings {
    Settings {
        depth,
        nodes: u64::MAX,
        quiescence,
        table_bits,
        margin,
    }
}

fn game(fen: &str, played: &str) -> Game {
    let mut game = Game::new(1, Position::from_fen(fen).unwrap());
    for text in played.split_whitespace() {
        let player = game.position().side_to_move();
        let action = Action::Move(Move::parse(text).unwrap());
        game.apply(game.revision(), player, action).unwrap();
    }
    game
}

fn searched(game: &Game, settings: Settings, rules: Rules) -> SearchJob {
    let mut job = SearchJob::new(game, settings, 0).expect("the game is not over");
    job.rules = rules;
    job.run(u64::MAX);
    job
}

/// What the repetition rules compare: the pieces, the player to move and the
/// castling and en passant rights, as the first four fields of the FEN.
fn identity(position: &Position) -> String {
    let fen = position.to_string();
    fen.split(' ').take(4).collect::<Vec<_>>().join(" ")
}

/// Whether a move is followed past the full search: it captures or promotes
/// to a queen, does not promote to anything less, and does not lose material
/// in the exchange on its square.
fn tactical(position: &Position, mv: Move) -> bool {
    let mover = position.piece_at(mv.from).unwrap().kind;
    let en_passant = mover == PieceKind::Pawn && mv.from.file() != mv.to.file();
    let captures = position.piece_at(mv.to).is_some() || en_passant;
    let forcing = match mv.promotion {
        None => captures,
        Some(kind) => kind == PieceKind::Queen,
    };
    forcing && plain_exchange(position, mv) >= 0
}

/// The plain search.
struct Plain {
    quiescence: u8,
    repetition: bool,
    /// The positions of the game before the one searched, the latest first.
    history: Vec<String>,
}

impl Plain {
    fn new(game: &Game, settings: Settings, rules: Rules) -> Self {
        let (_, earlier) = game.positions().split_last().unwrap();
        Self {
            quiescence: settings.quiescence,
            repetition: rules.repetition,
            history: earlier.iter().rev().map(identity).collect(),
        }
    }

    /// The value of every legal move for the player to move in `root`.
    fn root_values(&self, root: &Position, depth: u8) -> Vec<(Move, i32)> {
        let mut line = vec![(root.clone(), identity(root))];
        root.successors()
            .into_iter()
            .map(|(mv, next)| (mv, self.value_after(&mut line, &next, depth - 1, 0)))
            .collect()
    }

    /// The value of moving to `next`, for the player who moves there.
    fn value_after(
        &self,
        line: &mut Vec<(Position, String)>,
        next: &Position,
        depth: u8,
        quiet: u8,
    ) -> i32 {
        line.push((next.clone(), identity(next)));
        let value = -self.value(line, depth, quiet);
        line.pop();
        value
    }

    /// The value of the last position of `line` for the player to move in
    /// it. The first position of `line` is the root.
    fn value(&self, line: &mut Vec<(Position, String)>, depth: u8, quiet: u8) -> i32 {
        let position = line.last().unwrap().0.clone();
        let mated = (line.len() - 1) as i32 - MATE;
        if position.has_dead_material() {
            return 0;
        }
        if self.repetition && self.repeated(line) {
            return 0;
        }
        let replies = position.successors();
        let is_mate = position.is_check() && replies.is_empty();
        if position.halfmove_clock() >= 100 {
            return if is_mate { mated } else { 0 };
        }

        let (followed, at_least): (Vec<&Position>, Option<i32>) = if depth > 0 {
            if replies.is_empty() {
                return if is_mate { mated } else { 0 };
            }
            (replies.iter().map(|(_, next)| next).collect(), None)
        } else if quiet >= self.quiescence {
            return if is_mate { mated } else { evaluate(&position) };
        } else if position.is_check() {
            if is_mate {
                return mated;
            }
            (replies.iter().map(|(_, next)| next).collect(), None)
        } else {
            let captures = replies
                .iter()
                .filter(|&&(mv, _)| tactical(&position, mv))
                .map(|(_, next)| next)
                .collect();
            (captures, Some(evaluate(&position)))
        };

        let (depth, quiet) = if depth > 0 {
            (depth - 1, 0)
        } else {
            (0, quiet + 1)
        };
        followed
            .into_iter()
            .map(|next| self.value_after(line, next, depth, quiet))
            .chain(at_least)
            .max()
            .expect("there is a move to follow or a standing value")
    }

    /// Whether the last position of `line` occurred earlier below the root,
    /// or twice at or before it.
    fn repeated(&self, line: &[(Position, String)]) -> bool {
        let ((_, position), before) = line.split_last().unwrap();
        let below_root = before.len() - 1;
        let on_the_way = before.iter().rev().map(|(_, earlier)| earlier);
        let mut at_or_before_root = 0;
        for (index, earlier) in on_the_way.chain(&self.history).enumerate() {
            if earlier != position {
                continue;
            }
            if index < below_root {
                return true;
            }
            at_or_before_root += 1;
            if at_or_before_root == 2 {
                return true;
            }
        }
        false
    }
}

/// What the plain search makes of one game, worked out once for each depth.
struct Expected<'a> {
    name: &'a str,
    game: &'a Game,
    plain: Plain,
    rules: Rules,
    by_depth: HashMap<u8, Vec<(Move, i32)>>,
}

impl<'a> Expected<'a> {
    fn new(name: &'a str, game: &'a Game, quiescence: u8, rules: Rules) -> Self {
        Self {
            name,
            game,
            plain: Plain::new(game, settings(1, quiescence, 0, 0), rules),
            rules,
            by_depth: HashMap::new(),
        }
    }

    fn at(&mut self, depth: u8) -> &[(Move, i32)] {
        self.by_depth
            .entry(depth)
            .or_insert_with(|| self.plain.root_values(self.game.position(), depth))
    }
}

/// Searches the game and checks every root move against the plain search.
fn assert_agrees(expected: &mut Expected, settings: Settings) {
    assert_eq!(settings.quiescence, expected.plain.quiescence);
    let job = searched(expected.game, settings, expected.rules);
    let decision = *job.decision().unwrap();
    let context = format!("{}, {settings:?}", expected.name);
    if decision.depth == 0 {
        assert_eq!(job.root_moves.len(), 1, "{context}: nothing was searched");
        return;
    }

    let expected = expected.at(decision.depth);
    let best = expected.iter().map(|&(_, value)| value).max().unwrap();
    let margin = if best.abs() >= MATE_BOUND {
        0
    } else {
        i32::from(settings.margin)
    };
    assert_eq!(decision.score, Some(best), "{context}: the best value");

    let mut good_enough = 0;
    for &(mv, value) in expected {
        let entry = job
            .root_moves
            .iter()
            .find(|entry| entry.mv == mv)
            .unwrap_or_else(|| panic!("{context}: {mv} was not searched"));
        if value >= best - margin {
            good_enough += 1;
            assert!(entry.exact, "{context}: {mv} should have its value");
        }
        if entry.exact {
            assert_eq!(entry.score, value, "{context}: the value of {mv}");
        } else {
            // A move shown to be too weak has only a ceiling.
            assert!(
                value <= entry.score && entry.score < best - margin,
                "{context}: {mv} is worth {value}, not at most {}",
                entry.score
            );
        }
    }
    assert_eq!(
        usize::from(decision.candidates),
        good_enough,
        "{context}: the moves to choose between"
    );
    let Action::Move(chosen) = decision.action else {
        return;
    };
    let chosen = expected.iter().find(|&&(mv, _)| mv == chosen).unwrap().1;
    assert!(chosen >= best - margin, "{context}: the chosen move");
}

/// Positions with the depth and the half-moves of captures to search them
/// at. The plain search tries everything, so the busy ones stay shallow.
const CASES: [(&str, &str, &str, u8, u8); 17] = [
    ("start", START, "", 3, 2),
    ("kiwipete", KIWIPETE, "", 2, 2),
    ("rook-endgame", ROOK_ENDGAME, "", 4, 3),
    ("promotions", PROMOTIONS, "", 2, 2),
    ("tactics", TACTICS, "", 2, 2),
    ("pawn-endgame", PAWN_ENDGAME, "", 5, 2),
    ("pawn-race", PAWN_RACE, "", 5, 3),
    ("en-passant", EN_PASSANT, "", 4, 2),
    ("rook-mate", ROOK_MATE, "", 4, 0),
    ("back-rank", BACK_RANK, "", 3, 2),
    ("stalemate-trap", STALEMATE_TRAP, "", 3, 1),
    ("nearly-fifty", NEARLY_FIFTY, "", 4, 1),
    ("mate-on-the-hundredth", MATE_ON_THE_HUNDREDTH, "", 2, 1),
    ("knight-promotion", KNIGHT_PROMOTION, "", 2, 3),
    ("queen-endgame", QUEEN_ENDGAME, "", 5, 1),
    ("rook-against-king", ROOK_AGAINST_KING, "", 5, 1),
    ("after-a-shuffle", START, SHUFFLE, 3, 1),
];

/// A position with few pieces, where the same position is reached by many
/// orders of moves and the table has the most to say.
const DEEP_CASE: (&str, &str, u8, u8) = ("pawn-endgame", PAWN_ENDGAME, 7, 2);

#[test]
fn the_search_agrees_with_the_plain_search() {
    for (name, fen, played, depth, quiescence) in CASES {
        let game = game(fen, played);
        let mut expected = Expected::new(name, &game, quiescence, WITHOUT_TABLE);
        for depth in 1..=depth {
            for margin in [0, 40, u16::MAX] {
                assert_agrees(&mut expected, settings(depth, quiescence, 0, margin));
            }
        }
    }
}

#[test]
fn the_table_does_not_change_the_answer() {
    for (name, fen, played, depth, quiescence) in CASES {
        let game = game(fen, played);
        let mut expected = Expected::new(name, &game, quiescence, WITH_TABLE);
        // Four entries are overwritten constantly; the larger table is not.
        for table_bits in [2, 16] {
            for margin in [0, u16::MAX] {
                assert_agrees(
                    &mut expected,
                    settings(depth, quiescence, table_bits, margin),
                );
            }
        }
    }
}

#[test]
fn the_table_does_not_change_the_answer_where_orders_of_moves_meet() {
    let (name, fen, depth, quiescence) = DEEP_CASE;
    let game = game(fen, "");
    let mut expected = Expected::new(name, &game, quiescence, WITH_TABLE);
    for table_bits in [6, 16] {
        assert_agrees(&mut expected, settings(depth, quiescence, table_bits, 0));
    }
}

#[test]
fn positions_already_repeated_in_the_game_are_draws() {
    // After two shuffles the starting position has occurred three times.
    // After one and a half, either player can bring it about again.
    for played in [
        [SHUFFLE, SHUFFLE].join(" "),
        [SHUFFLE, "g1f3 g8f6"].join(" "),
        [SHUFFLE, "g1f3 g8f6 f3g1"].join(" "),
    ] {
        let game = game(START, &played);
        let mut expected = Expected::new(&played, &game, 1, WITHOUT_TABLE);
        for depth in 1..=3 {
            assert_agrees(&mut expected, settings(depth, 1, 0, u16::MAX));
        }
    }
}

#[test]
fn without_quiescence_a_player_sees_only_to_the_depth() {
    for (name, fen, played, depth, _) in CASES {
        let game = game(fen, played);
        let mut expected = Expected::new(name, &game, 0, WITHOUT_TABLE);
        assert_agrees(&mut expected, settings(depth.min(3), 0, 0, 0));
    }
}

#[test]
fn a_forced_mate_is_found_at_its_true_distance() {
    // The number of half-moves to checkmate, the mating move included.
    for (fen, half_moves) in [(BACK_RANK, 1), (ROOK_MATE, 3), (DOUBLED_ROOKS, 3)] {
        let game = game(fen, "");
        let job = searched(&game, Level::Hard.settings(), WITHOUT_TABLE);
        let decision = job.decision().unwrap();
        assert_eq!(decision.score, Some(MATE - half_moves), "{fen}");
        // It stops when the mate is inside the full search, and not before:
        // a mate seen only among captures may not be the quickest.
        assert_eq!(i32::from(decision.depth), half_moves, "{fen}");
    }
}

#[test]
fn the_table_keeps_a_mate_at_its_distance_from_the_position() {
    // Found four half-moves from the root, a mate three half-moves on.
    let found = MATE - 7;
    let kept = to_table(found, 4);
    assert_eq!(kept, MATE - 3);
    // Met again two half-moves from the root, it is five half-moves away.
    assert_eq!(from_table(kept, 2), MATE - 5);
    assert_eq!(from_table(to_table(-found, 4), 2), 5 - MATE);
    // Other scores do not depend on where the position was met.
    for score in [0, 17, -980, MATE_BOUND - 1, 1 - MATE_BOUND] {
        assert_eq!(to_table(score, 9), score);
        assert_eq!(from_table(score, 9), score);
    }
}

#[test]
fn the_table_finds_the_same_mates_at_the_same_distance() {
    let in_play = TABLE_AS_IN_PLAY;
    for fen in [
        "8/8/8/4k3/8/8/7q/K7 b - - 0 1",
        "8/8/4k3/8/8/8/7q/K7 b - - 0 1",
        "8/8/8/8/3k4/8/7r/K7 b - - 0 1",
        "8/8/8/4k3/8/8/7r/K7 b - - 0 1",
        ROOK_MATE,
        DOUBLED_ROOKS,
    ] {
        let game = game(fen, "");
        let limits = settings(9, 4, 0, 0);
        let plain = searched(&game, limits, in_play);
        let with_table = Settings {
            table_bits: 12,
            ..limits
        };
        let tabled = searched(&game, with_table, in_play);
        let (plain, tabled) = (plain.decision().unwrap(), tabled.decision().unwrap());
        assert!(plain.score.unwrap() >= MATE_BOUND, "{fen}: {plain:?}");
        assert_eq!(tabled.score, plain.score, "{fen}");
        assert_eq!(tabled.depth, plain.depth, "{fen}");
    }
}

#[test]
fn moves_are_tried_most_promising_first() {
    let position = Position::from_fen("4k2r/pP5p/8/3q4/4P3/8/8/R3K2R w KQ - 0 1").unwrap();
    let mv = |text| Move::parse(text).unwrap();
    let killers = [Some(mv("a1a2")), Some(mv("h1h2"))];
    let mut history = History::new();
    for _ in 0..100 {
        history.record(Color::White, mv("a1b1"), 6_400, true);
        history.record(Color::White, mv("h1g1"), 6_400, false);
    }
    let favoured = Favoured::new(Some(mv("e1e2")), killers);
    let score = |text| order(&position, mv(text), favoured, &history);
    let expected = [
        ("e1e2", "the table's move"),
        ("e4d5", "a pawn takes the queen"),
        ("a1a7", "a rook takes a pawn"),
        ("b7b8q", "a pawn becomes a queen"),
        ("a1a2", "the first killer"),
        ("h1h2", "the second killer"),
        ("a1b1", "a quiet move that has always done well"),
        ("e1g1", "a quiet move of which nothing is known"),
        ("h1g1", "a quiet move that has always done badly"),
        ("b7b8n", "a pawn becomes a knight"),
        ("h1h7", "a rook takes a pawn that a rook defends"),
    ];
    for pair in expected.windows(2) {
        let [(first, what), (next, then)] = pair else {
            unreachable!()
        };
        assert!(score(first) > score(next), "{what} comes before {then}");
    }
}

#[test]
fn the_table_saves_work() {
    let in_play = TABLE_AS_IN_PLAY;
    // In the endgame the same positions come up by many orders of moves, so
    // the table should save most of the work. In a crowded middlegame at
    // these depths it saves only some.
    for (fen, depth, times) in [(START, 6, 1), (KIWIPETE, 4, 1), (PAWN_ENDGAME, 9, 4)] {
        let game = game(fen, "");
        let nodes = |table_bits| {
            let job = searched(&game, settings(depth, 4, table_bits, 0), in_play);
            let decision = job.decision().unwrap();
            assert_eq!(decision.depth, depth, "{fen}");
            decision.nodes
        };
        let (without, with) = (nodes(0), nodes(16));
        assert!(
            with * times < without,
            "{fen}: {with} positions with the table, {without} without"
        );
    }
}

#[test]
fn a_search_goes_on_until_its_positions_are_spent() {
    for fen in [START, KIWIPETE, TACTICS, ROOK_ENDGAME] {
        let game = game(fen, "");
        let settings = Settings {
            nodes: 20_000,
            ..Level::Normal.settings()
        };
        let mut job = SearchJob::new(&game, settings, 0).unwrap();
        job.run(u64::MAX);
        let decision = job.decision().unwrap();
        assert_eq!(decision.nodes, settings.nodes, "{fen}");
        assert!(decision.depth >= 2, "{fen}: several depths were searched");
        // The depth after the last one finished was begun and left off.
        assert_eq!(job.depth, decision.depth + 1, "{fen}");
    }
}

/// Checks the decision of a search stopped where `job` stands against the
/// plain search: it is made from the moves searched so far at the depth in
/// hand, or from the depth before if there are none yet.
fn assert_answers_from_what_it_searched(job: &SearchJob, expected: &mut Expected, context: &str) {
    let decision = job.best_so_far();
    let finished = job.finished.map_or(0, |finished| finished.depth);
    assert_eq!(decision.depth, finished, "{context}");

    let in_hand: Vec<Move> = job.root_moves[..job.index]
        .iter()
        .map(|entry| entry.mv)
        .collect();
    let (depth, searched) = if !in_hand.is_empty() {
        (job.depth, in_hand)
    } else if finished > 0 {
        let all = job.root_moves.iter().map(|entry| entry.mv).collect();
        (finished, all)
    } else {
        // Nothing has been searched: the move is the first to be tried.
        assert_eq!(decision.score, None, "{context}");
        assert_eq!(decision.choice, job.root_moves[0].mv, "{context}");
        return;
    };
    let context = format!("{context}, {} moves into depth {depth}", searched.len());
    let value = |expected: &mut Expected, mv: Move| {
        let values = expected.at(depth);
        values.iter().find(|&&(other, _)| other == mv).unwrap().1
    };
    let values: Vec<i32> = searched.iter().map(|&mv| value(expected, mv)).collect();
    let best = *values.iter().max().unwrap();
    let margin = if best.abs() >= MATE_BOUND {
        0
    } else {
        i32::from(job.settings.margin)
    };

    if let Some(at_least) = job.proved {
        // The move in hand has been shown good enough, and its value is
        // still being worked out.
        let mv = job.root_moves[job.index].mv;
        let value = value(expected, mv);
        assert!(value >= best - margin, "{context}: {mv} is worth {value}");
        assert!(
            (best - margin..=value).contains(&at_least),
            "{context}: {mv} is worth {value}, not at least {at_least}"
        );
        assert_eq!(decision.choice, mv, "{context}");
        assert_eq!(decision.score, Some(best.max(at_least)), "{context}");
        assert_eq!(decision.candidates, 1, "{context}");
        return;
    }
    assert_eq!(decision.score, Some(best), "{context}: the best value");
    let good_enough = values.iter().filter(|&&value| value >= best - margin);
    assert_eq!(
        usize::from(decision.candidates),
        good_enough.count(),
        "{context}: the moves to choose between"
    );
    let chosen = searched.iter().position(|&mv| mv == decision.choice);
    let chosen = chosen.unwrap_or_else(|| panic!("{context}: {} chosen", decision.choice));
    assert!(
        values[chosen] >= best - margin,
        "{context}: the chosen move"
    );
}

#[test]
fn a_search_stopped_after_any_step_answers_from_the_moves_it_has_searched() {
    let (mut partway, mut proved) = (0, 0);
    for (name, fen, played, depth, quiescence) in CASES {
        let game = game(fen, played);
        let mut expected = Expected::new(name, &game, quiescence, WITHOUT_TABLE);
        for margin in [0, 40] {
            let settings = settings(depth, quiescence, 0, margin);
            let mut job = SearchJob::new(&game, settings, 5).unwrap();
            job.rules = WITHOUT_TABLE;
            let mut last = None;
            loop {
                // The answer can change only when the search has got
                // further with the moves of the root.
                let stands = (job.depth, job.index, job.proved, job.best_so_far().choice);
                if last != Some(stands) {
                    let context = format!("{name}, margin {margin}, {} steps", job.nodes);
                    assert_answers_from_what_it_searched(&job, &mut expected, &context);
                    let unfinished = job
                        .finished
                        .is_none_or(|finished| finished.depth < job.depth);
                    partway += u32::from(unfinished && job.index > 0);
                    proved += u32::from(job.proved.is_some());
                    last = Some(stands);
                }
                if job.is_finished() {
                    break;
                }
                job.step();
            }
        }
    }
    assert!(partway > 500, "only {partway} answers from a depth in hand");
    assert!(
        proved > 50,
        "only {proved} from a move with its value unfinished"
    );
}

#[test]
fn the_evaluation_is_the_same_from_either_side() {
    fn mirrored(fen: &str) -> String {
        let fields: Vec<&str> = fen.split(' ').collect();
        let swap_case = |text: &str| -> String {
            text.chars()
                .map(|c| {
                    if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect()
        };
        let ranks: Vec<String> = fields[0].split('/').rev().map(swap_case).collect();
        let side = if fields[1] == "w" { "b" } else { "w" };
        let mut castling: Vec<char> = swap_case(fields[2]).chars().collect();
        castling.sort_by_key(|&c| "KQkq-".find(c));
        let en_passant = match fields[3].as_bytes() {
            [file, b'3'] => format!("{}6", *file as char),
            [file, b'6'] => format!("{}3", *file as char),
            _ => "-".to_string(),
        };
        format!(
            "{} {side} {} {en_passant} {} {}",
            ranks.join("/"),
            castling.into_iter().collect::<String>(),
            fields[4],
            fields[5]
        )
    }

    for (name, fen, ..) in CASES {
        let position = Position::from_fen(fen).unwrap();
        let other = Position::from_fen(&mirrored(fen)).unwrap();
        assert_eq!(evaluate(&position), evaluate(&other), "{name}");
    }
}

#[test]
fn the_evaluation_counts_material_and_placement() {
    let value = |fen: &str| evaluate(&Position::from_fen(fen).unwrap());
    assert_eq!(value(START), 0);
    // White is without the queen, which stood on a square worth -5.
    let no_queen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB1KBNR w KQkq - 0 1";
    assert_eq!(value(no_queen), -975);
    assert_eq!(value(&no_queen.replace(" w ", " b ")), 975);

    // One piece besides the kings, which stand alike and cancel out: its
    // value and the bonus for its square, read from its owner's side.
    // A lone pawn is passed, which on its fourth rank is worth 20, and a
    // lone rook stands on an open file, which is worth 9.
    for (board, worth) in [
        ("4k3/8/8/8/4P3/8/8/4K3", 100 + 10 + 20),
        ("4k3/8/8/8/3N4/8/8/4K3", 330 + 8),
        ("4k3/8/8/8/8/8/8/2B1K3", 350 - 12),
        ("4k3/8/8/8/8/8/8/3RK3", 520 + 2 + 9),
        ("4k3/8/8/8/8/8/8/3QK3", 980 - 5),
        ("4k3/8/8/4p3/8/8/8/4K3", -(100 + 10 + 20)),
        ("4k3/8/8/3n4/8/8/8/4K3", -(330 + 8)),
        ("2b1k3/8/8/8/8/8/8/4K3", -(350 - 12)),
        ("3rk3/8/8/8/8/8/8/4K3", -(520 + 2 + 9)),
        ("3qk3/8/8/8/8/8/8/4K3", -(980 - 5)),
    ] {
        assert_eq!(value(&format!("{board} w - - 0 1")), worth, "{board}");
        assert_eq!(value(&format!("{board} b - - 0 1")), -worth, "{board}");
    }

    // The king has changed places with the knight on the g-file, then with
    // the bishop on the c-file. Either castled square is worth 30 to it, on
    // White's side of the board as on Black's.
    for (white, black, gain) in [("RNBQNBKR", "rnbqnbkr", 34), ("RNKQBBNR", "rnkqbbnr", 38)] {
        let for_white = format!("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/{white} w kq - 0 1");
        let for_black = format!("{black}/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQ - 0 1");
        assert_eq!(value(&for_white), gain, "{for_white}");
        assert_eq!(value(&for_black), gain, "{for_black}");
    }
}

#[test]
fn the_evaluation_counts_what_pawns_and_rooks_are_worth_beyond_their_placement() {
    let value = |board: &str| evaluate(&Position::from_fen(&format!("{board} w - - 0 1")).unwrap());
    // The kings stand alike and cancel out unless a case says otherwise.
    for (what, board, worth) in [
        (
            "a passed pawn on its fifth rank",
            "4k3/8/8/4P3/8/8/8/4K3",
            100 + 8 + 40,
        ),
        (
            "a pawn ahead on the next file stops both from being passed",
            "4k3/8/3p4/4P3/8/8/8/4K3",
            (100 + 8) - (100 + 5),
        ),
        (
            "a pawn beside it on the next file does not",
            "4k3/8/8/3pP3/8/8/8/4K3",
            (100 + 8 + 40) - (100 + 10 + 20),
        ),
        (
            "of two pawns on a file the rear one is doubled and only the front one passed",
            "4k3/8/8/8/4P3/4P3/8/4K3",
            (100 + 5 - 11) + (100 + 10 + 20),
        ),
        (
            "a rook behind its own pawn is on no open file",
            "4k3/8/8/8/8/8/3P4/3RK3",
            (100 - 24) + (520 + 2),
        ),
        (
            "nor is one that faces a pawn of the other player",
            "4k3/3p4/8/8/8/8/8/3RK3",
            (520 + 2) - (100 - 24),
        ),
        (
            "a rook on the seventh rank with the king on the eighth",
            "4k3/R7/8/8/8/8/8/4K3",
            520 - 5 + 9 + 17,
        ),
        (
            "or with a pawn beside it, which also closes no file to it",
            "8/R6p/4k3/8/8/8/8/4K3",
            (520 - 5 + 9 + 17) - (100 - 6) - 1,
        ),
        (
            "with neither, the seventh rank is worth nothing, and the king stands a point better",
            "8/R7/4k3/8/8/8/8/4K3",
            (520 - 5 + 9) - 1,
        ),
    ] {
        assert_eq!(value(board), worth, "{what}: {board}");
    }
}

#[test]
fn slicing_the_steps_does_not_change_the_decision() {
    let game = game(KIWIPETE, "");
    let settings = Settings {
        nodes: 30_000,
        ..Level::Normal.settings()
    };
    let mut whole = SearchJob::new(&game, settings, 7).unwrap();
    whole.run(u64::MAX);

    for slice in [1, 7, 1_000] {
        let mut sliced = SearchJob::new(&game, settings, 7).unwrap();
        while !sliced.is_finished() {
            sliced.run(slice);
        }
        assert_eq!(sliced.decision(), whole.decision(), "slices of {slice}");
    }
    assert!(whole.decision().unwrap().nodes <= settings.nodes);
}

#[test]
fn stopping_after_any_step_leaves_a_legal_action() {
    for fen in [KIWIPETE, TACTICS, ROOK_ENDGAME] {
        let game = game(fen, "");
        let settings = Settings {
            nodes: 4_000,
            ..Level::Normal.settings()
        };
        let mut job = SearchJob::new(&game, settings, 3).unwrap();
        let mut in_quiescence = 0;
        loop {
            let decision = job.best_so_far();
            let player = game.position().side_to_move();
            let mut copy = game.clone();
            assert!(
                copy.apply(decision.revision, player, decision.action)
                    .is_ok(),
                "{fen}: {:?} after {} steps",
                decision.action,
                job.progress().nodes
            );
            in_quiescence += u32::from(job.progress().quiescence);
            if job.is_finished() {
                break;
            }
            job.step();
        }
        assert!(in_quiescence > 100, "{fen}: stopped among captures too");
        assert_eq!(job.decision(), Some(&job.best_so_far()));
    }
}

/// A frame for `fen`, set up as the search sets one up, that asks only
/// whether the position is worth what it looks.
fn frame_asking_a_bound(fen: &str) -> Frame {
    let mut frame = Frame::new(Position::from_fen(fen).unwrap());
    frame.in_check = frame.position.is_check();
    frame.depth = PASS_DEPTH;
    frame.beta = evaluate(&frame.position);
    frame.alpha = frame.beta - 1;
    frame
}

#[test]
fn a_player_passes_only_where_a_pass_can_prove_something() {
    let ordinary = frame_asking_a_bound(KIWIPETE);
    assert!(may_pass(&ordinary, false));

    let after = |change: fn(&mut Frame)| {
        let mut frame = ordinary.clone();
        change(&mut frame);
        may_pass(&frame, false)
    };
    assert!(!may_pass(&ordinary, true), "straight after a pass");
    assert!(
        !after(|frame| frame.depth = PASS_DEPTH - 1),
        "with too little search left"
    );
    assert!(
        !after(|frame| frame.alpha -= 1),
        "where a value is wanted and not a bound"
    );
    assert!(
        !after(|frame| {
            frame.alpha += 1;
            frame.beta += 1;
        }),
        "from a position that looks short of the bound"
    );
    assert!(
        !after(|frame| {
            frame.alpha = -MATE_BOUND - 1;
            frame.beta = -MATE_BOUND;
        }),
        "where the bound is a forced mate"
    );

    // White is in check and has a rook besides.
    let checked = frame_asking_a_bound("4k3/8/8/8/8/8/4r3/4K2R w K - 0 1");
    assert!(checked.in_check && checked.position.mover_has_pieces());
    assert!(!may_pass(&checked, false), "in check");
    // With pawns and a king, having to move is often the whole trouble.
    assert!(
        !may_pass(&frame_asking_a_bound(PAWN_ENDGAME), false),
        "with pawns only"
    );
}

#[test]
fn a_pass_proves_a_bound_and_never_a_mate() {
    assert_eq!(proved_by_pass(49, 50), None);
    assert_eq!(proved_by_pass(50, 50), Some(50));
    assert_eq!(proved_by_pass(300, 50), Some(300));
    assert_eq!(proved_by_pass(MATE_BOUND - 1, 50), Some(MATE_BOUND - 1));
    assert_eq!(proved_by_pass(MATE_BOUND, 50), Some(50));
    assert_eq!(proved_by_pass(MATE - 5, 50), Some(50));
    assert_eq!(proved_by_pass(-MATE + 4, 50), None);
}

/// Checks what must hold of a search in progress whatever it skips, and
/// returns how many of its frames have passed and how many are waiting for
/// a reply searched less deeply than its turn.
fn assert_stack_is_sound(job: &SearchJob, context: &str) -> (usize, usize) {
    let (mut passes, mut reduced) = (0, 0);
    for ply in 1..=job.height {
        let frame = &job.frames[ply - 1];
        let context = format!("{context}, {ply} half-moves down: {}", frame.position);
        // Nobody has left their own king to be taken.
        assert!(frame.position.mover_is_safe(), "{context}");
        let reply = (ply < job.height).then(|| &job.frames[ply]);
        match frame.state {
            State::Passing => {
                passes += 1;
                let reply = reply.unwrap_or_else(|| panic!("{context}: no reply to the pass"));
                assert_eq!(reply.position, frame.position.pass(), "{context}");
                assert!(!frame.position.is_check(), "{context}");
                assert!(frame.position.mover_has_pieces(), "{context}");
                assert!(frame.depth >= PASS_DEPTH, "{context}");
                assert_eq!(frame.beta - frame.alpha, 1, "{context}");
                assert!(reply.depth < frame.depth - 1, "{context}");
                let after_pass = ply >= 2 && job.frames[ply - 2].state == State::Passing;
                assert!(!after_pass, "{context}: two passes in a row");
                if reply.state == State::Fresh {
                    // Whether the position still reaches the bound is all
                    // that is asked.
                    let asked = (-frame.beta, 1 - frame.beta);
                    assert_eq!((reply.alpha, reply.beta), asked, "{context}");
                }
            }
            State::Open => {
                // The moves counted as tried are the legal ones.
                let tried = frame.moves.tried().count();
                assert_eq!(tried, usize::from(frame.legal), "{context}");
                if let Some(reply) = reply {
                    let mv = frame.current;
                    assert_eq!(reply.position, frame.position.make(mv), "{context}");
                    let full = frame.depth.saturating_sub(1);
                    if frame.reduced {
                        reduced += 1;
                        let context = format!("{context}: {mv} reduced");
                        assert!(is_quiet(&frame.position, mv), "{context}");
                        assert!(!frame.in_check, "{context}");
                        assert!(!reply.position.is_check(), "{context}");
                        assert!(frame.depth >= REDUCE_DEPTH, "{context}");
                        assert!(frame.legal > REDUCE_AFTER, "{context}");
                        assert!((1..full).contains(&reply.depth), "{context}");
                        // Whether the move matters is all that is asked.
                        assert_eq!(reply.beta, -frame.alpha, "{context}");
                    } else {
                        assert_eq!(reply.depth, full, "{context}");
                    }
                }
            }
            State::Fresh | State::Done(_) => assert!(reply.is_none(), "{context}"),
        }
    }
    (passes, reduced)
}

#[test]
fn a_search_in_progress_passes_and_reduces_only_where_it_may() {
    let (mut passes, mut reduced) = (0, 0);
    for (fen, played) in [
        (START, "e2e4 e7e5 g1f3"),
        (KIWIPETE, ""),
        (TACTICS, ""),
        (PROMOTIONS, ""),
        (ROOK_ENDGAME, ""),
        (QUEEN_ENDGAME, ""),
        (PAWN_ENDGAME, ""),
        (PAWN_RACE, ""),
    ] {
        let game = game(fen, played);
        let settings = Settings {
            nodes: 20_000,
            ..Level::Hard.settings()
        };
        let mut job = SearchJob::new(&game, settings, 0).unwrap();
        let mut passes_here = 0;
        while !job.is_finished() {
            job.step();
            let (passing, waiting) = assert_stack_is_sound(&job, fen);
            passes_here += passing;
            reduced += waiting;
        }
        if fen == PAWN_ENDGAME {
            assert_eq!(passes_here, 0, "{fen}: nobody has a piece to pass with");
        }
        passes += passes_here;
    }
    assert!(passes > 1_000, "only {passes} passes were seen");
    assert!(reduced > 1_000, "only {reduced} reduced searches were seen");
}

#[test]
fn a_search_too_shallow_for_either_agrees_with_the_plain_search() {
    for (name, fen, played, depth, quiescence) in CASES {
        let game = game(fen, played);
        let mut expected = Expected::new(name, &game, quiescence, IN_PLAY);
        // The first position that could pass or reduce lies one half-move
        // down, with one half-move less to search.
        for depth in 1..=depth.min(PASS_DEPTH).min(REDUCE_DEPTH) {
            assert_agrees(&mut expected, settings(depth, quiescence, 0, 0));
        }
    }
}

#[test]
fn with_pawns_only_the_answer_is_the_plain_search_s() {
    // No pawn can promote within the depth, so nobody ever has a piece.
    let game = game(PAWN_ENDGAME, "");
    let mut expected = Expected::new("pawn-endgame", &game, 2, PASSING);
    for depth in 4..=6 {
        assert_agrees(&mut expected, settings(depth, 2, 0, 0));
    }
}

#[test]
fn passing_and_reducing_each_save_work() {
    // Either needs three half-moves of search below it, so it tells only in
    // searches a few half-moves deeper than that.
    for (fen, depth) in [(START, 7), (KIWIPETE, 6), (ROOK_ENDGAME, 9)] {
        let game = game(fen, "");
        let nodes = |rules| {
            let job = searched(&game, settings(depth, 4, 16, 0), rules);
            let decision = job.decision().unwrap();
            assert_eq!(decision.depth, depth, "{fen}");
            decision.nodes
        };
        let without = nodes(TABLE_AS_IN_PLAY);
        for (what, rules) in [("passing", PASSING), ("reducing", REDUCING)] {
            let with = nodes(rules);
            assert!(
                with * 5 < without * 4,
                "{fen}: {with} positions with {what}, {without} without"
            );
        }
        let both = nodes(IN_PLAY);
        assert!(both * 2 < without, "{fen}: {both} positions with both");
    }
}

#[test]
fn a_shallow_search_may_only_reject_a_move() {
    let (alpha, beta) = (10, 50);
    // Whatever a shallow search rates above the best so far is searched
    // again at its depth, in the same window.
    for probing in [false, true] {
        assert_eq!(judge(alpha, alpha, beta, true, probing), Verdict::Take);
        assert_eq!(
            judge(alpha + 1, alpha, beta, true, probing),
            Verdict::Deepen
        );
        assert_eq!(judge(beta + 7, alpha, beta, true, probing), Verdict::Deepen);
    }
    // A search at full depth in a window of one point says only whether the
    // move beats the best so far. If it does, and the other player might
    // still allow it, the move is searched again for its value.
    assert_eq!(judge(alpha, alpha, beta, false, true), Verdict::Take);
    assert_eq!(judge(alpha + 1, alpha, beta, false, true), Verdict::Widen);
    assert_eq!(judge(beta - 1, alpha, beta, false, true), Verdict::Widen);
    assert_eq!(judge(beta, alpha, beta, false, true), Verdict::Take);
    // A search at full depth for the value is the last word.
    for score in [alpha - 1, alpha + 1, beta, MATE - 3] {
        assert_eq!(judge(score, alpha, beta, false, false), Verdict::Take);
    }
}

#[test]
fn a_reduction_spares_the_first_moves_and_leaves_a_full_half_move() {
    for depth in 0..=MAX_DEPTH {
        for tried in 1..=300 {
            let reduction = reduction_for(depth, tried);
            let context = format!("depth {depth}, move {tried}");
            if depth < REDUCE_DEPTH || tried <= REDUCE_AFTER {
                assert_eq!(reduction, 0, "{context}");
                continue;
            }
            assert!(reduction >= 1, "{context}");
            // The reply keeps at least one half-move of full search.
            assert!(depth - 1 - reduction >= 1, "{context}");
            // Later moves and deeper searches are reduced no less.
            assert!(reduction >= reduction_for(depth, tried - 1), "{context}");
            assert!(reduction >= reduction_for(depth - 1, tried), "{context}");
        }
    }
    // The table of logarithms is what it says.
    for (number, &stored) in LOGARITHMS.iter().enumerate().skip(1) {
        let exact = 64.0 * (number as f64).ln();
        assert!((f64::from(stored) - exact).abs() <= 0.5, "{number}");
    }
}

#[test]
fn a_quiet_move_that_stops_a_search_is_marked_up_and_those_tried_before_it_down() {
    let position = Position::from_fen(KIWIPETE).unwrap();
    let mv = |text| Move::parse(text).unwrap();
    // Two captures and two quiet moves were tried before castling stopped
    // the search.
    let tried = ["e2a6", "a2a3", "g2h3", "b2b3", "e1g1"];
    let marked = |depth| {
        let mut history = History::new();
        let stopped_at = mv("e1g1");
        mark(
            &mut history,
            &position,
            tried.into_iter().map(mv),
            stopped_at,
            depth,
        );
        history
    };
    let history = marked(4);
    let score = |text| history.score(Color::White, mv(text));
    assert!(score("e1g1") > 0);
    assert!(score("a2a3") < 0 && score("b2b3") < 0);
    assert_eq!((score("e2a6"), score("g2h3")), (0, 0), "captures");
    assert_eq!(score("d2e3"), 0, "a move that was not tried");
    assert_eq!(history.score(Color::Black, mv("e1g1")), 0);
    // A result found by a deeper search counts for more.
    assert!(marked(8).score(Color::White, mv("e1g1")) > score("e1g1"));
    assert!(marked(8).score(Color::White, mv("a2a3")) < score("a2a3"));
}

#[test]
fn a_search_marks_the_quiet_moves_it_tries() {
    let game = game(KIWIPETE, "");
    let mut job = SearchJob::new(&game, settings(5, 4, 12, 0), 0).unwrap();
    job.rules = TABLE_AS_IN_PLAY;
    job.run(u64::MAX);

    let square = crate::Square::from_index;
    let (mut raised, mut lowered) = (0, 0);
    for side in [Color::White, Color::Black] {
        for (from, to) in (0..64).flat_map(|from| (0..64).map(move |to| (from, to))) {
            let mv = Move {
                from: square(from),
                to: square(to),
                promotion: None,
            };
            let score = job.history.score(side, mv);
            assert!(score.abs() <= History::LIMIT, "{side:?} {mv}: {score}");
            raised += u32::from(score > 0);
            lowered += u32::from(score < 0);
        }
    }
    // How many depends on the evaluation. That some are marked each way is
    // what the search owes.
    assert!(raised > 0, "no move was marked up");
    assert!(lowered > 0, "no move was marked down");
}

#[test]
fn a_mate_announced_in_play_is_real_and_no_quicker_than_the_quickest() {
    for fen in [
        BACK_RANK,
        ROOK_MATE,
        DOUBLED_ROOKS,
        "8/8/8/4k3/8/8/7q/K7 b - - 0 1",
        "8/8/8/8/3k4/8/7r/K7 b - - 0 1",
    ] {
        let start = game(fen, "");
        let distance = |rules| {
            let job = searched(&start, Level::Hard.settings(), rules);
            MATE - job.decision().unwrap().score.unwrap()
        };
        // The search that skips nothing finds the quickest mate.
        let quickest = distance(WITHOUT_TABLE);
        let announced = distance(IN_PLAY);
        assert!(quickest <= 9, "{fen}: no mate was found");
        assert!(
            (quickest..=1_000).contains(&announced),
            "{fen}: mate in {announced} announced, and the quickest is in {quickest}"
        );

        // Played out against the same search, the game ends in checkmate
        // within the half-moves announced.
        let mut game = start.clone();
        for _ in 0..announced {
            let job = searched(&game, Level::Hard.settings(), IN_PLAY);
            let decision = job.decision().unwrap();
            let player = game.position().side_to_move();
            game.apply(decision.revision, player, decision.action)
                .unwrap();
            if game.outcome().is_some() {
                break;
            }
        }
        let winner = start.position().side_to_move();
        assert_eq!(
            game.outcome(),
            Some(crate::Outcome::Checkmate { winner }),
            "{fen}"
        );
    }
}
