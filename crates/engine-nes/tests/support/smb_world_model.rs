//! Shared Super Mario Bros. world-model reader used by the `world_model_*`
//! examples and the optional `smb_world_model` test.
//!
//! This is a port of the address map and PPU decode that DirtSim runs against
//! its vendored `smolnes` core (`apps/src/core/scenarios/nes/`), retargeted at
//! the `engine-nes` read surface. It is deliberately kept out of `src/`: the
//! core stays cartridge-agnostic and holds no game knowledge, so the SMB map
//! lives here, on the host side of the boundary a real reader would occupy.
//!
//! Valid for `Super Mario Bros. (JU) (PRG0) [!]`, the image in `data/roms`.

// Included by several examples and the optional test, each of which uses
// only part of the map, so an item unused in one build is not dead.
#![allow(dead_code)]

use engine_nes::{
    CHR_MEMORY_BYTES, ControllerButtons, FRAME_PIXELS, FRAME_WIDTH, MachineConfig, Mirroring,
    NesMachine,
};

// Player and progression state. DirtSim NesSuperMarioBrosRamExtractor.cpp,
// from the Data Crystal Super Mario Bros. RAM map.
pub const GAME_MODE: u16 = 0x0770;
pub const WORLD: u16 = 0x075F;
pub const LEVEL: u16 = 0x0760;
pub const LIVES: u16 = 0x075A;
pub const POWERUP: u16 = 0x0756;
pub const PLAYER_STATE: u16 = 0x000E;
pub const FLOAT_STATE: u16 = 0x001D;
pub const H_SPEED: u16 = 0x0057;
pub const V_SPEED: u16 = 0x009F;
pub const FACING: u16 = 0x0033;
pub const X_PAGE: u16 = 0x006D;
pub const X_SCREEN: u16 = 0x0086;
pub const Y_SCREEN: u16 = 0x00CE;

// The five enemy slots the game keeps live at once.
pub const ENEMY_ACTIVE: [u16; 5] = [0x000F, 0x0010, 0x0011, 0x0012, 0x0013];
pub const ENEMY_TYPE: [u16; 5] = [0x0016, 0x0017, 0x0018, 0x0019, 0x001A];
pub const ENEMY_X_PAGE: [u16; 5] = [0x006E, 0x006F, 0x0070, 0x0071, 0x0072];
pub const ENEMY_X_SCREEN: [u16; 5] = [0x0087, 0x0088, 0x0089, 0x008A, 0x008B];
pub const ENEMY_Y_SCREEN: [u16; 5] = [0x00CF, 0x00D0, 0x00D1, 0x00D2, 0x00D3];

// Player modes at `PLAYER_STATE`.
pub const STATE_LEFTMOST: u8 = 0x00;
pub const STATE_PLAYER_DIES: u8 = 0x06;
pub const STATE_NORMAL: u8 = 0x08;
pub const STATE_GROWING: u8 = 0x09;
pub const STATE_SHRINKING: u8 = 0x0A;
pub const STATE_DYING: u8 = 0x0B;

// Game modes at `GAME_MODE`.
pub const MODE_ATTRACT: u8 = 0x00;
pub const MODE_PLAYING: u8 = 0x01;
pub const MODE_END_WORLD: u8 = 0x02;
pub const MODE_END_GAME: u8 = 0x03;

// Float state at `FLOAT_STATE`.
pub const FLOAT_GROUNDED: u8 = 0x00;
pub const FLOAT_JUMPING: u8 = 0x01;
pub const FLOAT_WALKED_OFF: u8 = 0x02;
pub const FLOAT_FLAGPOLE: u8 = 0x03;

// DirtSim's setup policy: one Start press, then watch for normal play.
pub const SETUP_START_FRAME: u64 = 120;
pub const SETUP_SCRIPT_END_FRAME: u64 = 300;
pub const SETUP_FAILURE_FRAME: u64 = 500;

/// `$00CE` to the top edge of the block the player stands on, in framebuffer
/// pixels. The drawn sprite sits 32 pixels below the stored Y; see `player_tile`.
pub const PLAYER_FEET_OFFSET_PIXELS: i16 = 32;

// The evaluator's terminal test: below the visible screen means lost.
pub const BELOW_SCREEN_Y: u8 = 224;

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

/// A decoded enemy slot. `delta_x` is signed distance from the player inside
/// the 512-pixel nametable period, which is what the PPU and the game both see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enemy {
    pub slot: usize,
    pub kind: u8,
    pub absolute_x: u16,
    pub screen_y: u8,
    pub delta_x: i16,
}

