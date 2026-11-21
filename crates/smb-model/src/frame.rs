//! L2 frame: what is drawn, where it is, and an identity for it that survives a
//! bank switch or a re-release.
//!
//! Terrain comes from the PPU, not RAM. The snapshot is taken at a frame
//! boundary, the scroll latch `t` rather than the current address `v` is the
//! reliable origin, tiles are identified by CHR pattern hash rather than tile
//! number, and a cell named by a *world* pixel is resolved through the
//! cartridge's own mirroring. Each of those is a decode trap documented in
//! `docs/nes-world-model-reader.md`, and each one is here because the alternative
//! was measured to be wrong.
//!
//! The enemy slots are here rather than in the profile for the same reason the
//! nametable period is: an enemy's position is only meaningful relative to the
//! camera, so reading one needs L2's arithmetic and not just L1's addresses.

use crate::profile::{
    ENEMY_ACTIVE, ENEMY_TYPE, ENEMY_X_PAGE, ENEMY_X_SCREEN, ENEMY_Y_SCREEN, Enemy, SmbState,
    absolute_x,
};
use engine_nes::{CHR_MEMORY_BYTES, FRAME_PIXELS, FRAME_WIDTH, Mirroring, NesMachine};

/// `$00CE` to the top edge of the block the player stands on, in framebuffer
/// pixels. The drawn sprite sits 32 pixels below the stored Y; see `player_tile`.
pub const PLAYER_FEET_OFFSET_PIXELS: i16 = 32;

// Terrain geometry. The NES renders 256x240 of a 32x30-tile nametable pair,
// and SMB crops the top 8 pixels, so 28 tile rows are on screen.
pub const TILE_COLUMNS: usize = 32;

pub const TILE_ROWS: usize = 28;

pub const TILE_CELLS: usize = TILE_COLUMNS * TILE_ROWS;

pub const TOP_CROP_PIXELS: i16 = 8;

pub const NAMETABLE_PIXEL_PERIOD: u16 = 512;

pub const NAMETABLE_ROW_PERIOD: u16 = 480;

/// Everything DirtSim's `NesPpuSnapshot` holds, read out of the Rust core at
/// one instant. Borrowing the machine instead would let a later frame move the
/// state under a reader that compares pixels with tiles.
pub struct PpuBytes {
    pub frame_id: u64,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub control: u8,
    pub chr: [u8; CHR_MEMORY_BYTES],
    pub oam: [u8; 0x100],
    pub palette: [u8; 0x20],
    pub vram: [u8; 0x1000],
    pub mirroring: Mirroring,
}

/// Snapshot the PPU side of the world model.
///
/// The core finishes a frame inside vblank, before the cartridge has written
/// the next scroll, so a snapshot taken after `run_frame` pairs with the
/// framebuffer that frame left behind.
pub fn snapshot(machine: &NesMachine) -> PpuBytes {
    let registers = machine.ppu().registers();
    let mut chr = [0u8; CHR_MEMORY_BYTES];
    for (index, byte) in chr.iter_mut().enumerate() {
        // $0000-$1fff is the pattern table; NROM answers it from CHR ROM.
        *byte = machine
            .bus()
            .ppu_memory_peek(u16::try_from(index).expect("CHR fits in u16"));
    }
    PpuBytes {
        frame_id: machine.ppu().timing().frame_id,
        v: registers.vram_address,
        t: registers.temporary_address,
        fine_x: registers.fine_x,
        control: registers.control,
        chr,
        oam: *machine.ppu().oam(),
        palette: *machine.ppu().palette_ram(),
        vram: *machine.ppu().nametables(),
        mirroring: machine.bus().cartridge().mirroring(),
    }
}

/// FNV-1a over a tile's 16 pattern bytes.
///
/// Tiles are identified by pattern hash rather than tile number: a number is
/// bank- and game-specific, a pattern is not, so a vocabulary built on hashes
/// survives a CHR bank switch and a different ROM revision.
pub fn pattern_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Coarse X of the top-left visible pixel inside the 512-pixel nametable period.
///
/// The scroll latch `t`, not the current address `v`, is the reliable source at
/// a frame boundary: `v`'s Y field is incremented on every visible scanline and
/// only reloaded from `t` during vblank, so sampling `v` reports a Y nametable
/// one table away from the one just rendered. DirtSim's `NesTileFrame` reads
/// `v`; against rendered pixels that error is 57 mismatched tile shapes.
pub fn scroll_x(state: &PpuBytes) -> u16 {
    ((state.t >> 10) & 1) * 256 + (state.t & 0x1f) * 8 + u16::from(state.fine_x & 7)
}

