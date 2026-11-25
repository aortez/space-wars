//! A UCI front end for the search, so that standard chess tools can run it:
//! match runners, test-position tools, or any program with a board to play
//! against. The game itself does not use this.
//!
//! ```text
//! cargo run --release -p engine-chess --example uci
//! ```
//!
//! `go` takes `nodes`, `depth`, `movetime`, the clock (`wtime`, `btime`,
//! `winc`, `binc`, `movestogo`) and `infinite`. With none of them it searches
//! 1,000,000 positions. Only `nodes` and `depth` give the same move on every
//! machine and every run; the others stop the search by the clock.
//!
//! The interface has no way to claim a draw. Where the search would claim
//! one, this plays the move it would otherwise play, and leaves the draw to
//! whoever runs the game.
//!
//! When its input closes, the program answers any search in hand and quits,
//! as it does for `quit`: nobody is left to tell it to stop.

use std::io::{self, BufRead, Write};
use std::str::SplitWhitespace;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use engine_chess::search::{
    MATE, MATE_BOUND, MAX_DEPTH, MAX_QUIESCENCE, MAX_TABLE_BITS, SearchJob, Settings,
};
use engine_chess::{Action, ActionError, Color, Game, Move, Position};

/// The positions searched by a `go` that sets no limit.
const DEFAULT_NODES: u64 = 1_000_000;
/// How many positions are searched between looks at the clock and at the
/// commands waiting.
const SLICE: u64 = 512;
/// Time kept back for the answer to reach whoever is waiting for it.
const OVERHEAD: Duration = Duration::from_millis(10);

struct Options {
    /// The size of the table of positions already searched, in MiB.
    hash: u32,
    quiescence: u8,
    margin: u16,
    seed: u64,
}

/// What a `go` command asked for.
#[derive(Default)]
struct Limits {
    nodes: Option<u64>,
    depth: Option<u8>,
    move_time: Option<u64>,
    /// What White and Black have left on the clock, in milliseconds.
    clock: [Option<u64>; 2],
    increment: [u64; 2],
    moves_to_go: Option<u64>,
    infinite: bool,
}

impl Limits {
    fn parse(tokens: SplitWhitespace) -> Self {
        let mut limits = Self::default();
        let mut tokens = tokens.peekable();
        while let Some(name) = tokens.next() {
            if name == "infinite" {
                limits.infinite = true;
                continue;
            }
            let Some(value) = tokens.peek().and_then(|text| number(text)) else {
                continue;
            };
            tokens.next();
            match name {
                "nodes" => limits.nodes = Some(value),
                "depth" => limits.depth = Some(value.min(u64::from(MAX_DEPTH)) as u8),
                "movetime" => limits.move_time = Some(value),
                "wtime" => limits.clock[0] = Some(value),
                "btime" => limits.clock[1] = Some(value),
                "winc" => limits.increment[0] = value,
                "binc" => limits.increment[1] = value,
                "movestogo" => limits.moves_to_go = Some(value),
                _ => {}
            }
        }
        limits
    }

    /// How long this move may take, if the clock decides.
    fn time(&self, player: Color) -> Option<Duration> {
        let allowed = match self.move_time {
            Some(move_time) => move_time,
            None => {
                let side = player as usize;
                let left = self.clock[side]?;
                // An even share of the time left, most of the increment, and
                // never more than half of what is left.
                let moves = self.moves_to_go.unwrap_or(30).max(1);
                let increment = self.increment[side].saturating_mul(3) / 4;
                (left / moves).saturating_add(increment).min(left / 2)
            }
        };
        let allowed = Duration::from_millis(allowed);
        Some(
            allowed
                .saturating_sub(OVERHEAD)
                .max(Duration::from_millis(1)),
        )
    }
}

/// A number as a program writes it. One below zero counts as zero, which is
/// how some show a clock that has run out, and one too large to hold counts
/// as the largest there is.
fn number(text: &str) -> Option<u64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(if negative {
        0
    } else {
        digits.parse().unwrap_or(u64::MAX)
    })
}

/// What the commands that arrive during a search ask of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Nothing,
    Stop,
    Quit,
}