/// Semantic game state, decoded from CPU RAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmbState {
    pub game_mode: u8,
    pub world: u8,
    pub level: u8,
    pub lives: u8,
    pub powerup: u8,
    pub player_state: u8,
    pub float_state: u8,
    pub h_speed: i8,
    pub v_speed: i8,
    pub facing: u8,
    pub absolute_x: u16,
    pub screen_x: u8,
    pub screen_y: u8,
}

impl SmbState {
    pub fn is_playing(&self) -> bool {
        self.game_mode == MODE_PLAYING
    }

    pub fn is_grounded(&self) -> bool {
        self.float_state == FLOAT_GROUNDED && self.player_state == STATE_NORMAL
    }

    pub fn has_fallen_off_screen(&self) -> bool {
        self.screen_y > BELOW_SCREEN_Y
    }

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

/// Absolute horizontal position: SMB splits it across a page byte and an
/// on-screen byte because a level is wider than one nametable.
pub fn absolute_x(machine: &NesMachine, page: u16, screen: u16) -> u16 {
    (u16::from(machine.bus().peek(page)) << 8) | u16::from(machine.bus().peek(screen))
}

/// Decode the whole map in one pass. Reads are side-effect free, so this can
/// run every frame without disturbing the game.
pub fn read_state(machine: &NesMachine) -> SmbState {
    let peek = |address| machine.bus().peek(address);
    SmbState {
        game_mode: peek(GAME_MODE),
        world: peek(WORLD),
        level: peek(LEVEL),
        lives: peek(LIVES),
        powerup: peek(POWERUP),
        player_state: peek(PLAYER_STATE),
        float_state: peek(FLOAT_STATE),
        h_speed: peek(H_SPEED) as i8,
        v_speed: peek(V_SPEED) as i8,
        facing: peek(FACING),
        absolute_x: absolute_x(machine, X_PAGE, X_SCREEN),
        screen_x: peek(X_SCREEN),
        screen_y: peek(Y_SCREEN),
    }
}

/// The DirtSim setup script: nothing until frame 120, one Start press there.
pub fn setup_buttons(frame: u64) -> ControllerButtons {
    if frame == SETUP_START_FRAME {
        ControllerButtons::START
    } else {
        ControllerButtons::NONE
    }
}

/// Boot a cartridge and run the setup script until normal play begins.
///
/// Returns the frame gameplay appeared on. Fails if the script ends without it,
/// which is how a wrong ROM revision or a broken core shows up.
pub fn boot_to_gameplay(rom: &[u8]) -> Result<(NesMachine, u64), Box<dyn std::error::Error>> {
    let mut machine = NesMachine::from_ines(rom, MachineConfig::default())?;
    for frame in 1..=SETUP_FAILURE_FRAME {
        machine.run_frame([setup_buttons(frame), ControllerButtons::NONE])?;
        if machine.bus().peek(GAME_MODE) == MODE_PLAYING {
            return Ok((machine, frame));
        }
    }
    Err("setup script never reached normal play".into())
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

/// Tile ids that carry the player's weight, as observed in level 1-1.
///
/// The nametable records what is drawn, not what is solid, and the two disagree
/// in the worst possible direction for a bot: the hill and bush decoration is
/// fully opaque, so `is_blank_tile` calls it solid. Running a fixed script and
/// recording the tile id under the player's feet on frames the game reported as
/// grounded, against the ids his body sat inside, split them cleanly. Over 2583
/// grounded frames the two sets did not overlap once: `$26` was a body tile on
/// 2413 of them and a feet tile on none, while the `$b4`/`$b5` ground, the
/// `$60`-`$63` pipe tops and bodies and the `$45`, `$57` and `$58` solid blocks
/// (each drawn as a lit top edge over a filled body) only ever held his feet.
///
/// Two honest limits. It is 1-1's vocabulary, gathered by observation rather
/// than from the ROM's own collision tables, so underground and castle areas add
/// ids this run never reached. And it is what was seen under his feet, not what
/// is solid: `$b6` and `$b7` are the same ground pattern one row lower, which he
/// can never stand on, so they are absent without being passable.
pub const LOAD_BEARING_TILES: [u8; 9] = [0x45, 0x57, 0x58, 0x60, 0x61, 0x62, 0x63, 0xb4, 0xb5];

/// Whether a tile has ever been seen holding the player up. The opposite of
/// `is_blank_tile` is *not* this: see [`LOAD_BEARING_TILES`].
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
