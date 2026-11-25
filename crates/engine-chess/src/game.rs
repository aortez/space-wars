//! A game: the positions reached so far, its result, and the players' actions.

use std::fmt;

use crate::board::Color;
use crate::fen::FenError;
use crate::moves::Move;
use crate::position::Position;

/// The version written into every [`Snapshot`].
pub const SNAPSHOT_VERSION: u32 = 1;

/// Identifies one state of one game. An action names the revision it was
/// chosen for, so one chosen for an earlier state or another game is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Revision {
    /// The game, as numbered by whoever started it.
    pub session: u64,
    /// How many actions the game has accepted.
    pub actions: u32,
}

/// A draw that a player may claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawClaim {
    /// The same position has appeared at least three times.
    Threefold,
    /// Each player's last fifty moves had no capture and no pawn move.
    FiftyMoves,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawReason {
    Stalemate,
    /// Neither player can ever give checkmate with the pieces left.
    DeadPosition,
    FivefoldRepetition,
    SeventyFiveMoves,
    ThreefoldClaim,
    FiftyMoveClaim,
}

/// How a game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Checkmate { winner: Color },
    Resignation { winner: Color },
    Draw(DrawReason),
}

/// Something a player does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Move(Move),
    /// Claims a draw as the position stands or, with `intended`, by naming
    /// the move the player is about to play. The move is not played.
    ClaimDraw {
        claim: DrawClaim,
        intended: Option<Move>,
    },
    /// Gives the game to the opponent, whatever pieces they have left.
    Resign,
}

/// Why an action was refused. A refused action changes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    /// The action was chosen for another state of the game, or another game.
    Stale,
    GameOver,
    /// Only the player to move may move or claim a draw.
    OutOfTurn,
    IllegalMove,
    ClaimNotValid,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stale => "the action was chosen for another state of the game",
            Self::GameOver => "the game is over",
            Self::OutOfTurn => "it is the other player's turn",
            Self::IllegalMove => "the move is not legal",
            Self::ClaimNotValid => "the draw claim is not valid",
        })
    }
}

impl std::error::Error for ActionError {}

/// What an accepted action led to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    /// The revision that the next action must name.
    pub revision: Revision,
    pub outcome: Option<Outcome>,
}

/// An action other than a move that ended a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Resigned(Color),
    DrawClaimed {
        claim: DrawClaim,
        intended: Option<Move>,
    },
}

/// Everything needed to rebuild a game, repetition history included. A
/// position alone would not do: it does not say which positions came before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub version: u32,
    /// The position the game started from, as FEN.
    pub start: String,
    pub moves: Vec<Move>,
    pub ending: Option<Ending>,
}

/// Why a snapshot was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    /// The snapshot was written by a version this one does not read.
    Version(u32),
    Start(FenError),
    /// The move at this place in the list could not be played.
    Move(usize),
    /// The recorded ending could not have happened in the final position.
    Ending,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version(version) => write!(f, "snapshot version {version} is not supported"),
            Self::Start(error) => write!(f, "the starting position is not valid: {error}"),
            Self::Move(index) => write!(f, "move {} could not be played", index + 1),
            Self::Ending => f.write_str("the recorded ending could not have happened"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// A game in progress or finished.
///
/// The game decides checkmate, stalemate and the automatic draws itself.
/// Draws that the rules leave to the players stay claims: a position that has
/// appeared three times is not over until a player claims it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    session: u64,
    /// Every position reached, the starting position first.
    positions: Vec<Position>,
    moves: Vec<Move>,
    ending: Option<Ending>,
    outcome: Option<Outcome>,
    actions: u32,
}

impl Game {
    /// Starts a game from `start`. `session` tells this game's revisions
    /// apart from those of every other game.
    pub fn new(session: u64, start: Position) -> Self {
        let outcome = automatic_outcome(&start, 1);
        Self {
            session,
            positions: vec![start],
            moves: Vec::new(),
            ending: None,
            outcome,
            actions: 0,
        }
    }

    /// The current position.
    pub fn position(&self) -> &Position {
        self.positions
            .last()
            .expect("a game has a starting position")
    }

    /// Every position reached, the starting position first and the current
    /// one last.
    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    pub fn revision(&self) -> Revision {
        Revision {
            session: self.session,
            actions: self.actions,
        }
    }

    /// How the game ended, or `None` while it is still on.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    /// The legal moves of the player to move. A finished game has none.
    pub fn legal_moves(&self) -> Vec<Move> {
        if self.outcome.is_some() {
            return Vec::new();
        }
        self.position().legal_moves()
    }