/// Coarse Y of the top-left visible pixel inside the 480-pixel nametable period.
pub fn scroll_y(state: &PpuBytes) -> u16 {
    ((state.t >> 11) & 1) * 240 + ((state.t >> 5) & 0x1f) * 8 + ((state.t >> 12) & 7)
}

/// Tile number for one world pixel, following the cartridge's nametable
/// mirroring exactly as the PPU does when it renders.
pub fn tile_id_at(state: &PpuBytes, origin_x: u16, origin_y: u16, x: u16, y: u16) -> u8 {
    let world_x = (origin_x + x) % NAMETABLE_PIXEL_PERIOD;
    let world_y = (origin_y + y) % NAMETABLE_ROW_PERIOD;
    let (tile_x, tile_y) = (world_x / 8, world_y / 8);
    let logical = ((tile_y / 30) & 1) * 0x800
        + ((tile_x / 32) & 1) * 0x400
        + (tile_y % 30) * 32
        + tile_x % 32;
    state.vram[state.mirroring.map_nametable_address(0x2000 + logical)]
}

/// The 16 pattern bytes behind a tile number, honouring the pattern-table select bit.
pub fn pattern_bytes(state: &PpuBytes, tile: u8) -> &[u8] {
    let table = usize::from(state.control & 0x10 != 0) * 0x1000;
    let start = table + usize::from(tile) * 16;
    &state.chr[start..start + 16]
}

/// True when a tile draws nothing. Derived from pixels, not from a tile number,
/// because SMB's blank-sky filler is `$24` rather than `$00`: a reader that
/// only trusts `$00` sees a wall of solid tiles across the whole sky.
pub fn is_blank_tile(state: &PpuBytes, tile: u8) -> bool {
    pattern_bytes(state, tile).iter().all(|&byte| byte == 0)
}

/// Tile ids that have been seen carrying the player's weight, in level 1-1.
///
/// **This is not a solidity oracle and must not be used as one.** It answers one
/// question, "is this the surface he is standing on", and it is only trustworthy
/// there. See [`is_load_bearing`] for why the two directions of solidity cannot
/// share a set.
///
/// The nametable records what is drawn, not what is solid, and the two disagree
/// in the worst possible direction for a bot: the hill and bush decoration is
/// fully opaque, so `is_blank_tile` calls it solid. Running a fixed script and
/// recording the tile id under the player's feet on frames the game reported as
/// grounded, against the ids his body sat inside, split them cleanly. Over 2583
/// grounded frames the two sets did not overlap once: `$26` was a body tile on
/// 2413 of them and a feet tile on none, while the `$b4`/`$b5` ground, the
/// `$60`-`$63` pipe tops and the `$45`, `$57` and `$58` solid blocks (each drawn
/// as a lit top edge over a filled body) only ever held his feet.
///
/// Three limits, each of them a trap.
///
/// 1. It is 1-1's vocabulary, gathered by observation rather than from the ROM's
///    own collision tables, so underground and castle areas add ids this run
///    never reached.
/// 2. It is what was seen under his feet, not what is solid. `$b6` and `$b7` are
///    the same ground pattern one row lower, which he can never stand on, so
///    they are absent without being passable.
/// 3. Standing on something and being stopped by it are different observations,
///    and this method can only make the first. A pipe is `$60`-`$63` on its top
///    row, which is in the set, over `$64`-`$6a` for its body, which is not, so
///    every row that actually stops him reads as scenery. The reader-driven bot
///    in `smb-ai`'s `bot` walks into a pipe body at absolute X 899, is stopped by
///    the game, and stands there pushing right with `wall_ahead` false; the grid it
///    prints at that frame is the evidence. Learning the wall ids needs a different
///    experiment, walking into things, or the ROM's own collision table.
pub const LOAD_BEARING_TILES: [u8; 9] = [0x45, 0x57, 0x58, 0x60, 0x61, 0x62, 0x63, 0xb4, 0xb5];

