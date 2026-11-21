//! L3 ground truth: what the game resists.
//!
//! L2 records what is drawn and cannot say what is solid, because solidity is not
//! a property of a picture: a pipe is a surface from above and a wall from the
//! side, a hill is opaque and never a wall, a block changes identity when hit. So
//! this layer runs the experiment instead of making the inference. When SMB blocks
//! a body it takes the motion back out of the speed, so the difference between the
//! motion the buttons asked for and the motion that came out is a verdict from the
//! game, and `Resistance` records that verdict and nothing else.
//!
//! It is keyed on the CHR pattern hash, not the nametable number, per SMB-002: a
//! refusal belongs to the artwork, and the same wall drawn with a different number
//! in another area is still the same wall.

use crate::frame::{
    PpuBytes, is_blank_tile, pattern_bytes, pattern_hash, player_tile, tile_at_world_pixel,
};
use crate::profile::SmbState;
use engine_nes::ControllerButtons;
use std::collections::BTreeSet;

/// Which way the body was trying to go when the game refused it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Right,
    Left,
}

impl Side {
    /// The column a refused cell occupies, relative to the body's own column.
    pub fn column_offset(self) -> i16 {
        match self {
            Self::Right => 1,
            Self::Left => -1,
        }
    }

    fn button(self) -> ControllerButtons {
        match self {
            Self::Right => ControllerButtons::RIGHT,
            Self::Left => ControllerButtons::LEFT,
        }
    }

    fn opposed_by(self, buttons: ControllerButtons) -> bool {
        buttons.contains(self.button())
            && !buttons.contains(match self {
                Self::Right => ControllerButtons::LEFT,
                Self::Left => ControllerButtons::RIGHT,
            })
    }
}

/// The first world pixel to the right of the body the core collides with.
///
/// `$0086` is the left edge of the sprite, and the box the game stops against the
/// level is not the sprite. Two independent refusals on the same cartridge say where
/// it is: the game holds him at absolute X 594 against a pipe whose left edge is at
/// 608, and at absolute X 898 against a pipe whose left edge is at 912. Both leave
/// exactly 14 pixels, so the box is `[x, x + 14)`, the last pixel he occupies is
/// `x + 13`, and the cell that can refuse him is the one containing `x + 14`.
///
/// The earlier number here was 11, read off the decoded grid rather than off the
/// level. That reading was wrong for the reason [`tile_at_world_pixel`] explains: the
/// grid is indexed by screen column, so asking it which cell a pixel falls in is up
/// to one column out whenever the fine scroll is not zero.
pub const BODY_RIGHT_REACH_PIXELS: u16 = 14;

/// The same measurement on the left, still at the conservative end of its range:
/// the pixel his stored X is on. Nothing has been refused from the left yet, so
/// there is no second refusal to fix it with, and a leftward wall can only ever
/// look further off than it is.
pub const BODY_LEFT_REACH_PIXELS: u16 = 0;

/// The world pixel the body's leading edge is at on the way to that side.
pub fn leading_pixel(state: &SmbState, side: Side) -> u16 {
    match side {
        Side::Right => state.absolute_x + BODY_RIGHT_REACH_PIXELS,
        Side::Left => state.absolute_x.saturating_sub(BODY_LEFT_REACH_PIXELS),
    }
}

/// Rows above the feet row that his body occupies: small Mario is two tiles tall.
pub const BODY_ROWS: i16 = 2;

/// The cell `step` cells beyond the body's leading edge, `rise` rows above his feet
/// row, read through [`tile_at_world_pixel`]. `None` when it is off the visible
/// screen or off the top or bottom of the level.
pub fn cell_beyond(
    frame: &PpuBytes,
    origin_x: u16,
    state: &SmbState,
    side: Side,
    step: i16,
    rise: i16,
) -> Option<u8> {
    let edge = i32::from(leading_pixel(state, side));
    let step = i32::from(step) * 8;
    let world_x = match side {
        Side::Right => edge + step,
        Side::Left => edge - step,
    };
    let world_x = u16::try_from(world_x).ok()?;
    let row = player_tile(state, origin_x).1 - rise;
    tile_at_world_pixel(frame, origin_x, world_x, row)
}

