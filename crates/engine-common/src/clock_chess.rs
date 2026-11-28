//! What Clock publishes about a visit to its chess board: what the visit is
//! doing with the board, and the birds that came for it.

use serde::{Deserialize, Serialize};

/// What a visit to the chess board is doing with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockChessPhase {
    /// The duck brings the board, packed in its case, and the crow flies
    /// in.
    Arriving,
    /// The case opens out into the board.
    Opening,
    /// The pieces come out onto their squares.
    Placing,
    /// The birds play.
    Playing,
    /// The pieces go back.
    Clearing,
    /// The board folds back into its case.
    Closing,
    /// The crow flies out, and the duck takes the case away.
    Leaving,
}

impl ClockChessPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Arriving => "arriving",
            Self::Opening => "opening",
            Self::Placing => "placing",
            Self::Playing => "playing",
            Self::Clearing => "clearing",
            Self::Closing => "closing",
            Self::Leaving => "leaving",
        }
    }
}

/// A bird of a visit to the chess board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockChessBird {
    Crow,
    Duck,
}

impl ClockChessBird {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crow => "crow",
            Self::Duck => "duck",
        }
    }
}

/// What a bird of a visit to the chess board is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockChessPose {
    /// On its way to the board.
    Arriving,
    /// Setting the board up.
    Setting,
    /// At the board, between its moves.
    Waiting,
    /// Reaching for its piece and moving it.
    Moving,
    /// The game has ended, and the bird won it.
    Won,
    /// The game has ended, and the bird lost it.
    Lost,
    /// The game has ended in a draw.
    Drawn,
    /// Putting the board away.
    Packing,
    /// On its way out.
    Leaving,
}

impl ClockChessPose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Arriving => "arriving",
            Self::Setting => "setting",
            Self::Waiting => "waiting",
            Self::Moving => "moving",
            Self::Won => "won",
            Self::Lost => "lost",
            Self::Drawn => "drawn",
            Self::Packing => "packing",
            Self::Leaving => "leaving",
        }
    }
}

/// A bird of a visit to the chess board, while it is in the scene. It
/// belongs to that visit: it is no crow or duck of a visit of its own, it is
/// in neither of their states, and nobody takes it over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockChessActor {
    pub bird: ClockChessBird,
    /// Whether the bird plays White.
    pub white: bool,
    pub pose: ClockChessPose,
    /// Where the bird is, in thousandths of a world unit: for the duck, the
    /// middle of its body, and for the crow, its feet.
    pub position_milli: [i32; 2],
    pub facing_right: bool,
    /// Whether the bird has the packed board with it.
    pub carrying: bool,
}

/// A visit to the chess board below the digits, with its board and the
/// birds that came for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockChessState {
    /// Which of the run's visits to the board this is, from one. Its board
    /// and its birds share it.
    pub visit_id: u64,
    /// Which of the run's matches the visit plays, from one.
    pub match_number: u64,
    pub phase: ClockChessPhase,
    /// Ticks into the phase.
    pub phase_tick: u64,
    /// How many actions the match's game has accepted.
    pub actions: u32,
    /// The board's place, in thousandths of a world unit: its left, bottom,
    /// right and top.
    pub board_milli: [i32; 4],
    /// The visit's birds that are in the scene, the crow before the duck.
    pub actors: Vec<ClockChessActor>,
}
