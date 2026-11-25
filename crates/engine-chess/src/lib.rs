//! Deterministic, platform-independent chess rules, and a computer opponent
//! that plays by them.
//!
//! A [`Position`] is one arrangement of the board with the rights that go
//! with it. It finds the legal moves and plays them. A [`Game`] adds the
//! history that the draw rules depend on: it decides results, weighs draw
//! claims and accepts the players' actions. A [`search::SearchJob`] chooses
//! the action of the player to move, one bounded step at a time. None of
//! them owns a clock, a thread, a file or a player.
//!
//! `REFERENCE.md` records where the expected values of the reference suite in
//! `tests/rules` come from, and where the numbers of the search come from.

#![forbid(unsafe_code)]

mod bitboard;
mod board;
mod fen;
mod game;
mod moves;
mod position;
pub mod search;
#[cfg(test)]
mod tests;
mod zobrist;

pub use board::{Color, Piece, PieceKind, Square};
pub use fen::FenError;
pub use game::{
    Action, ActionError, DrawClaim, DrawReason, Ending, Game, Outcome, Revision, SNAPSHOT_VERSION,
    Snapshot, SnapshotError, Transition,
};
pub use moves::Move;
pub use position::Position;
