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

/// Pixels across one level cell. The level is laid out on this grid in world pixels,
/// independently of the scroll, which is why the boundary test below divides the world
/// pixel rather than the screen pixel.
pub const LEVEL_CELL_PIXELS: u16 = 8;

/// How far past the nearest cell boundary the body's leading edge sits, measured in the
/// direction it was going. Zero means flush against the boundary.
pub fn overshoot_past_cell_edge(state: &SmbState, side: Side) -> u16 {
    let lead = leading_pixel(state, side);
    match side {
        Side::Right => lead % LEVEL_CELL_PIXELS,
        Side::Left => (LEVEL_CELL_PIXELS - lead % LEVEL_CELL_PIXELS) % LEVEL_CELL_PIXELS,
    }
}

/// Whether the body sits where a cell could have left it.
///
/// The level is laid out on a grid, and the game stops a sideways body by taking the
/// speed out and setting it down against the cell that refused it, so a frame the level
/// decided leaves the box flush on a boundary. Measured over twelve runs of six levels
/// with both players, 5098 of the 5123 frames that carry the zeroed-speed signature with
/// something drawn at the leading edge are flush, and the flush test alone is what keeps
/// the game's own clamps out: a body that has fallen into a pit and been pinned to the
/// edge of a locked screen zeroes its speed every frame with no cell involved, at an
/// offset the grid does not explain — 4 frames of the camera-limit gym, where the leading
/// edge sits six past an edge, and 87 at the 5-1 scroll limit, five past one.
///
/// The other 25 frames split in two, and only one half is a wall. Sixteen sit one or two
/// pixels past a boundary at a pipe or a solid block: the frame a body first touches a
/// cell, before the game has pulled it back to the boundary, naming the same cell the
/// flush frame names. Nine sit five past one at the 5-1 scroll limit and are clamps.
///
/// A touch frame is dropped while the body is on the ground and kept while it is in the
/// air, which is a completeness argument rather than a tolerance. On the ground the game
/// holds a body at the boundary for as long as the direction is held, so the flush frame
/// follows and the touch adds a second report of a cell already reported. In the air the
/// speed comes out, the body falls away, and the boundary is never reached: the two cells
/// the reflex learns in 1-1 about the pipe body at absolute X 912, and the one it learns
/// in 6-2 about the solid block at 816, are named by airborne touch frames and by nothing
/// else, and a flush-only ledger has never heard of them. The bound on how far past a
/// boundary such a frame may sit is the distance the body covered that frame, which is the
/// most an entering body can get past an edge it has not been pushed back from.
pub fn body_at_cell_edge(asked: Motion, produced: &SmbState, side: Side) -> bool {
    let overshoot = overshoot_past_cell_edge(produced, side);
    if overshoot == 0 {
        return true;
    }
    if asked.grounded {
        return false;
    }
    let travel = i32::from(produced.absolute_x) - i32::from(asked.x);
    overshoot <= u16::try_from(travel.abs()).unwrap_or(u16::MAX)
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
///
/// All four fields end up in the signature, in two steps. `h_speed` and `asked` are the
/// refusal itself: a speed the buttons had built came back zero while the direction was
/// still held. `grounded` and `x` belong to the harder question of where the game leaves
/// a body it has stopped, which [`body_at_cell_edge`] answers — the stance decides
/// whether a frame sitting part-past a cell boundary counts at all, and the position
/// bounds how far past it may sit, by the ground the body covered that frame.
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

/// Whether a frame carries the game's sideways-refusal signature on one side, without
/// looking at what is drawn there.
///
/// These are the three conditions [`Resistance::observe`] charges a cell on, split out so
/// the `solidity` example can print the same census the ledger keeps rather than a
/// hand-copied approximation of it that drifts whenever the condition moves.
pub fn refusal_signature(asked: Motion, produced: &SmbState, side: Side) -> bool {
    let moving = match side {
        Side::Right => asked.h_speed > 0,
        Side::Left => asked.h_speed < 0,
    };
    moving
        && produced.h_speed == 0
        && produced.is_in_control()
        && side.opposed_by(asked.asked)
        && body_at_cell_edge(asked, produced, side)
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
    /// Three conditions, each the game's own signature rather than a threshold. The
    /// direction was held without its opposite, and the speed the buttons had built up
    /// came back zero: nothing else writes zero over a speed the core was building while
    /// the body is in control and still being told to go, so a frame that merely moved
    /// slowly is not a refusal and no tuning constant is needed. And the body sits flush
    /// against a cell boundary on that side, or inside the cell by no more than the frame
    /// covered, which is where the game leaves a body it has stopped, and which
    /// [`body_at_cell_edge`] explains with the twelve-run census behind it.
    ///
    /// There used to be a different third condition — that the body gained no ground at
    /// all — and it was not the game's signature. On the frame a body is refused, the
    /// core clamps it *to* the wall, so the position still moves; on the frame after, the
    /// speed this method would have asked about is already zero. Read together the two say
    /// a refusal lasts exactly one frame and this detector was looking for it on neither of
    /// them, which is survivable while the body walks into the wall and fatal when it
    /// arrives in the air. Measured over six levels under the reflex: 1806 frames carry the
    /// zeroed-speed signature, and in 6-2 and 2-2 — the two where both players end up
    /// pressed against a pipe body — **none** of them is grounded on both sides of the
    /// frame, because a body that reaches a wall by jumping is never standing on the frame
    /// the game says no on. The old detector recorded nothing in either level, so
    /// `wall_ahead` still called those pipes scenery six hundred frames later and the
    /// script hopped at them until its stall budget ran out.
    ///
    /// The boundary test is what keeps the widened signature honest, and it is not the
    /// blank-cell rule doing that job. A speed change with nothing drawn at the leading
    /// edge already records nothing, and of the 1806 frames only 79 have nothing there;
    /// the rest of the false positives are the game's own clamps, which have plenty drawn
    /// beside them and were being recorded as lessons until the flush test went back in.
    pub fn observe(
        &mut self,
        asked: Motion,
        produced: &SmbState,
        frame: &PpuBytes,
        origin_x: u16,
        at_frame: u64,
    ) -> Option<Refusal> {
        let mut first = None;
        for side in [Side::Right, Side::Left] {
            if !refusal_signature(asked, produced, side) {
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