/// Whether a tile has ever been seen holding the player up.
///
/// Neither half of "is this tile solid" is a property of a tile id alone. The
/// opposite of [`is_blank_tile`] is not this, because the decoration is opaque
/// and passable. And this is not "blocks him sideways", because a set learned by
/// watching what he stands on cannot contain a wall he has only ever walked
/// into: the pipe's top row is in here and its body is not. Solidity is a
/// relation over tile, direction, player state and time, and used blocks change
/// id when hit. Treat this as the narrow feet-row predicate it is.
pub fn is_load_bearing(tile: u8) -> bool {
    LOAD_BEARING_TILES.contains(&tile)
}

/// The 32x28 grid of tile numbers actually on screen, sampled at cell centres.
///
/// SMB scrolls at pixel granularity, so this grid is a sampling of a pixel-exact
/// field and drifts up to seven pixels while the camera moves. `tile_id_at` is
/// the exact channel.
pub fn tile_grid(state: &PpuBytes, origin_x: u16, origin_y: u16) -> [u8; TILE_CELLS] {
    let mut tiles = [0u8; TILE_CELLS];
    for (row, grid_row) in tiles.chunks_exact_mut(TILE_COLUMNS).enumerate() {
        for (column, cell) in grid_row.iter_mut().enumerate() {
            let x = u16::try_from(column * 8 + 4).expect("screen fits in u16");
            let y =
                u16::try_from(row * 8 + 4 + TOP_CROP_PIXELS as usize).expect("screen fits in u16");
            *cell = tile_id_at(state, origin_x, origin_y, x, y);
        }
    }
    tiles
}

/// One cell of the decoded grid, or `None` off its edges.
pub fn tile_at(tiles: &[u8; TILE_CELLS], column: i16, row: i16) -> Option<u8> {
    let column = usize::try_from(column).ok()?;
    let row = usize::try_from(row).ok()?;
    tiles.get(row * TILE_COLUMNS + column).copied()
}

/// Tile index of a screen pixel, rounding towards negative infinity.
pub fn floor_div_tile(pixel: i16) -> i16 {
    if pixel >= 0 {
        pixel / 8
    } else {
        -(((-pixel) + 7) / 8)
    }
}

/// The tile the player is standing on, in the decoded grid's coordinates.
///
/// The horizontal half is a wrapped delta: absolute X runs to 512 while the
/// camera wraps, so a plain subtraction puts Mario on the wrong page.
///
/// The vertical half needs an offset that is not obvious from either RAM or the
/// nametable. `$00CE` is not a screen coordinate in the sense the PPU means:
/// the sprite the player sees is drawn 32 pixels below the value, so the block
/// under his feet is the row whose top edge sits at `y_screen + 32`, which the
/// 8-pixel status-bar crop turns into `y_screen + 24`. Two measurements agree:
/// on flat ground the rendered surface is framebuffer row 208 while `$00CE`
/// reads 176, and sweeping every 8-pixel offset across 297 grounded frames puts
/// the only consistent surface at `+24`.
///
/// DirtSim's `makeNesSuperMarioBrosPlayerTileScreenY` subtracts the crop
/// instead, a four-row error. It never bites there because that value anchors a
/// player-relative window, where a constant bias is absorbed by training. It
/// bites badly the moment a bot asks "is there ground under me".
pub fn player_tile(state: &SmbState, origin_x: u16) -> (i16, i16) {
    (
        floor_div_tile(wrapped_delta(state.absolute_x, origin_x)),
        floor_div_tile(i16::from(state.screen_y) + PLAYER_FEET_OFFSET_PIXELS - TOP_CROP_PIXELS),
    )
}

/// The tile drawn at one world pixel and one grid row.
///
/// This is the exact horizontal channel, and the difference between it and
/// [`tile_at`] is not cosmetic. [`tile_grid`] samples the screen at cell centres, so
/// its column 14 is the tile under screen pixel 116, and screen pixel 116 is world
/// pixel `origin + 116` for whatever the fine scroll happens to be. The level's own
/// cells sit on world multiples of eight. The two agree only when the fine scroll is
/// zero, so a cell looked up through the grid is up to one column away from the cell
/// the pixel actually falls in, on seven frames out of eight.
///
/// That is the whole reason a pipe stopped the bot with `wall_ahead` false: the
/// refusal was attributed to the right artwork but the look-ahead was measured
/// against the wrong grid.
pub fn tile_at_world_pixel(frame: &PpuBytes, origin_x: u16, world_x: u16, row: i16) -> Option<u8> {
    let screen_x = wrapped_delta(world_x, origin_x);
    if screen_x < 0 {
        return None;
    }
    let y = i32::from(row) * 8 + i32::from(TOP_CROP_PIXELS) + 4;
    let y = u16::try_from(y).ok()?;
    Some(tile_id_at(
        frame,
        origin_x,
        0,
        u16::try_from(screen_x).ok()?,
        y,
    ))
}

