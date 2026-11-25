//! Reference suite for the chess rules.
//!
//! Each module holds a table of positions or games with the values the rules
//! must produce, and the test that checks the table. `REFERENCE.md` in the
//! crate root records where the values come from and how they were checked.
//!
//! Only `engine` calls the rules model, so the suite follows a change to the
//! model's interface by changing that one file.

mod engine;
mod games;
mod legal_moves;
mod outcomes;
mod perft;

/// The initial position.
const INITIAL: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Fails the test with every message collected, so one run shows all the
/// cases that disagree.
fn report(failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} disagree:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
