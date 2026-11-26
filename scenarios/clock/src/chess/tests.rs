use std::time::Duration;

use engine_chess::{DrawClaim, DrawReason, Move, Outcome};
use engine_common::{ClockEventProfile, Scenario};

use super::*;
use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario, ClockState};

/// One of the Clock's ticks.
const TICK: Duration = Duration::from_nanos(16_666_667);

/// Ticks `play` `ticks` times, and returns each event with the tick of play
/// it came on.
fn play_for(play: &mut Play, exhibition: &mut Exhibition, ticks: u64) -> Vec<(u64, Event)> {
    (0..ticks)
        .filter_map(|_| play.tick(exhibition).map(|event| (play.ticks(), event)))
        .collect()
}

/// The game of the match in hand.
fn game(exhibition: &Exhibition) -> &Game {
    exhibition.current().expect("a match").game()
}

/// An exhibition whose first match starts from `fen` and has had `moves`.
fn starting_from(fen: &str, moves: &[&str]) -> Exhibition {
    let mut exhibition = Exhibition::new(7);
    exhibition.admit();
    let current = exhibition.current.as_mut().expect("a match");
    current.game = Game::new(current.seed, Position::from_fen(fen).expect("a position"));
    for text in moves {
        let game = &mut current.game;
        let color = game.position().side_to_move();
        let mv = Move::parse(text).expect("a move");
        game.apply(game.revision(), color, Action::Move(mv))
            .expect("a legal move");
    }
    exhibition
}

fn resign(exhibition: &mut Exhibition) {
    let game = &mut exhibition.current.as_mut().expect("a match").game;
    let color = game.position().side_to_move();
    game.apply(game.revision(), color, Action::Resign)
        .expect("the player to move may resign");
}

impl Play {
    /// Has the bird to move take `action` at the end of the turn, whatever
    /// its search finds.
    fn intend(&mut self, game: &Game, action: Action) {
        let choice = match action {
            Action::Move(mv) => mv,
            _ => game.legal_moves()[0],
        };
        self.turn = Turn::Decided(Decision {
            revision: game.revision(),
            action,
            choice,
            score: None,
            depth: 0,
            nodes: 0,
            candidates: 1,
            seed: 0,
        });
    }
}

#[test]
fn the_bird_to_move_acts_every_five_seconds_and_knows_its_action_a_second_before() {
    let mut exhibition = Exhibition::new(1);
    let mut play = Play::begin(exhibition.admit(), Pace::CLOCK);
    let mut events = Vec::new();
    let mut reaching = Vec::new();
    for _ in 0..6 * 300 {
        if let Some(event) = play.tick(&mut exhibition) {
            events.push((play.ticks(), event));
        }
        reaching.push(play.reaching().is_some());
    }

    let ticks: Vec<_> = events.iter().map(|(tick, _)| *tick).collect();
    let expected: Vec<_> = (1..=6)
        .flat_map(|turn| [turn * 300 - 60, turn * 300])
        .collect();
    assert_eq!(ticks, expected);
    let expected: Vec<_> = (1..=6 * 300_u64).map(|tick| tick % 300 >= 240).collect();
    assert_eq!(reaching, expected);

    let mut bird = exhibition.current().expect("a match").bird(Color::White);
    let mut moves = Vec::new();
    for (turn, pair) in events.chunks(2).enumerate() {
        let [
            (
                _,
                Event::Reach {
                    bird: reacher,
                    action: reached,
                },
            ),
            (
                _,
                Event::Act {
                    bird: actor,
                    action,
                    transition,
                },
            ),
        ] = pair
        else {
            panic!("a reach and then an act: {pair:?}");
        };
        assert_eq!((*reacher, *actor), (bird, bird), "turn {turn}");
        assert_eq!(reached, action, "turn {turn}");
        assert_eq!(transition.revision.actions as usize, turn + 1);
        let Action::Move(mv) = action else {
            panic!("a move from the start: {action:?}");
        };
        moves.push(*mv);
        bird = bird.other();
    }
    assert_eq!(game(&exhibition).moves(), moves);
}