/// The tile row the player's own sprite is standing on, measured from OAM.
///
/// [`player_tile`] needs `PLAYER_FEET_OFFSET_PIXELS`, and that number is the one
/// thing in this reader no RAM byte states outright. This derives the same row
/// without it, so a test can hold the two against each other instead of trusting
/// the constant.
///
/// SMB writes the player's sprite parts at OAM X = `$0086` and lays them out as a
/// column of 8-pixel parts starting at OAM Y = `$00CE`, unused parts left as the
/// blank `$fc` pattern. So the group is identified by the entry at `$00CE` and
/// measured from the entry at the other end of it: a sprite at OAM y covers
/// screen rows y+1 to y+height, the surface starts on the row below that, and the
/// status-bar crop shifts it again. On a grounded frame that lands on `$00CE + 32`
/// either way, which is the whole argument for the offset, arrived at from the
/// picture rather than from the number.
///
/// `None` when nothing is drawn at the player's X, or when only something else
/// is: the game hides him during pipe transitions and death, and an enemy can
/// share his column.
pub fn player_surface_row_from_sprites(state: &SmbState, frame: &PpuBytes) -> Option<i16> {
    let height: i16 = if frame.control & 0x20 != 0 { 16 } else { 8 };
    let group: Vec<i16> = frame
        .oam
        .chunks_exact(4)
        .filter(|part| i16::from(part[3]) == i16::from(state.screen_x) && part[1] != 0xff)
        .map(|part| i16::from(part[0]))
        .collect();
    if !group.contains(&i16::from(state.screen_y)) {
        return None;
    }
    let lowest = *group.iter().max()?;
    Some(floor_div_tile(lowest + height + 1 - TOP_CROP_PIXELS))
}

/// Cells a sprite is drawn over, so they can be set aside before comparing a
/// reconstructed background with the framebuffer.
///
/// Two cells of margin: a sprite straddles cell boundaries and is drawn on top
/// of whatever the background put there. Note that OAM read after a frame is the
/// list the *next* frame draws, so a sprite that moved or despawned leaves this
/// mask slightly wrong; that is the sprite channel's skew, not the background's.
pub fn sprite_covered_cells(state: &PpuBytes) -> [[bool; TILE_COLUMNS]; TILE_ROWS] {
    let sprite_rows: i32 = if state.control & 0x20 != 0 { 2 } else { 1 };
    let mut covered = [[false; TILE_COLUMNS]; TILE_ROWS];
    for sprite in state.oam.chunks_exact(4) {
        let (y, pattern, x) = (i32::from(sprite[1]), sprite[2], i32::from(sprite[3]));
        if pattern == 0xff && x == 0 {
            continue;
        }
        for offset_y in (-16..sprite_rows * 8 + 16).step_by(8) {
            for offset_x in (-16..24).step_by(8) {
                let screen_x = x + offset_x;
                let crop = i32::from(TOP_CROP_PIXELS);
                let screen_y = y + offset_y - 1 + crop;
                if screen_x < 0 || screen_y < crop {
                    continue;
                }
                let column = usize::try_from(screen_x / 8).unwrap_or(0) % TILE_COLUMNS;
                let row = usize::try_from((screen_y - crop) / 8).unwrap_or(0) % TILE_ROWS;
                covered[row][column] = true;
            }
        }
    }
    covered
}

/// Outcome of rebuilding the background from nametables and CHR and comparing
/// the transparency mask with the pixels the core actually rendered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reconstruction {
    pub cells: u32,
    pub pixels: u32,
    pub wrong_pixels: u32,
    pub affected_cells: u32,
    pub wrong_pixels_by_column: [u32; TILE_COLUMNS],
}

