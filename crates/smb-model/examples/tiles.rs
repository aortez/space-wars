//! Tile-frame world-model reader: the terrain a bot has to plan over.
//!
//! The RAM map in `probe.rs` gives object positions; terrain is
//! not in RAM, it is in the PPU. This rebuilds the 32x28 tile grid that is
//! actually on screen by decoding the scroll latch, walking the nametables
//! through the cartridge's own mirroring, and identifying each tile by a hash
//! of its CHR pattern rather than by its tile number. Tile numbers are bank-
//! and game-specific; pattern hashes are not.
//!
//! It then proves the decode by reconstructing the background pixel by pixel
//! and comparing it with the framebuffer the core really rendered.
//!
//! The decode lives in `smb-model`'s `frame` layer, shared with the optional
//! `reader` test and with every bot. A cartridge path is optional: the default is
//! `cartridge::path`, which is `SPACEWARS_SMB_ROM` or `data/roms/smb.nes`.

use std::{collections::HashMap, env, error::Error, path::PathBuf};

use smb_model::cartridge;
use smb_model::frame::{
    is_blank_tile, is_load_bearing, label, player_surface_row_from_sprites, player_tile,
    reconstruct_background, scroll_x, scroll_y, snapshot, sprite_covered_cells, tile_at, tile_grid,
};
use smb_model::profile::read_state;

use engine_nes::ControllerButtons;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let chosen = args.next().map(PathBuf::from);
    let frames: u64 = args.next().unwrap_or_else(|| "420".to_owned()).parse()?;
    // How long to keep running after gameplay before sampling. Varying this
    // samples different camera positions and sprite layouts.
    let settle_frames: u64 = args.next().unwrap_or_else(|| "30".to_owned()).parse()?;

    let Some(rom) = cartridge::read_from(chosen)? else {
        return Ok(());
    };
    let (mut machine, gameplay_frame) = smb_model::profile::boot_to_gameplay(&rom)?;
    println!("frame {gameplay_frame}: gameplay");

    // Sample once Mario is standing on something, so the terrain decoded under
    // his feet can be compared with the terrain he is really on.
    let mut sample_frame = None;
    for elapsed in 1..=frames {
        machine.run_frame([
            ControllerButtons::RIGHT | ControllerButtons::B,
            ControllerButtons::NONE,
        ])?;
        if elapsed >= settle_frames && read_state(&machine).is_grounded() {
            sample_frame = Some(gameplay_frame + elapsed);
            break;
        }
    }
    let sample_frame = sample_frame.ok_or("never reached a grounded frame")?;
    println!("frame {sample_frame}: sampling a grounded frame");

    let state = read_state(&machine);
    let frame = snapshot(&machine);
    let (origin_x, origin_y) = (scroll_x(&frame), scroll_y(&frame));
    println!(
        "frame {} v=${:04x} t=${:04x} ctrl=${:02x} mirror={:?} -> scroll=({origin_x},{origin_y})",
        frame.frame_id, frame.v, frame.t, frame.control, frame.mirroring,
    );

    // Identify tiles by pattern hash so the map survives a different ROM
    // revision or a CHR bank switch.
    let tiles = tile_grid(&frame, origin_x, origin_y);
    let mut vocabulary = HashMap::<u64, u16>::new();
    for &tile in &tiles {
        let next = u16::try_from(vocabulary.len())? + 1;
        vocabulary
            .entry(smb_model::frame::pattern_hash(
                smb_model::frame::pattern_bytes(&frame, tile),
            ))
            .or_insert(next);
    }
    let token = |tile: u8| -> Result<u16, Box<dyn Error>> {
        let hash = smb_model::frame::pattern_hash(smb_model::frame::pattern_bytes(&frame, tile));
        Ok(*vocabulary
            .get(&hash)
            .ok_or("vocabulary built from these tiles above")?)
    };

    let mut line = String::new();
    for (row, grid_row) in tiles
        .chunks_exact(smb_model::frame::TILE_COLUMNS)
        .enumerate()
    {
        for &tile in grid_row {
            if is_blank_tile(&frame, tile) {
                line.push_str(" ..");
            } else {
                line.push_str(&format!(" {}", label(token(tile)?)));
            }
        }
        println!("  r{row:>2}{line}");
        line.clear();
    }

    println!("\n  legend (token = tile number, '*' = blank pattern):");
    let mut seen = HashMap::<u8, ()>::new();
    for &tile in &tiles {
        if seen.insert(tile, ()).is_some() {
            continue;
        }
        let blank = if is_blank_tile(&frame, tile) {
            "*"
        } else {
            " "
        };
        println!("    {}{blank} = ${tile:02x}", label(token(tile)?));
    }

    // Where is Mario inside that grid? His absolute X lives in two bytes and
    // the camera wraps, so the delta has to be taken modulo the nametable.
    let (column, row) = player_tile(&state, origin_x);
    println!(
        "\n  player absolute_x={} y_screen={} -> tile=({column},{row})",
        state.absolute_x, state.screen_y,
    );
    for (name, offset) in [("head", -2i16), ("body", -1), ("feet", 0), ("below", 1)] {
        let Some(tile) = tile_at(&tiles, column, row + offset) else {
            println!("    {name:>6}: off the decoded grid");
            continue;
        };
        // Opaque is not solid. SMB paints its hills and bushes with opaque
        // tiles and walks Mario straight through them, so the two have to be
        // reported apart or a bot parks itself inside a hill.
        let description = if is_blank_tile(&frame, tile) {
            "air"
        } else if is_load_bearing(tile) {
            "terrain, holds him up"
        } else {
            "opaque, never seen holding him up"
        };
        println!("    {name:>6}: ${tile:02x} ({description})");
    }
    // The feet row is the one number in the reader no RAM byte states outright,
    // so show it arriving a second way, from the player's own sprite column.
    if let Some(from_sprites) = player_surface_row_from_sprites(&state, &frame) {
        println!("    sprite: his own OAM column ends on row {from_sprites}");
    }

    check_against_framebuffer(&frame, &machine)?;

    // The blank-sky filler in SMB is tile $24, not $00. A reader that treats
    // only $00 as empty sees a wall of solid tiles across the whole sky.
    let filler = tile_at(&tiles, 0, 12);
    println!(
        "\n  mid-air tile at column 0 row 12 is ${:02x} and {} -- SMB uses $24, not $00, as sky filler",
        filler.unwrap_or(0),
        if filler.is_some_and(|tile| is_blank_tile(&frame, tile)) {
            "blank"
        } else {
            "not blank"
        },
    );
    println!("  gameplay reached at frame {gameplay_frame}");
    Ok(())
}