#[test]
fn the_first_match_draws_white_from_the_seed_and_each_new_match_changes_hands() {
    let firsts: Vec<_> = (0..16)
        .map(|seed| Exhibition::new(seed).admit().bird(Color::White))
        .collect();
    assert!(
        firsts.contains(&Bird::Crow) && firsts.contains(&Bird::Duck),
        "{firsts:?}"
    );

    let mut exhibition = Exhibition::new(3);
    let first = exhibition.admit().bird(Color::White);
    let mut sessions = Vec::new();
    for number in 1..=4 {
        let current = exhibition.admit();
        assert_eq!(current.number(), number);
        let white = if number % 2 == 1 {
            first
        } else {
            first.other()
        };
        assert_eq!(current.bird(Color::White), white);
        assert_eq!(current.bird(Color::Black), white.other());
        assert_eq!(current.game().position(), &Position::initial());
        sessions.push(current.game().revision().session);
        // Admitted again before it has ended, a visit plays the same match.
        assert_eq!(exhibition.admit().number(), number);

        resign(&mut exhibition);
        // An ended match stays in hand until the next admission.
        let ended = exhibition.current().expect("the ended match");
        assert_eq!(
            (ended.number(), ended.game().outcome().is_some()),
            (number, true)
        );
    }
    sessions.sort_unstable();
    sessions.dedup();
    assert_eq!(sessions.len(), 4, "each match is its own session");
}

#[test]
fn a_match_played_over_many_visits_is_the_match_played_in_one() {
    let mut whole = Exhibition::new(11);
    let mut play = Play::begin(whole.admit(), Pace::CLOCK);
    for _ in 0..40 * Pace::CLOCK.turn {
        play.tick(&mut whole);
    }
    let reference = game(&whole).clone();
    let done = |visits: &Exhibition| game(visits).revision() == reference.revision();

    // Visits that end as a turn begins, just before and just after the bird
    // reaches or acts, and after whole turns. Each round of them plays 28
    // actions, so a hundred visits are more than enough.
    let lengths = [1, 239, 240, 241, 299, 300, 301, 599, 600, 1_000, 6_000];
    let mut visits = Exhibition::new(11);
    for &ticks in lengths.iter().cycle().take(100) {
        let mut play = Play::begin(visits.admit(), Pace::CLOCK);
        for _ in 0..ticks {
            if done(&visits) {
                break;
            }
            play.tick(&mut visits);
        }
        if done(&visits) {
            break;
        }
    }
    assert_eq!(visits.current().expect("a match").number(), 1);
    assert_eq!(game(&visits), &reference);
}

#[test]
fn a_game_played_to_its_end_stops_play_and_the_next_visit_starts_a_new_match() {
    // White has a queen against a lone king.
    let mut exhibition = starting_from("8/8/8/4k3/8/8/8/KQ6 w - - 0 1", &[]);
    let white = exhibition.current().expect("a match").bird(Color::White);
    let mut play = Play::begin(exhibition.admit(), Pace::CLOCK);
    let mut acts = 0;
    // The rules end the game within 150 actions with no capture or pawn move.
    for _ in 0..200 * Pace::CLOCK.turn {
        if play.is_over() {
            break;
        }
        if let Some(Event::Act { transition, .. }) = play.tick(&mut exhibition) {
            acts += 1;
            assert_eq!(transition.outcome, game(&exhibition).outcome());
        }
    }
    assert!(play.is_over(), "the rules end every game");
    let outcome = game(&exhibition).outcome().expect("an ended game");
    eprintln!("queen against king: {outcome:?} after {acts} actions");
    assert_eq!(play.tick(&mut exhibition), None);

    let next = exhibition.admit();
    assert_eq!(next.number(), 2);
    assert_eq!(next.bird(Color::White), white.other());
    assert_eq!(next.game().position(), &Position::initial());
}