/// Acts on the commands that have arrived while a search runs, as far as
/// the first that ends it. What follows that one is left for when the
/// search has answered.
fn asked(waiting: &mut dyn FnMut() -> Option<String>, out: &mut dyn Write) -> io::Result<Asked> {
    while let Some(line) = waiting() {
        match line.split_whitespace().next() {
            Some("stop") => return Ok(Asked::Stop),
            Some("quit") => return Ok(Asked::Quit),
            Some("isready") => {
                writeln!(out, "readyok")?;
                out.flush()?;
            }
            _ => {}
        }
    }
    Ok(Asked::Nothing)
}

/// The next command that has arrived, if one has. Once the input has
/// closed, nobody is left to stop a search, so it is told to quit.
fn waiting_line(lines: &mpsc::Receiver<String>) -> Option<String> {
    match lines.try_recv() {
        Ok(line) => Some(line),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => Some("quit".to_string()),
    }
}

/// The table holds entries of sixteen bytes, so a mebibyte is two to the
/// sixteenth of them.
fn table_bits(megabytes: u32) -> u8 {
    match megabytes.checked_ilog2() {
        Some(doublings) => (16 + doublings as u8).min(MAX_TABLE_BITS),
        None => 0,
    }
}

/// A score as the interface writes it: centipawns, or moves to a mate.
fn score_text(score: i32) -> String {
    if score.abs() < MATE_BOUND {
        return format!("cp {score}");
    }
    let half_moves = MATE - score.abs();
    if score > 0 {
        format!("mate {}", (half_moves + 1) / 2)
    } else {
        format!("mate -{}", half_moves / 2)
    }
}

struct Engine {
    options: Options,
    game: Game,
    games: u64,
}

impl Engine {
    fn new() -> Self {
        Self {
            options: Options {
                hash: 16,
                quiescence: 8,
                margin: 0,
                seed: 0,
            },
            game: Game::new(0, Position::initial()),
            games: 0,
        }
    }

