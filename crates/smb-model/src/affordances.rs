//! L4 affordances: the situations a body can act on.
//!
//! A skill may ask whether a gap, a wall or an enemy in reach is present, and
//! which cell each is about. It may not ask for an address, a tile number, a
//! pattern hash or a PPU register: those are profile and frame vocabulary, and a
//! skill that reaches for them has stopped being portable to a fabricated level or
//! a second cartridge. The four questions and the forbidden vocabulary are
//! decided in `docs/smb-execution-plan.md` under Phase 0.
//!
//! `wall_ahead` is the one place the model's two sources of solidity meet: the
//! load-bearing set, which is what the game has held him up on, and the ledger,
//! which is what the game has refused to let him through.

use crate::frame::{
    PpuBytes, TILE_CELLS, is_load_bearing, pattern_bytes, pattern_hash, tile_at,
    tile_at_world_pixel,
};
use crate::profile::SmbState;
use crate::resistance::{BODY_ROWS, Resistance, Side, cell_beyond};

/// Columns right of the player to look for a hole. Roughly what a running jump
/// carries, so anything further is not this policy's problem yet.
pub const REACH_COLUMNS: i16 = 5;

/// Rows below the feet row that still count as ground, so a ledge one step down is
/// not read as a hole.
pub const SUPPORT_ROWS: i16 = 2;

/// How far ahead to look for something drawn before committing to a jump. The
/// bot's reflex asks for a wider window than this; `obstruction_ahead` takes the
/// reach as a parameter so that a caller can widen it without re-deriving what
/// "draws something" means.
pub const TRIGGER_COLUMNS: i16 = 3;

/// Jump when an enemy is this close ahead, in pixels.
pub const ENEMY_TRIGGER_PIXELS: i16 = 48;

/// Pixels of body past his position that the game's collision box reaches.
///
/// One measured number, kept in [`crate::resistance`], where two independent refusals
/// against pipe edges fixed it at fourteen; named again here because the affordance
/// questions ask about the body rather than about a refusal, and a skill that reads a
/// landing should not have to know which layer first measured the box.
///
/// It is the reason a landing is judged by where the box comes down rather than by the
/// byte the profile calls his position: the pit at world pixel 1379 ends at 1427 and
/// the shortest jump the commit sweep scored as a crossing left him at absolute X
/// 1414, and it stood. A jump that stops one pixel short of this falls in.
pub const BODY_REACH_PIXELS: u16 = crate::resistance::BODY_RIGHT_REACH_PIXELS;

/// The widest hole this layer will call a gap.
///
/// Eight cells. The pit at world pixel 1379 is six, and a hole wider than the longest
/// measured jump carries is not a gap a skill can act on — it is a different
/// situation, and naming it `Gap` would let a skill commit to a jump that cannot
/// finish it.
pub const MAX_GAP_PIXELS: u16 = 64;

/// A hole in the ground the model can see, named by the cells that bound it.
///
/// This is the affordance question Phase 0 allows — "which affordance holds right
/// now, naming the cell it is about" — asked of world pixels rather than of screen
/// columns, because a skill has to compare a distance against a jump it measured, and
/// a column number is only a distance eight frames out of eight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gap {
    /// The first world pixel ahead with nothing to stand on.
    pub near_x: u16,
    /// The first world pixel past the hole that does carry weight.
    ///
    /// Only a claim about the picture: read [`Gap::far_edge_seen`] before spending a
    /// jump on it. Past the camera's window the PPU holds no answer, and a `far_x`
    /// from there is where the reading stopped, not where the ground came back.
    pub far_x: u16,
    /// The feet row the reading was made on.
    pub row: i16,
    /// Whether the picture actually showed the ground return.
    pub far_edge_seen: bool,
}

impl Gap {
    /// Pixels of hole between the two cells.
    pub fn width_pixels(&self) -> u16 {
        self.far_x.saturating_sub(self.near_x)
    }

    /// Whether a body standing at `x` is over the hole rather than on either lip.
    ///
    /// Uses the box, not the point: he is over the hole until the box reaches the far
    /// lip, which is the same fourteen pixels that decide a landing — asked at
    /// [`body_last_pixel`], because the game's own edge is the last pixel he occupies.
    pub fn contains(&self, x: u16) -> bool {
        x >= self.near_x && body_last_pixel(x) < self.far_x
    }
}

/// The last world pixel a body at `x` occupies.
///
/// [`BODY_REACH_PIXELS`] is the *exclusive* right edge of the game's box, so the pixel
/// that can be inside a cell and counted is one short of it, and every question this
/// layer asks about reach — is he over the hole, does a landing come down on the far
/// lip, has he got past the pixel a skill was started about — is asked here.
///
/// The pit is the evidence and it is a two-pixel bracket: a landing at absolute X 1411
/// stands and one at 1409 falls into a hole whose ground returns at 1424, and the last
/// pixel answers both (1411 + 13 = 1424, 1409 + 13 = 1422). The pixel between them, a
/// landing at 1410, has never been pressed, so one pixel of ambiguity is still open
/// here. It is recorded rather than argued because it is the kind of number that gets
/// re-measured the first time a landing is refused for the wrong reason.
pub fn body_last_pixel(x: u16) -> u16 {
    x.saturating_add(BODY_REACH_PIXELS - 1)
}