#[test]
fn special_moves_claims_and_mates_stand_once_taken_and_are_never_taken_twice() {
    let knights_back_twice = ["g1f3", "g8f6", "f3g1", "f6g8"].repeat(2);
    let knights: Vec<&str> = knights_back_twice.to_vec();
    let start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let mv = |text| Action::Move(Move::parse(text).expect("a move"));
    let cases = [
        (
            "capture",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            &[][..],
            mv("e4d5"),
            None,
        ),
        (
            "castling",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            &[],
            mv("e1g1"),
            None,
        ),
        (
            "en passant",
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
            &[],
            mv("e5f6"),
            None,
        ),
        (
            // To a knight, which the search would not choose. The pawn on h2
            // keeps a knight and king against a king from ending the game.
            "promotion",
            "8/P6k/8/8/8/8/7P/K7 w - - 0 1",
            &[],
            mv("a7a8n"),
            None,
        ),
        (
            "checkmate",
            "6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1",
            &[],
            mv("a1a8"),
            Some(Outcome::Checkmate {
                winner: Color::White,
            }),
        ),
        (
            "draw claim",
            start,
            &knights[..],
            Action::ClaimDraw {
                claim: DrawClaim::Threefold,
                intended: None,
            },
            Some(Outcome::Draw(DrawReason::ThreefoldClaim)),
        ),
    ];
    for (name, fen, moves, action, outcome) in cases {
        let mut exhibition = starting_from(fen, moves);
        let begun = game(&exhibition).clone();

        // A visit that ends just before the bird acts leaves the game as it
        // was.
        let mut play = Play::begin(exhibition.admit(), Pace::CLOCK);
        play.intend(&begun, action);
        play_for(&mut play, &mut exhibition, 299);
        assert_eq!(play.reaching(), Some(action), "{name}");
        drop(play);
        assert_eq!(game(&exhibition), &begun, "{name}");

        // One that ends just after leaves the action taken, once.
        let mut play = Play::begin(exhibition.admit(), Pace::CLOCK);
        play.intend(&begun, action);
        let events = play_for(&mut play, &mut exhibition, 300);
        assert!(
            matches!(events.last(), Some((300, Event::Act { action: taken, .. })) if *taken == action),
            "{name}: {events:?}"
        );
        drop(play);
        let after = game(&exhibition).clone();
        assert_eq!(
            after.revision().actions,
            begun.revision().actions + 1,
            "{name}"
        );
        assert_eq!(after.outcome(), outcome, "{name}");
        if let Action::Move(mv) = action {
            assert_eq!(after.moves().last(), Some(&mv), "{name}");
            let played = begun.position().play(mv).expect("a legal move");
            assert_eq!(after.position(), &played, "{name}");
        }

        if outcome.is_none() {
            // The next visit goes on from there.
            let mut play = Play::begin(exhibition.admit(), Pace::CLOCK);
            play_for(&mut play, &mut exhibition, 300);
            let next = game(&exhibition);
            assert_eq!(
                next.revision().actions,
                after.revision().actions + 1,
                "{name}"
            );
            assert_eq!(
                &next.moves()[..after.moves().len()],
                after.moves(),
                "{name}"
            );
        } else {
            // An ended game has nothing more to play.
            let mut play = Play::begin(exhibition.current().expect("a match"), Pace::CLOCK);
            assert!(play.is_over(), "{name}");
            assert_eq!(play_for(&mut play, &mut exhibition, 300), [], "{name}");
            assert_eq!(game(&exhibition), &after, "{name}");
        }
    }
}

#[test]
fn a_search_unfinished_when_the_bird_must_reach_gives_the_best_action_it_had() {
    let pace = Pace {
        quota: 10,
        ..Pace::CLOCK
    };
    let mut exhibition = Exhibition::new(5);
    let current = exhibition.admit().clone();
    let mut play = Play::begin(&current, pace);
    let events = play_for(&mut play, &mut exhibition, 300);

    // By the reach, 240 quotas of 10 positions: a quarter of what Normal
    // looks at.
    let mut job =
        SearchJob::new(current.game(), pace.settings, current.search_seed()).expect("a search");
    job.run(240 * 10);
    assert!(!job.is_finished());
    let best = job.best_so_far().action;
    let actions: Vec<_> = events
        .iter()
        .map(|(tick, event)| match event {
            Event::Reach { action, .. } | Event::Act { action, .. } => (*tick, *action),
        })
        .collect();
    assert_eq!(actions, [(240, best), (300, best)]);
}

#[test]
fn normal_play_has_its_action_within_three_ticks_of_its_turn() {
    let mut exhibition = Exhibition::new(9);
    let mut play = Play::begin(exhibition.admit(), Pace::CLOCK);
    for turn in 0..12 {
        let decided = (1..=3).find(|_| {
            play.tick(&mut exhibition);
            matches!(play.turn, Turn::Decided(_))
        });
        assert!(decided.is_some(), "turn {turn}");
        play_for(&mut play, &mut exhibition, 300 - decided.unwrap_or(0));
        assert_eq!(game(&exhibition).moves().len(), turn + 1);
    }
}