/// What the body was doing on the frame a command was issued, so that the next
/// frame can say what the game did with it.
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub x: u16,
    pub h_speed: i8,
    pub grounded: bool,
    pub asked: ControllerButtons,
}

impl Motion {
    /// The motion a frame's buttons were asking for.
    pub fn asked(state: &SmbState, buttons: ControllerButtons) -> Self {
        Self {
            x: state.absolute_x,
            h_speed: state.h_speed,
            grounded: state.is_grounded(),
            asked: buttons,
        }
    }
}

/// A single refusal, recorded where it happened so a claim about it can be replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The CHR pattern of the tile that refused him.
    pub pattern: u64,
    /// The number it was drawn with on that frame, for reporting only.
    pub tile: u8,
    /// Row relative to his feet row. Only -1, the row a grounded body is pressed
    /// against, is recorded; a head bump is a different relation and is not here.
    pub row_offset: i16,
    pub side: Side,
    pub absolute_x: u16,
    pub frame: u64,
}

/// Everything the game has refused so far, keyed by the pattern that refused it.
///
/// This is the model's memory of solidity, and it is what a player has after
/// walking into a pipe once. It survives a death and a level change because a
/// wall does not move, and it is empty at the start of a cartridge, which is
/// honest: nothing has been refused yet.
#[derive(Clone, Debug, Default)]
pub struct Resistance {
    cells: BTreeSet<(u64, i16, Side)>,
    log: Vec<Refusal>,
}

impl Resistance {
    /// Rebuild a memory from refusals recorded elsewhere, which is how a gym hands
    /// a model what a previous run learned without replaying the run.
    pub fn from_log(log: &[Refusal]) -> Self {
        Self {
            cells: log
                .iter()
                .map(|r| (r.pattern, r.row_offset, r.side))
                .collect(),
            log: log.to_vec(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// How many distinct pattern, row and direction triples have been refused.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Every refusal in the order the game made them.
    pub fn log(&self) -> &[Refusal] {
        &self.log
    }

    /// Whether this pattern has refused this body row from this side.
    pub fn refused(&self, pattern: u64, row_offset: i16, side: Side) -> bool {
        self.cells.contains(&(pattern, row_offset, side))
    }

    /// Compare the motion a frame's buttons asked for with the motion the core
    /// produced, and attribute the difference to the cell at the body's leading
    /// edge.
    ///
    /// The three conditions are the game's own signature, not a threshold: the
    /// direction was held without its opposite, the body gained no ground at all,
    /// and the speed the buttons had built up came back zero, which is what the
    /// core does when it subtracts a blocked move. A frame that merely moved
    /// slowly is not a refusal, so nothing here needs a tuning constant.
    pub fn observe(
        &mut self,
        asked: Motion,
        produced: &SmbState,
        frame: &PpuBytes,
        origin_x: u16,
        at_frame: u64,
    ) -> Option<Refusal> {
        if !asked.grounded || !produced.is_grounded() || !produced.is_in_control() {
            return None;
        }
        if produced.absolute_x != asked.x {
            return None;
        }
        let mut first = None;
        for side in [Side::Right, Side::Left] {
            let moving = match side {
                Side::Right => asked.h_speed > 0,
                Side::Left => asked.h_speed < 0,
            };
            if !moving || produced.h_speed != 0 || !side.opposed_by(asked.asked) {
                continue;
            }
            // The run of drawn cells from the row his body rests on upwards, in the
            // cell his box would enter next. The first blank one is air he would have
            // walked through, and anything above that is not part of what stopped
            // him.
            for rise in 1..=BODY_ROWS {
                let Some(tile) = cell_beyond(frame, origin_x, produced, side, 0, rise) else {
                    break;
                };
                if is_blank_tile(frame, tile) {
                    break;
                }
                let refusal = Refusal {
                    pattern: pattern_hash(pattern_bytes(frame, tile)),
                    tile,
                    row_offset: -rise,
                    side,
                    absolute_x: produced.absolute_x,
                    frame: at_frame,
                };
                self.cells
                    .insert((refusal.pattern, refusal.row_offset, refusal.side));
                self.log.push(refusal);
                if first.is_none() {
                    first = Some(refusal);
                }
            }
        }
        first
    }
}
