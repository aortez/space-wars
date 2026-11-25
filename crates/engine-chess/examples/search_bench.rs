//! Times the move generator and each level of the search on a few fixed
//! positions, for sizing the levels' budgets on a new machine.
//!
//! ```text
//! cargo run --release -p engine-chess --example search_bench
//! ```

use std::time::{Duration, Instant};

use engine_chess::search::{Level, SearchJob};
use engine_chess::{Action, Game, Position};

const POSITIONS: [(&str, &str); 5] = [
    (
        "start",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    ),
    (
        "kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    ),
    (
        "middlegame",
        "r1bq1rk1/pp2ppbp/2np1np1/8/3NP3/2N1BP2/PPPQ2PP/R3KB1R w KQ - 3 9",
    ),
    ("rook-endgame", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    (
        "all-captures",
        "q2k2q1/2nqn2b/1n1P1n1b/2rnr2Q/1NQ1QN1Q/3Q3B/2RQR2B/Q2K2Q1 w - - 0 1",
    ),
];

fn main() {
    let started = Instant::now();
    let counted = Position::initial().perft(5);
    let seconds = started.elapsed().as_secs_f64();
    println!(
        "move generation: {counted} sequences of 5 half-moves in {:.3} s, {:.1} million a second",
        seconds,
        counted as f64 / seconds / 1e6
    );

    println!(
        "\n{:<7} {:<13} {:>5} {:>9} {:>9} {:>9} {:>9} {:>10}  action",
        "level", "position", "depth", "nodes", "ms", "knodes/s", "start ms", "slowest µs"
    );
    for level in Level::ALL {
        let settings = level.settings();
        let mut memory = 0;
        for (name, fen) in POSITIONS {
            let game = Game::new(0, Position::from_fen(fen).expect("a valid position"));

            let started = Instant::now();
            let mut job = SearchJob::new(&game, settings, 0).expect("the game is not over");
            let starting = started.elapsed();
            memory = job.memory_bytes();

            // The longest a host waits for one step, which is how soon it
            // can stop a search.
            let mut slowest = Duration::ZERO;
            let mut probe = job.clone();
            for _ in 0..20_000 {
                let started = Instant::now();
                probe.step();
                slowest = slowest.max(started.elapsed());
            }

            let started = Instant::now();
            job.run(u64::MAX);
            let seconds = started.elapsed().as_secs_f64();
            let decision = job.decision().expect("the search finished");
            let action = match decision.action {
                Action::Move(mv) => mv.to_string(),
                other => format!("{other:?}"),
            };
            println!(
                "{:<7} {:<13} {:>5} {:>9} {:>9.1} {:>9.0} {:>9.2} {:>10.1}  {action} ({})",
                format!("{level:?}"),
                name,
                decision.depth,
                decision.nodes,
                seconds * 1e3,
                decision.nodes as f64 / seconds / 1e3,
                starting.as_secs_f64() * 1e3,
                slowest.as_secs_f64() * 1e6,
                decision
                    .score
                    .map_or("no score".to_string(), |score| score.to_string()),
            );
        }
        println!("{:<7} holds {} KiB", format!("{level:?}"), memory / 1024);
    }
}