#[test]
fn play_for_a_state_the_game_has_left_or_for_another_match_changes_nothing() {
    // Two plays at one match: the first to act moves the game on, and the
    // other's action, chosen for the state before, is refused.
    let mut exhibition = Exhibition::new(13);
    let mut first = Play::begin(exhibition.admit(), Pace::CLOCK);
    let mut second = Play::begin(exhibition.admit(), Pace::CLOCK);
    let mut events = [Vec::new(), Vec::new()];
    for _ in 0..300 {
        events[0].extend(first.tick(&mut exhibition));
        events[1].extend(second.tick(&mut exhibition));
    }
    assert!(matches!(
        events[0][..],
        [Event::Reach { .. }, Event::Act { .. }]
    ));
    assert!(matches!(events[1][..], [Event::Reach { .. }]));
    assert_eq!(game(&exhibition).revision().actions, 1);
    assert!(second.is_over() && !first.is_over());

    // A play for a match that the run has left behind.
    resign(&mut exhibition);
    exhibition.admit();
    let next = exhibition.current().expect("a new match").clone();
    assert_eq!(play_for(&mut first, &mut exhibition, 300), []);
    assert!(first.is_over());
    assert_eq!(exhibition.current(), Some(&next));

    // A play for another run's match of the same number.
    let mut other = Exhibition::new(14);
    other.admit();
    let mut stray = Play::begin(Exhibition::new(13).admit(), Pace::CLOCK);
    assert_eq!(play_for(&mut stray, &mut other, 300), []);
    assert!(stray.is_over());
    assert_eq!(game(&other).revision().actions, 0);
}

fn clock(seed: u64) -> ClockState {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Off,
        ..ClockConfig::default()
    };
    ClockScenario::init(config, seed)
}

/// The moves of the Clock's match in hand.
fn clock_moves(state: &ClockState) -> &[Move] {
    state.chess().current().expect("a match").game().moves()
}

#[test]
fn the_board_takes_only_ticks_that_move_and_pays_no_heed_to_the_time_of_day() {
    let mut quiet = clock(21);
    let mut busy = clock(21);
    assert!(quiet.admit_chess_play() && busy.admit_chess_play());
    assert!(!busy.admit_chess_play(), "one play at a time");

    let at = |hour, minute, second| ClockReading::new(hour, minute, second).expect("a time");
    let date = |day| ClockDate::new(2026, 11, day).expect("a date");
    // Steps that take no time play nothing, whatever the time does.
    for second in 0..120 {
        let reading = ClockAction::set_reading(at(9, 59, second % 60));
        ClockScenario::step(&mut busy, &[reading], Duration::ZERO);
    }
    assert_eq!(busy.chess_play().expect("play").ticks(), 0);

    for tick in 1..=600 {
        let actions = match tick {
            100 => vec![ClockAction::set_reading(at(10, 0, 0))],
            200 => vec![ClockAction::set_reading(at(23, 59, 59).with_date(date(25)))],
            201 => vec![ClockAction::set_reading(at(0, 0, 0).with_date(date(26)))],
            400 => vec![ClockAction::set_reading(at(3, 15, 0))],
            _ => Vec::new(),
        };
        ClockScenario::step(&mut busy, &actions, TICK);
        ClockScenario::step(&mut quiet, &[], TICK);
        assert_eq!(
            busy.chess().current(),
            quiet.chess().current(),
            "tick {tick}"
        );
        assert_eq!(clock_moves(&busy).len(), tick / 300, "tick {tick}");
    }
}

#[test]
fn a_resize_ends_play_and_the_match_goes_on_at_the_next_visit_but_not_into_a_new_run() {
    let mut reference = clock(22);
    reference.admit_chess_play();
    for _ in 0..900 {
        ClockScenario::step(&mut reference, &[], TICK);
    }
    let moves = clock_moves(&reference).to_vec();
    assert_eq!(moves.len(), 3);

    let mut state = clock(22);
    state.admit_chess_play();
    for _ in 0..600 {
        ClockScenario::step(&mut state, &[], TICK);
    }
    let aspect_ratio = state.aspect_ratio();
    state.set_aspect_ratio(aspect_ratio * 0.8);
    assert_ne!(state.aspect_ratio(), aspect_ratio);
    assert!(state.chess_play().is_none(), "a resize ends the visit");

    // Away from the board, nothing is played.
    for _ in 0..1_000 {
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert_eq!(clock_moves(&state), &moves[..2]);

    // The next visit plays on from there.
    assert!(state.admit_chess_play());
    for _ in 0..300 {
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert_eq!(state.chess().current().expect("a match").number(), 1);
    assert_eq!(clock_moves(&state), moves);

    assert!(state.end_chess_play() && !state.end_chess_play());
    assert!(
        clock(22).chess().current().is_none(),
        "a new run has no match"
    );
}
