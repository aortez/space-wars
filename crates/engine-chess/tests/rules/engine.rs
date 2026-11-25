//! The one place where this suite calls the rules model.
//!
//! The tables state positions as FEN and moves as their squares. This file
//! turns those into calls on `engine-chess` and the answers back into the
//! plain values below, so no other file in the suite names the model's types.
//!
//! A move is its origin and destination square, followed by `q`, `r`, `b` or
//! `n` when a pawn promotes: `e2e4`, `e7e8q`. Castling is the king's move:
//! `e1g1`.

use std::collections::{BTreeMap, BTreeSet};

use engine_chess::{Action, Color, DrawClaim, DrawReason, Game, Move, Outcome, Position};

/// How a game stands. A drawn game names the rule that drew it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameResult {
    Ongoing,
    WhiteWinsByCheckmate,
    BlackWinsByCheckmate,
    Stalemate,
    /// Neither player can ever give checkmate with the material on the board.
    DeadPosition,
    FivefoldRepetition,
    SeventyFiveMoves,
}

/// The draws a player may claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Claims {
    /// The position has appeared, or would appear, for at least the third time.
    pub threefold: bool,
    /// Each player's last fifty moves were made without a capture or a pawn move.
    pub fifty: bool,
}

/// How a replayed game stands. The suite reads `result` and `check` for every
/// game, and the other fields only while the game is ongoing.
pub struct Replay {
    pub result: GameResult,
    /// Whether the player to move is in check.
    pub check: bool,
    /// Every legal move of the player to move.
    pub legal_moves: BTreeSet<String>,
    /// The draws the player to move may claim as the position stands.
    pub claims_now: Claims,
    /// For each legal move, the draws the player may claim by naming it as the
    /// move they intend to play. No case relies on a move that would itself
    /// end the game.
    pub claims_with: BTreeMap<String, Claims>,
}

fn position(fen: &str) -> Position {
    Position::from_fen(fen).unwrap_or_else(|error| panic!("`{fen}` was refused: {error}"))
}

/// Counts the sequences of `depth` legal half-moves that start from `fen`.
///
/// Only the rules of movement apply. A draw by repetition, by move count or
/// by dead position does not end a sequence, so this counts on the position
/// and not on a game.
pub fn perft(fen: &str, depth: u32) -> u64 {
    position(fen).perft(depth)
}

/// Starts a game from `start`, plays `moves` and reports how the game stands.
///
/// `start` is a FEN with all six fields. Its half-move clock counts toward the
/// fifty-move and seventy-five-move rules, and no position came before it.
/// Every move is legal and none follows the end of the game. A case that
/// breaks either rule is wrong, so this panics rather than report it.
pub fn replay(start: &str, moves: &[&str]) -> Replay {
    let mut game = Game::new(0, position(start));
    for (index, text) in moves.iter().enumerate() {
        let mv = Move::parse(text).unwrap_or_else(|| panic!("`{text}` is not a move"));
        let player = game.position().side_to_move();
        game.apply(game.revision(), player, Action::Move(mv))
            .unwrap_or_else(|error| panic!("move {} ({text}) was refused: {error}", index + 1));
    }

    let result = match game.outcome() {
        None => GameResult::Ongoing,
        Some(Outcome::Checkmate {
            winner: Color::White,
        }) => GameResult::WhiteWinsByCheckmate,
        Some(Outcome::Checkmate {
            winner: Color::Black,
        }) => GameResult::BlackWinsByCheckmate,
        Some(Outcome::Draw(DrawReason::Stalemate)) => GameResult::Stalemate,
        Some(Outcome::Draw(DrawReason::DeadPosition)) => GameResult::DeadPosition,
        Some(Outcome::Draw(DrawReason::FivefoldRepetition)) => GameResult::FivefoldRepetition,
        Some(Outcome::Draw(DrawReason::SeventyFiveMoves)) => GameResult::SeventyFiveMoves,
        // Nobody resigns or claims a draw while a game is replayed.
        Some(other) => panic!("moves alone ended the game with {other:?}"),
    };
    let claims = |holds: &dyn Fn(DrawClaim) -> bool| Claims {
        threefold: holds(DrawClaim::Threefold),
        fifty: holds(DrawClaim::FiftyMoves),
    };
    let legal = game.legal_moves();
    Replay {
        result,
        check: game.position().is_check(),
        legal_moves: legal.iter().map(Move::to_string).collect(),
        claims_now: claims(&|claim| game.can_claim(claim)),
        claims_with: legal
            .iter()
            .map(|&mv| {
                let with = claims(&|claim| game.can_claim_with(claim, mv));
                (mv.to_string(), with)
            })
            .collect(),
    }
}