/// Reconstruct the background from nametables, mirroring and CHR, then compare
/// the transparency mask with what the PPU drew. Palette is ignored on purpose:
/// the attribute table recolours a pattern without changing its shape.
fn check_against_framebuffer(
    frame: &smb_model::frame::PpuBytes,
    machine: &engine_nes::NesMachine,
) -> Result<(), Box<dyn Error>> {
    let Some(framebuffer) = machine.ppu().framebuffer() else {
        println!("\n  framebuffer check skipped: video output is disabled");
        return Ok(());
    };
    let covered = sprite_covered_cells(frame);
    let exact = reconstruct_background(frame, framebuffer, &covered, 0);
    println!(
        "\n  background reconstruction at scroll ({},{}): \
         {} sprite-free cells, {} pixels, {} wrong ({} cells affected)",
        scroll_x(frame),
        scroll_y(frame),
        exact.cells,
        exact.pixels,
        exact.wrong_pixels,
        exact.affected_cells,
    );
    if exact.wrong_pixels > 0 {
        // A camera that is mid-motion is the usual cause of a residual, so show
        // whether a one-pixel scroll skew accounts for it.
        let mut probe = String::new();
        for shift in -2i16..=2 {
            let skewed = reconstruct_background(frame, framebuffer, &covered, shift);
            probe.push_str(&format!(" {shift:+}->{wrong}", wrong = skewed.wrong_pixels));
        }
        println!("  wrong pixels for a scroll X skew of{probe}");
        let touched: Vec<String> = exact
            .wrong_pixels_by_column
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(column, count)| format!("{column}:{count}"))
            .collect();
        println!("  columns with a mismatch: {}", touched.join(" "));
    }
    Ok(())
}