    /// Acts on one command. `waiting` returns a command that arrived while a
    /// search was running, if one has. Returns `false` when asked to quit.
    fn handle(
        &mut self,
        line: &str,
        waiting: &mut dyn FnMut() -> Option<String>,
        out: &mut dyn Write,
    ) -> io::Result<bool> {
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("uci") => {
                writeln!(
                    out,
                    "id name Space-Wars Chess {}",
                    env!("CARGO_PKG_VERSION")
                )?;
                writeln!(out, "id author the Space-Wars authors")?;
                writeln!(out, "option name Hash type spin default 16 min 0 max 64")?;
                writeln!(
                    out,
                    "option name Quiescence type spin default 8 min 0 max {MAX_QUIESCENCE}"
                )?;
                writeln!(out, "option name Margin type spin default 0 min 0 max 1000")?;
                writeln!(
                    out,
                    "option name Seed type spin default 0 min 0 max 2147483647"
                )?;
                writeln!(out, "uciok")?;
            }
            Some("isready") => writeln!(out, "readyok")?,
            Some("ucinewgame") => {
                self.games += 1;
                self.game = Game::new(self.games, Position::initial());
            }
            Some("setoption") => self.set_option(tokens),
            Some("position") => {
                if let Err(problem) = self.set_position(tokens) {
                    writeln!(out, "info string position not understood: {problem}")?;
                }
            }
            Some("go") => {
                let limits = Limits::parse(tokens);
                return self.go(&limits, waiting, out);
            }
            Some("quit") => return Ok(false),
            _ => {}
        }
        out.flush()?;
        Ok(true)
    }

    fn set_option(&mut self, tokens: SplitWhitespace) {
        let words: Vec<&str> = tokens.collect();
        let ["name", name @ .., "value", value] = words.as_slice() else {
            return;
        };
        let Ok(value) = value.parse::<u64>() else {
            return;
        };
        match name.join(" ").to_ascii_lowercase().as_str() {
            "hash" => self.options.hash = value.min(64) as u32,
            "quiescence" => self.options.quiescence = value.min(u64::from(MAX_QUIESCENCE)) as u8,
            "margin" => self.options.margin = value.min(1000) as u16,
            "seed" => self.options.seed = value,
            _ => {}
        }
    }

    fn set_position(&mut self, mut tokens: SplitWhitespace) -> Result<(), String> {
        let start = match tokens.next() {
            Some("startpos") => {
                match tokens.next() {
                    None | Some("moves") => {}
                    Some(other) => return Err(format!("`{other}` where `moves` should be")),
                }
                Position::initial()
            }
            Some("fen") => {
                let mut fields: Vec<&str> = tokens
                    .by_ref()
                    .take_while(|&word| word != "moves")
                    .collect();
                if fields.len() == 4 {
                    // Some programs leave the two counters out.
                    fields.extend(["0", "1"]);
                }
                Position::from_fen(&fields.join(" ")).map_err(|error| error.to_string())?
            }
            other => return Err(format!("{other:?} is neither `startpos` nor `fen`")),
        };
        let mut game = Game::new(self.games, start);
        for text in tokens {
            let mv = Move::parse(text).ok_or_else(|| format!("`{text}` is not a move"))?;
            match play(&mut game, mv) {
                Ok(()) => {}
                Err(ActionError::GameOver) => {
                    // The rules have ended the game and the program plays
                    // on: after a fifth repetition, seventy-five moves
                    // without progress, or with too little left to mate.
                    // Go on from the position the move leads to, with
                    // nothing behind it.
                    let next = game.position().play(mv);
                    let next = next.ok_or_else(|| format!("{text}: the move is not legal"))?;
                    game = Game::new(self.games, next);
                }
                Err(error) => return Err(format!("{text}: {error}")),
            }
        }
        self.game = game;
        Ok(())
    }

    /// The limits of the search that a `go` asks for. `timed` says whether
    /// the clock will stop it.
    fn settings(&self, limits: &Limits, timed: bool) -> Settings {
        // Without a limit of some other kind, the search is given a number
        // of positions.
        let unbounded = limits.infinite || timed || limits.depth.is_some();
        Settings {
            depth: limits.depth.unwrap_or(MAX_DEPTH),
            nodes: limits
                .nodes
                .unwrap_or(if unbounded { u64::MAX } else { DEFAULT_NODES }),
            quiescence: self.options.quiescence,
            table_bits: table_bits(self.options.hash),
            margin: self.options.margin,
        }
    }

    fn go(
        &mut self,
        limits: &Limits,
        waiting: &mut dyn FnMut() -> Option<String>,
        out: &mut dyn Write,
    ) -> io::Result<bool> {
        let player = self.game.position().side_to_move();
        let time = limits.time(player);
        let settings = self.settings(limits, time.is_some());
        let Some(mut job) = SearchJob::new(&self.game, settings, self.options.seed) else {
            // The rules have ended the game, and the program asks anyway.
            let mut quit = false;
            if limits.infinite {
                // An endless search may not answer until it is told to stop.
                loop {
                    match asked(waiting, out)? {
                        Asked::Nothing => std::thread::sleep(Duration::from_millis(1)),
                        Asked::Stop => break,
                        Asked::Quit => {
                            quit = true;
                            break;
                        }
                    }
                }
            }
            match self.game.position().legal_moves().first() {
                Some(mv) => writeln!(out, "bestmove {mv}")?,
                None => writeln!(out, "bestmove 0000")?,
            }
            out.flush()?;
            return Ok(!quit);
        };

        let started = Instant::now();
        let mut reported = 0;
        let mut stopped = false;
        let mut quit = false;
        while !stopped {
            if job.is_finished() {
                // An endless search may not answer until it is told to stop.
                if !limits.infinite {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            job.run(SLICE);

            let depth = job.progress().depth;
            if depth > reported {
                reported = depth;
                let decision = job.best_so_far();
                let elapsed = started.elapsed().as_millis().max(1);
                writeln!(
                    out,
                    "info depth {depth} score {} nodes {} nps {} time {elapsed} pv {}",
                    score_text(decision.score.unwrap_or(0)),
                    decision.nodes,
                    u128::from(decision.nodes) * 1000 / elapsed,
                    decision.choice
                )?;
                out.flush()?;
            }
            // A depth cut short by the clock still counts for the moves it
            // got to, so the search runs for all of its time.
            stopped |= time.is_some_and(|time| started.elapsed() >= time);

            match asked(waiting, out)? {
                Asked::Nothing => {}
                Asked::Stop => stopped = true,
                Asked::Quit => {
                    stopped = true;
                    quit = true;
                }
            }
        }

        writeln!(out, "bestmove {}", job.best_so_far().choice)?;
        out.flush()?;
        Ok(!quit)
    }
}

fn play(game: &mut Game, mv: Move) -> Result<(), ActionError> {
    let player = game.position().side_to_move();
    game.apply(game.revision(), player, Action::Move(mv))
        .map(|_| ())
}

fn main() -> io::Result<()> {
    // Commands are read on a thread of their own, so that `stop` is seen
    // while a search runs.
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });

    let mut engine = Engine::new();
    let mut out = io::stdout().lock();
    // The loop ends when asked to quit or when the commands run out.
    while let Ok(line) = lines.recv() {
        let mut waiting = || waiting_line(&lines);
        if !engine.handle(&line, &mut waiting, &mut out)? {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACK_RANK: &str = "6k1/5ppp/8/8/8/8/8/R3K3 w Q - 0 1";

    /// Runs the commands with nothing arriving meanwhile, and returns what
    /// was written.
    fn run(engine: &mut Engine, commands: &[&str]) -> String {
        let mut out = Vec::new();
        for command in commands {
            engine.handle(command, &mut || None, &mut out).unwrap();
        }
        String::from_utf8(out).unwrap()
    }

    fn best_move(output: &str) -> Move {
        let line = output.lines().last().unwrap();
        let text = line.strip_prefix("bestmove ").expect(line);
        Move::parse(text).expect(line)
    }

    #[test]
    fn it_introduces_itself_and_says_when_it_is_ready() {
        let output = run(&mut Engine::new(), &["uci", "isready"]);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines[0].starts_with("id name Space-Wars Chess "));
        assert!(lines.contains(&"option name Hash type spin default 16 min 0 max 64"));
        assert_eq!(lines[lines.len() - 2..], ["uciok", "readyok"]);
    }

    #[test]
    fn a_number_of_positions_gives_the_same_legal_move_every_time() {
        let commands = ["position startpos moves e2e4 e7e5", "go nodes 3000"];
        // What a run wrote, without the speed and the time, which differ from
        // run to run.
        let found = |output: &str| -> Vec<String> {
            let untimed = |line: &str| {
                let words: Vec<&str> = line.split(' ').collect();
                match words.iter().position(|&word| word == "nps") {
                    Some(nps) => [&words[..nps], &words[nps + 4..]].concat().join(" "),
                    None => line.to_string(),
                }
            };
            output.lines().map(untimed).collect()
        };

        let mut engine = Engine::new();
        let first = run(&mut engine, &commands);
        let again = run(&mut Engine::new(), &commands);
        assert_eq!(found(&first), found(&again));
        assert!(first.contains("info depth 2 score cp "), "{first}");
        assert_eq!(engine.game.moves().len(), 2);
        assert!(engine.game.legal_moves().contains(&best_move(&first)));
    }

    #[test]
    fn a_position_given_as_fen_is_searched_and_a_mate_counted_in_moves() {
        let position = format!("position fen {BACK_RANK}");
        let output = run(&mut Engine::new(), &[&position, "go depth 3"]);
        assert!(output.contains(" score mate 1 "), "{output}");
        assert_eq!(best_move(&output), Move::parse("a1a8").unwrap());
        assert_eq!(score_text(MATE - 3), "mate 2");
        assert_eq!(score_text(4 - MATE), "mate -2");
        assert_eq!(score_text(-17), "cp -17");
    }

    #[test]
    fn a_position_that_makes_no_sense_is_reported_and_the_last_one_kept() {
        let mut engine = Engine::new();
        run(&mut engine, &["position startpos moves e2e4"]);
        for bad in [
            "position startpos moves e2e5",
            "position startpos moves e7e5",
            "position fen 8/8/8/8/8/8/8/8 w - - 0 1",
            "position somewhere",
        ] {
            let output = run(&mut engine, &[bad]);
            assert!(
                output.starts_with("info string position not understood"),
                "{bad}"
            );
            assert_eq!(engine.game.moves().len(), 1, "{bad}");
        }
    }

    #[test]
    fn a_game_the_rules_have_ended_can_still_be_followed() {
        // Four times round brings the position up a fifth time, which ends
        // the game. A program that does not know that plays on.
        let shuffle = "g1f3 g8f6 f3g1 f6g8 ".repeat(4);
        let mut engine = Engine::new();
        let position = format!("position startpos moves {shuffle}e2e4");
        let output = run(&mut engine, &[&position, "go nodes 500"]);
        assert!(engine.game.legal_moves().contains(&best_move(&output)));
        assert_eq!(engine.game.position().side_to_move(), Color::Black);

        // Asked for a move in a finished game, it still names a legal one,
        // or none when there is none.
        let ended = format!("position startpos moves {shuffle}");
        let output = run(&mut engine, &[&ended, "go nodes 500"]);
        assert!(engine.game.outcome().is_some());
        assert!(
            Position::initial()
                .legal_moves()
                .contains(&best_move(&output))
        );
        let mated = "position startpos moves f2f3 e7e5 g2g4 d8h4";
        assert_eq!(run(&mut engine, &[mated, "go"]), "bestmove 0000\n");
    }

    #[test]
    fn an_endless_search_answers_when_told_to_stop() {
        let mut engine = Engine::new();
        let mut asked = 0;
        let mut waiting = || {
            asked += 1;
            match asked {
                3 => Some("isready".to_string()),
                20 => Some("stop".to_string()),
                _ => None,
            }
        };
        let mut out = Vec::new();
        let position = format!("position fen {BACK_RANK}");
        engine.handle(&position, &mut || None, &mut out).unwrap();
        // The mate is found at once, and the answer still waits for `stop`.
        let go_on = engine
            .handle("go infinite", &mut waiting, &mut out)
            .unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(go_on);
        assert!(asked >= 20);
        assert!(output.contains("readyok\n"));
        assert!(output.ends_with("bestmove a1a8\n"), "{output}");
    }

    #[test]
    fn quit_during_a_search_answers_and_then_quits() {
        let mut out = Vec::new();
        let mut commands = vec!["quit".to_string()];
        let mut waiting = || commands.pop();
        let go_on = Engine::new()
            .handle("go infinite", &mut waiting, &mut out)
            .unwrap();
        assert!(!go_on);
        assert!(String::from_utf8(out).unwrap().contains("bestmove "));
    }

    #[test]
    fn a_search_by_the_clock_stops_in_time() {
        for go in [
            "go movetime 60",
            "go wtime 1800 btime 1800 winc 0 binc 0",
            "go wtime 5000 btime 100 movestogo 100",
        ] {
            let started = Instant::now();
            let output = run(&mut Engine::new(), &[go]);
            let taken = started.elapsed();
            // Without the clock this search would never end. The bound is
            // loose for the sake of a busy machine.
            assert!(taken < Duration::from_secs(2), "{go}: {taken:?}");
            assert!(output.contains("info depth 3 "), "{go}: {output}");
            assert!(
                Position::initial()
                    .legal_moves()
                    .contains(&best_move(&output))
            );
        }
    }

    #[test]
    fn options_set_the_limits_of_the_search() {
        let mut engine = Engine::new();
        run(
            &mut engine,
            &[
                "setoption name Hash value 4",
                "setoption name Margin value 35",
                "setoption name Quiescence value 99",
                "setoption name Seed value 7",
                "setoption name Ponder value true",
            ],
        );
        let options = &engine.options;
        assert_eq!((options.hash, options.margin, options.seed), (4, 35, 7));
        assert_eq!(options.quiescence, MAX_QUIESCENCE);
        // Megabytes become a table of sixteen-byte entries.
        let megabytes = [0, 1, 2, 3, 16, 64, 1 << 20];
        assert_eq!(megabytes.map(table_bits), [0, 16, 17, 17, 20, 22, 22]);
    }

    fn limits(text: &str) -> Limits {
        Limits::parse(text.split_whitespace())
    }

    #[test]
    fn the_clock_gives_a_move_its_share_of_the_time_left() {
        let (white, black) = (Color::White, Color::Black);
        for (go, player, expected) in [
            // A set time for the move, less what is kept back for the
            // answer to arrive.
            ("movetime 60", white, Some(50)),
            // A thirtieth of the player's own clock.
            ("wtime 3000 btime 60000", white, Some(90)),
            ("wtime 3000 btime 60000", black, Some(1990)),
            // And three quarters of the player's own increment.
            ("wtime 3000 btime 3000 winc 1000", white, Some(840)),
            ("wtime 3000 btime 3000 winc 1000", black, Some(90)),
            ("wtime 3000 btime 3000 binc 1000", black, Some(840)),
            // An even share for each move left before the next control.
            ("wtime 3000 movestogo 5", white, Some(590)),
            ("wtime 3000 movestogo 0", white, Some(1490)),
            // Never more than half of what is left.
            ("wtime 1000 winc 5000", white, Some(490)),
            // Always something, even when the clock has run out.
            ("wtime 5", white, Some(1)),
            ("wtime -5", white, Some(1)),
            // With no clock for the player to move, no clock stops the
            // search.
            ("btime 3000", white, None),
            ("nodes 5000", white, None),
        ] {
            let time = limits(go).time(player).map(|time| time.as_millis());
            assert_eq!(time, expected, "go {go}, {player:?} to move");
        }
        // The largest numbers there are overflow nothing.
        let huge = format!("wtime {0} winc {0} movestogo 1", u64::MAX);
        assert!(limits(&huge).time(white).is_some());
    }

    #[test]
    fn a_number_out_of_range_is_read_as_the_nearest_in_range() {
        assert_eq!(number("12"), Some(12));
        assert_eq!(number("-5"), Some(0));
        assert_eq!(number("99999999999999999999999"), Some(u64::MAX));
        for text in ["", "-", "x", "1x", "+1", "1.5", "--1"] {
            assert_eq!(number(text), None, "`{text}`");
        }
        // A depth beyond the deepest search asks for the deepest.
        let deep = limits("depth 99999999999999999999999");
        assert_eq!(deep.depth, Some(MAX_DEPTH));
    }

    #[test]
    fn a_go_sets_the_limits_of_its_search() {
        let mut engine = Engine::new();
        run(
            &mut engine,
            &[
                "setoption name Hash value 4",
                "setoption name Margin value 35",
                "setoption name Quiescence value 5",
            ],
        );
        let settings = |go: &str, timed| engine.settings(&limits(go), timed);
        let expected = Settings {
            depth: MAX_DEPTH,
            nodes: 5_000,
            quiescence: 5,
            table_bits: 18,
            margin: 35,
        };
        assert_eq!(settings("nodes 5000", false), expected);
        // With no limit at all, a search is given a number of positions.
        assert_eq!(settings("", false).nodes, DEFAULT_NODES);
        // A limit of any other kind lifts that one.
        let four_deep = Settings {
            depth: 4,
            nodes: u64::MAX,
            ..expected
        };
        assert_eq!(settings("depth 4", false), four_deep);
        assert_eq!(settings("infinite", false).nodes, u64::MAX);
        assert_eq!(settings("wtime 1000", true).nodes, u64::MAX);
        assert_eq!(settings("depth 4 nodes 300", false).nodes, 300);
    }

    #[test]
    fn the_seed_and_the_margin_reach_the_search() {
        // How many different moves sixteen seeds choose from the start.
        let chosen = |margin: u32| {
            let mut moves: Vec<Move> = (0..16)
                .map(|seed| {
                    let margin = format!("setoption name Margin value {margin}");
                    let seed = format!("setoption name Seed value {seed}");
                    let output = run(&mut Engine::new(), &[&margin, &seed, "go nodes 2000"]);
                    best_move(&output)
                })
                .collect();
            moves.sort();
            moves.dedup();
            moves.len()
        };
        // With a margin of ten pawns every move is good enough, and the
        // seed alone chooses.
        let (exact, loose) = (chosen(0), chosen(1000));
        assert!(loose >= 5, "{loose} moves chosen with a wide margin");
        assert!(exact < loose, "{exact} moves chosen with no margin");
    }

    #[test]
    fn a_position_is_read_with_its_moves_and_a_new_game_forgets_it() {
        let mut engine = Engine::new();
        // Four fields of a FEN are enough, and the moves after them are
        // played.
        let castled = "position fen 4k3/8/8/8/8/8/8/4K2R w K - moves e1g1 e8d7";
        assert_eq!(run(&mut engine, &[castled]), "");
        assert_eq!(engine.game.moves().len(), 2);
        assert_eq!(
            engine.game.position().to_string(),
            "8/3k4/8/8/8/8/8/5RK1 w - - 2 2"
        );

        run(&mut engine, &["ucinewgame"]);
        assert_eq!(*engine.game.position(), Position::initial());
        assert!(engine.game.moves().is_empty());

        // The word that introduces the moves may not be left out.
        let output = run(&mut engine, &["position startpos e2e4"]);
        assert!(
            output.starts_with("info string position not understood"),
            "{output}"
        );
        assert!(engine.game.moves().is_empty());
    }

    #[test]
    fn a_game_drawn_by_the_rules_can_still_be_followed() {
        for (start, moves) in [
            // The rook's first move is the hundred and fiftieth half-move
            // without a capture or a pawn move, which draws the game.
            ("4k3/8/8/8/8/8/8/R3K3 w - - 149 100", "a1a2 e8e7 a2a1"),
            // Two kings cannot mate, so this game is drawn as it starts.
            ("8/8/4k3/8/8/4K3/8/8 w - - 0 1", "e3d2 e6e5"),
        ] {
            let mut expected = Position::from_fen(start).unwrap();
            for text in moves.split_whitespace() {
                expected = expected.play(Move::parse(text).unwrap()).unwrap();
            }

            let mut engine = Engine::new();
            let position = format!("position fen {start} moves {moves}");
            let output = run(&mut engine, &[&position, "go nodes 500"]);
            assert_eq!(*engine.game.position(), expected, "{output}");
            assert!(
                expected.legal_moves().contains(&best_move(&output)),
                "{output}"
            );
        }
    }

    #[test]
    fn what_follows_stop_is_kept_for_after_the_answer() {
        // The last of these arrives first.
        let mut arriving = vec![
            "go depth 1".to_string(),
            "position startpos moves e2e4".to_string(),
            "stop".to_string(),
        ];
        let mut out = Vec::new();
        let go_on = Engine::new()
            .handle("go infinite", &mut || arriving.pop(), &mut out)
            .unwrap();
        assert!(go_on);
        assert!(String::from_utf8(out).unwrap().contains("bestmove "));
        assert_eq!(arriving, ["go depth 1", "position startpos moves e2e4"]);
    }

    #[test]
    fn a_closed_input_tells_a_search_to_quit() {
        let (sender, lines) = mpsc::channel();
        sender.send("isready".to_string()).unwrap();
        assert_eq!(waiting_line(&lines), Some("isready".to_string()));
        assert_eq!(waiting_line(&lines), None);
        drop(sender);
        assert_eq!(waiting_line(&lines), Some("quit".to_string()));
    }

    #[test]
    fn an_endless_search_of_a_finished_game_waits_to_be_stopped() {
        let mut engine = Engine::new();
        run(
            &mut engine,
            &["position startpos moves f2f3 e7e5 g2g4 d8h4"],
        );
        let mut polls = 0;
        let mut waiting = || {
            polls += 1;
            (polls == 5).then(|| "stop".to_string())
        };
        let mut out = Vec::new();
        let go_on = engine
            .handle("go infinite", &mut waiting, &mut out)
            .unwrap();
        assert!(go_on);
        assert_eq!(polls, 5);
        assert_eq!(String::from_utf8(out).unwrap(), "bestmove 0000\n");
    }
}