/// Whether the cells under one world pixel would hold him up.
///
/// `None` is not the same answer as `Some(false)`: `None` means the pixel is outside
/// the picture the PPU is holding, which is where [`tile_at_world_pixel`] stops
/// rather than where the level stops. A skill that cannot see where a jump ends has
/// to be able to tell those apart, because "nothing to land on" and "not yet drawn"
/// want opposite decisions.
pub fn carries_weight_at(frame: &PpuBytes, origin_x: u16, world_x: u16, row: i16) -> Option<bool> {
    let mut on_screen = false;
    for drop in 0..=SUPPORT_ROWS {
        if let Some(tile) = tile_at_world_pixel(frame, origin_x, world_x, row + drop) {
            on_screen = true;
            if is_load_bearing(tile) {
                return Some(true);
            }
        }
    }
    on_screen.then_some(false)
}

/// The hole ahead of him, read by world pixel over [`SUPPORT_ROWS`] rows.
///
/// Starts one pixel ahead of `x` so the ground under him is never reported as a hole,
/// and stops at the picture's edge or at [`MAX_GAP_PIXELS`] of hole, whichever comes
/// first. `None` means nothing in `columns` of reach is missing, which is not a claim
/// that the ground is unbroken further out.
pub fn gap_ahead(frame: &PpuBytes, origin_x: u16, x: u16, row: i16, columns: i16) -> Option<Gap> {
    let reach = u16::try_from(i32::from(columns) * 8).ok()?;
    let near_x = (1..=reach).find(|&step| {
        carries_weight_at(frame, origin_x, x.saturating_add(step), row) == Some(false)
    })?;
    let near_x = x.saturating_add(near_x);
    let mut width = 0u16;
    while width <= MAX_GAP_PIXELS {
        width += 1;
        let at = near_x.saturating_add(width);
        match carries_weight_at(frame, origin_x, at, row) {
            // The picture ended before the ground did. The hole is real and its far
            // edge is not a fact yet.
            None => {
                return Some(Gap {
                    near_x,
                    far_x: at,
                    row,
                    far_edge_seen: false,
                });
            }
            Some(true) => {
                return Some(Gap {
                    near_x,
                    far_x: at,
                    row,
                    far_edge_seen: true,
                });
            }
            Some(false) => {}
        }
    }
    Some(Gap {
        near_x,
        far_x: near_x.saturating_add(width),
        row,
        far_edge_seen: false,
    })
}

/// Columns to the first cell ahead with nothing to land on, or `None` when every
/// column in reach has support.
///
/// The reflex's cheap form of [`gap_ahead`], on the screen-indexed grid the policy
/// reads: up to one column away from the cell the pixel actually falls in, on seven
/// frames out of eight. Enough to decide to jump; not enough to decide where to land.
pub fn gap_within_reach(tiles: &[u8; TILE_CELLS], column: i16, row: i16) -> Option<i16> {
    (1..=REACH_COLUMNS).find(|&step| {
        !(0..=SUPPORT_ROWS)
            .any(|drop| tile_at(tiles, column + step, row + drop).is_some_and(is_load_bearing))
    })
}

/// Cells beyond the body that a refusal is allowed to speak for.
///
/// Zero is the strictest reading of the ledger: it records the cell that refused
/// him, so only that cell is a report and every cell past it is an induction. One is
/// what acting on the report needs, because a body pressed flush against a wall has
/// no run-up left and a jump started flush clears a two-cell pipe only just. It is
/// still two cells, not the seven a reaction rule needed: the ledger names artwork,
/// so it can say what a cell is before he reaches it, which is the thing the load
/// bearing set could never do.
pub const WALL_LOOK_COLUMNS: i16 = 1;

/// Whether the model sees something to stand against in the cells his body would
/// sweep into.
///
/// Two sources, and the difference between them is the point. The load-bearing set
/// is what the game has held him up on, so on its own this reports false right up to
/// the frame a pipe body stops him. The ledger is what the game has refused to let
/// him through, the same fact arrived at from the other side, and because it is
/// keyed on the artwork it carries to every cell drawn the same way, which is what
/// makes looking more than one cell ahead worth doing at all.
pub fn wall_ahead(
    frame: &PpuBytes,
    origin_x: u16,
    state: &SmbState,
    resistance: &Resistance,
    columns: i16,
) -> bool {
    (0..=columns).any(|step| {
        (1..=BODY_ROWS).any(|rise| {
            cell_beyond(frame, origin_x, state, Side::Right, step, rise).is_some_and(|tile| {
                is_load_bearing(tile)
                    || resistance.refused(
                        pattern_hash(pattern_bytes(frame, tile)),
                        -rise,
                        Side::Right,
                    )
            })
        })
    })
}