/// Reconstruct the background pixel by pixel and count the pixels whose
/// transparency disagrees with the framebuffer.
///
/// Palette is deliberately ignored: the attribute table recolours a pattern
/// without changing its shape, so only the mask is compared. `shift` skews the
/// assumed scroll X, which separates a decode error from a one-pixel sampling
/// skew. Returns `None` when video output is off and there is nothing to
/// compare against.
pub fn reconstruct_background(
    state: &PpuBytes,
    framebuffer: &[u8; FRAME_PIXELS],
    covered: &[[bool; TILE_COLUMNS]; TILE_ROWS],
    shift: i16,
) -> Reconstruction {
    let origin_x = i16::try_from(scroll_x(state)).expect("scroll fits in i16") + shift;
    let origin_x = u16::try_from((origin_x + 512) % 512).expect("scroll fits in u16");
    let origin_y = scroll_y(state);
    let transparent = state.palette[0];
    let mut result = Reconstruction::default();
    for (row, covered_row) in covered.iter().enumerate() {
        for (column, covered_cell) in covered_row.iter().enumerate() {
            if *covered_cell {
                continue;
            }
            result.cells += 1;
            let mut cell_bad = 0u32;
            for py in 0..8usize {
                for px in 0..8usize {
                    let (screen_x, screen_y) =
                        (column * 8 + px, row * 8 + TOP_CROP_PIXELS as usize + py);
                    let (dx, dy) = (
                        u16::try_from(screen_x).expect("screen fits in u16"),
                        u16::try_from(screen_y).expect("screen fits in u16"),
                    );
                    let tile = tile_id_at(state, origin_x, origin_y, dx, dy);
                    let pattern = pattern_bytes(state, tile);
                    let bit = 7 - ((origin_x + dx) % 8);
                    let pattern_row = usize::from((origin_y + dy) % 8);
                    let (lo, hi) = (
                        u16::from(pattern[pattern_row]),
                        u16::from(pattern[8 + pattern_row]),
                    );
                    let drawn = (lo >> bit) & 1 != 0 || (hi >> bit) & 1 != 0;
                    let rendered = framebuffer[screen_y * FRAME_WIDTH + screen_x] != transparent;
                    result.pixels += 1;
                    if drawn != rendered {
                        result.wrong_pixels += 1;
                        cell_bad += 1;
                    }
                }
            }
            if cell_bad > 0 {
                result.affected_cells += 1;
                result.wrong_pixels_by_column[column] += cell_bad;
            }
        }
    }
    result
}

/// Two-character vocabulary label, as DirtSim prints it.
pub fn label(token: u16) -> String {
    const LABELS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let first = LABELS[usize::from(token / 36) % 36] as char;
    let second = LABELS[usize::from(token % 36) % 36] as char;
    format!("{first}{second}")
}

// A reader-driven policy, kept next to the reader so the bot example and the
// optional test act on the same sense data and cannot drift apart. It is the
// smallest policy that uses nothing the reader has not decoded.

/// Signed distance inside the 512-pixel nametable period, as the PPU sees it.
pub fn wrapped_delta(value: u16, origin: u16) -> i16 {
    let period = i16::try_from(NAMETABLE_PIXEL_PERIOD).expect("nametable period fits in i16");
    let mut delta = i16::try_from(i32::from(value & 0x1ff) - i32::from(origin & 0x1ff))
        .expect("one nametable period fits in i16");
    if delta < -(period / 2) {
        delta += period;
    }
    if delta > period / 2 - 1 {
        delta -= period;
    }
    delta
}

// An inherent block for a profile type, kept here because reading an
// enemy needs this layer's camera arithmetic. Rust lets the impl live in
// any module of the crate, so the call site does not have to know which
// layer the method came from.
impl SmbState {
    /// Live enemy slots, nearest first by squared distance from the player.
    /// DirtSim reports only the closest two so its sense vector stays fixed
    /// width; a reader that wants more can take the whole slice.
    pub fn enemies(&self, machine: &NesMachine) -> Vec<Enemy> {
        let mut enemies = Vec::new();
        for slot in 0..ENEMY_ACTIVE.len() {
            if machine.bus().peek(ENEMY_ACTIVE[slot]) == 0 {
                continue;
            }
            let absolute_x = absolute_x(machine, ENEMY_X_PAGE[slot], ENEMY_X_SCREEN[slot]);
            enemies.push(Enemy {
                slot,
                kind: machine.bus().peek(ENEMY_TYPE[slot]),
                absolute_x,
                screen_y: machine.bus().peek(ENEMY_Y_SCREEN[slot]),
                delta_x: wrapped_delta(absolute_x, self.absolute_x),
            });
        }
        enemies.sort_by_key(|enemy| {
            let dx = i32::from(enemy.delta_x);
            let dy = i32::from(enemy.screen_y) - i32::from(self.screen_y);
            dx * dx + dy * dy
        });
        enemies
    }
}
