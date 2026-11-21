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

use crate::frame::{PpuBytes, TILE_CELLS, is_load_bearing, pattern_bytes, pattern_hash, tile_at};
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

/// Columns to the first cell ahead with nothing to land on, or `None` when every
/// column in reach has support.
pub fn gap_ahead(tiles: &[u8; TILE_CELLS], column: i16, row: i16) -> Option<i16> {
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
