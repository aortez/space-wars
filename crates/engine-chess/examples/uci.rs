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
            let Some(value) = tokens.peek().and_then(|text| text.parse::<u64>().ok()) else {
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
                (left / moves + self.increment[side] * 3 / 4).min(left / 2)
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
                // Skip the word that introduces the moves.
                tokens.next();
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
            let mut played = play(&mut game, mv);
            if played == Err(ActionError::GameOver) {
                // The rules have ended the game, by a fifth repetition for
                // one, and the program plays on. Go on from the position
                // alone.
                game = Game::new(self.games, game.position().clone());
                played = play(&mut game, mv);
            }
            played.map_err(|error| format!("{text}: {error}"))?;
        }
        self.game = game;
        Ok(())
    }

    fn go(
        &mut self,
        limits: &Limits,
        waiting: &mut dyn FnMut() -> Option<String>,
        out: &mut dyn Write,
    ) -> io::Result<bool> {
        let player = self.game.position().side_to_move();
        let time = limits.time(player);
        let unbounded = limits.infinite || time.is_some() || limits.depth.is_some();
        let settings = Settings {
            depth: limits.depth.unwrap_or(MAX_DEPTH),
            nodes: limits
                .nodes
                .unwrap_or(if unbounded { u64::MAX } else { DEFAULT_NODES }),
            quiescence: self.options.quiescence,
            table_bits: table_bits(self.options.hash),
            margin: self.options.margin,
        };
        let Some(mut job) = SearchJob::new(&self.game, settings, self.options.seed) else {
            // The rules have ended the game, and the program asks anyway.
            match self.game.position().legal_moves().first() {
                Some(mv) => writeln!(out, "bestmove {mv}")?,
                None => writeln!(out, "bestmove 0000")?,
            }
            out.flush()?;
            return Ok(true);
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
                // Each depth takes several times as long as the last, so one
                // begun with under a third of the time gone is the last that
                // can finish.
                stopped |= time.is_some_and(|time| started.elapsed() * 10 >= time * 3);
            }
            stopped |= time.is_some_and(|time| started.elapsed() >= time);

            while let Some(line) = waiting() {
                match line.split_whitespace().next() {
                    Some("stop") => stopped = true,
                    Some("quit") => {
                        stopped = true;
                        quit = true;
                    }
                    Some("isready") => writeln!(out, "readyok")?,
                    _ => {}
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
        let mut waiting = || lines.try_recv().ok();
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
}