    /// Whether the player to move may claim this draw as the position stands.
    pub fn can_claim(&self, claim: DrawClaim) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        let (earlier, current) = self.positions.split_at(self.positions.len() - 1);
        claim_holds(claim, &current[0], occurrences(earlier, &current[0]))
    }

    /// Whether the player to move may claim this draw by naming `intended` as
    /// the move they are about to play. A move that would itself end the game
    /// supports no claim.
    pub fn can_claim_with(&self, claim: DrawClaim, intended: Move) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        let Some(next) = self.position().play(intended) else {
            return false;
        };
        let times = occurrences(&self.positions, &next);
        automatic_outcome(&next, times).is_none() && claim_holds(claim, &next, times)
    }

    /// Carries out `action` for `player`, if it was chosen for the current
    /// revision and the rules allow it.
    pub fn apply(
        &mut self,
        revision: Revision,
        player: Color,
        action: Action,
    ) -> Result<Transition, ActionError> {
        if revision != self.revision() {
            return Err(ActionError::Stale);
        }
        if self.outcome.is_some() {
            return Err(ActionError::GameOver);
        }
        let in_turn = player == self.position().side_to_move();
        match action {
            Action::Resign => self.end(Ending::Resigned(player)),
            Action::Move(_) | Action::ClaimDraw { .. } if !in_turn => {
                return Err(ActionError::OutOfTurn);
            }
            Action::Move(mv) => {
                let next = self.position().play(mv).ok_or(ActionError::IllegalMove)?;
                self.outcome = automatic_outcome(&next, occurrences(&self.positions, &next));
                self.positions.push(next);
                self.moves.push(mv);
            }
            Action::ClaimDraw { claim, intended } => {
                let valid = match intended {
                    Some(mv) => self.can_claim_with(claim, mv),
                    None => self.can_claim(claim),
                };
                if !valid {
                    return Err(ActionError::ClaimNotValid);
                }
                self.end(Ending::DrawClaimed { claim, intended });
            }
        }
        self.actions += 1;
        Ok(Transition {
            revision: self.revision(),
            outcome: self.outcome,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: SNAPSHOT_VERSION,
            start: self.positions[0].to_string(),
            moves: self.moves.clone(),
            ending: self.ending,
        }
    }

    /// Rebuilds a game by playing the snapshot through the same rules as a
    /// live game, so a snapshot that could not have happened is refused.
    pub fn restore(session: u64, snapshot: &Snapshot) -> Result<Self, SnapshotError> {
        if snapshot.version != SNAPSHOT_VERSION {
            return Err(SnapshotError::Version(snapshot.version));
        }
        let start = Position::from_fen(&snapshot.start).map_err(SnapshotError::Start)?;
        let mut game = Self::new(session, start);
        for (index, &mv) in snapshot.moves.iter().enumerate() {
            let player = game.position().side_to_move();
            game.apply(game.revision(), player, Action::Move(mv))
                .map_err(|_| SnapshotError::Move(index))?;
        }
        let ending = match snapshot.ending {
            None => return Ok(game),
            Some(Ending::Resigned(player)) => (player, Action::Resign),
            Some(Ending::DrawClaimed { claim, intended }) => (
                game.position().side_to_move(),
                Action::ClaimDraw { claim, intended },
            ),
        };
        game.apply(game.revision(), ending.0, ending.1)
            .map_err(|_| SnapshotError::Ending)?;
        Ok(game)
    }

    fn end(&mut self, ending: Ending) {
        self.ending = Some(ending);
        self.outcome = Some(match ending {
            Ending::Resigned(player) => Outcome::Resignation {
                winner: player.opposite(),
            },
            Ending::DrawClaimed { claim, .. } => Outcome::Draw(match claim {
                DrawClaim::Threefold => DrawReason::ThreefoldClaim,
                DrawClaim::FiftyMoves => DrawReason::FiftyMoveClaim,
            }),
        });
    }
}

/// How many times `position` has appeared, counting itself, when it follows
/// the positions in `earlier`.
fn occurrences(earlier: &[Position], position: &Position) -> usize {
    // A repeat has the same player to move, so it lies an even number of
    // half-moves back, and no further back than the last capture or pawn move.
    let pairs_back = position.halfmove_clock() as usize / 2;
    let repeats = earlier
        .iter()
        .rev()
        .skip(1)
        .step_by(2)
        .take(pairs_back)
        .filter(|before| before.repeats(position))
        .count();
    repeats + 1
}

/// The result that the rules impose on `position` without any claim, given
/// how many times it has appeared.
fn automatic_outcome(position: &Position, times: usize) -> Option<Outcome> {
    if !position.has_legal_move() {
        return Some(if position.is_check() {
            Outcome::Checkmate {
                winner: position.side_to_move().opposite(),
            }
        } else {
            Outcome::Draw(DrawReason::Stalemate)
        });
    }
    let reason = if position.has_dead_material() {
        DrawReason::DeadPosition
    } else if times >= 5 {
        DrawReason::FivefoldRepetition
    } else if position.halfmove_clock() >= 150 {
        DrawReason::SeventyFiveMoves
    } else {
        return None;
    };
    Some(Outcome::Draw(reason))
}

fn claim_holds(claim: DrawClaim, position: &Position, times: usize) -> bool {
    match claim {
        DrawClaim::Threefold => times >= 3,
        DrawClaim::FiftyMoves => position.halfmove_clock() >= 100,
    }
}
