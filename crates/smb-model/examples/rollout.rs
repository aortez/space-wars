//! World-model rollout check: can a planner branch this emulator forward from
//! any frame and come back to it exactly, and how far ahead of the camera can it
//! have already read the level?
//!
//! Boots Super Mario Bros., dumps the level geometry straight out of the
//! nametables, then branches four macro-actions from one checkpoint and rewinds
//! between each. A planner only works if the rewind is bit-exact, so every
//! restore is checked against the root state hash.
//!
//! The last section answers the second question by watching every nametable slot
//! the camera passes over: how often a slot's tile id changes, and how often it
//! changes while the player can still see it. That is what bounds lookahead, and
//! it is why a level map has to be keyed by absolute X rather than by slot.
//!
//! The reader it drives is `smb-model` itself, the same one the optional
//! `reader` test and the bots use. A cartridge path is optional: the default is
//! `cartridge::path`, which is `SPACEWARS_SMB_ROM` or `data/roms/smb.nes`.

use std::{error::Error, path::PathBuf, time::Instant};

use smb_model::cartridge;
use smb_model::frame::{TILE_COLUMNS, TILE_ROWS, scroll_x, snapshot, tile_id_at};
use smb_model::profile::{
    LIVES, PLAYER_STATE, STATE_NORMAL, X_PAGE, X_SCREEN, absolute_x, boot_to_gameplay, read_state,
};

use engine_nes::ControllerButtons;

/// Nametable columns in the 512-pixel horizontal period. SMB scrolls across a
/// vertically mirrored pair, so the period is two tables of 32 columns.
const SLOT_COLUMNS: usize = 64;
const SLOT_ROWS: usize = TILE_ROWS;
const SLOT_COUNT: usize = SLOT_COLUMNS * SLOT_ROWS;

fn main() -> Result<(), Box<dyn Error>> {
    let chosen = std::env::args().nth(1).map(PathBuf::from);
    let Some(rom) = cartridge::read_from(chosen)? else {
        return Ok(());
    };

    // ---- 1. Level geometry straight out of the nametables -------------------
    let (mut machine, gameplay_frame) = boot_to_gameplay(&rom)?;
    println!("gameplay reached at frame {gameplay_frame}");
    for _ in 0..240 {
        machine.run_frame([
            ControllerButtons::RIGHT | ControllerButtons::B,
            ControllerButtons::NONE,
        ])?;
    }
    let nt = machine.ppu().nametables();
    println!(
        "after 240 right-walk frames: absolute_x={}",
        absolute_x(&machine, X_PAGE, X_SCREEN),
    );
    println!(
        "PPU t=0x{:04X} fine_x={} ctrl=0x{:02X} mirror={:?}",
        machine.ppu().registers().temporary_address,
        machine.ppu().registers().fine_x,
        machine.ppu().registers().control,
        machine.bus().cartridge().mirroring(),
    );

    // SMB uses vertical mirroring, so a horizontally scrolling screen reads one
    // of the two pages of the active vertical pair.
    for page in [0usize, 1] {
        let base = page * 0x400;
        println!("\n  nametable page {page} (tile ids, '.' = empty):");
        for (row, grid_row) in nt[base..base + 28 * 32].chunks_exact(32).enumerate() {
            let line: String = grid_row
                .iter()
                .map(|&tile| {
                    if tile == 0 || tile == 0x24 {
                        "  .".to_owned()
                    } else {
                        format!("{tile:02x}")
                    }
                })
                .collect();
            println!("    r{row:>2} {line}");
        }
    }

    // ---- 2. Rollout determinism: branch, score, rewind, branch again --------
    let root = machine.checkpoint();
    let root_hash = machine.state_hash();
    println!(
        "\nroot state hash {:016x} at frame {}",
        root_hash.value,
        machine.ppu().frame_id(),
    );

    let actions = [
        ("neutral  ", ControllerButtons::NONE),
        ("right+run", ControllerButtons::RIGHT | ControllerButtons::B),
        (
            "right+jump",
            ControllerButtons::RIGHT | ControllerButtons::A,
        ),
        ("jump only", ControllerButtons::A),
    ];
    let mut best = (0u16, String::new());
    let started = Instant::now();
    let mut rollouts = 0u32;
    for (label, action) in actions {
        machine.restore(&root)?;
        assert_eq!(machine.state_hash(), root_hash, "restore must be exact");
        for _ in 0..120 {
            machine.run_frame([action, ControllerButtons::NONE])?;
        }
        rollouts += 1;
        let state = read_state(&machine);
        println!(
            "  {label}  120 frames -> absolute_x={:>5}  lives={}  state={:>3}  hash {:016x}",
            state.absolute_x,
            machine.bus().peek(LIVES),
            machine.bus().peek(PLAYER_STATE),
            machine.state_hash().value,
        );
        if state.absolute_x > best.0 {
            best = (state.absolute_x, label.to_owned());
        }
    }
    let elapsed = started.elapsed();
    println!(
        "\n{} rollouts x 120 frames = {} frames in {:.1?} ({:.0} frames/s with checkpoint restore)",
        rollouts,
        rollouts * 120,
        elapsed,
        f64::from(rollouts * 120) / elapsed.as_secs_f64(),
    );
    println!("greedy best: {} reached x={}", best.1, best.0);

    // ---- 3. How far ahead of the camera has the reader already seen? --------
    machine.restore(&root)?;
    let mut ids: Vec<Vec<u8>> = (0..SLOT_COUNT).map(|_| Vec::new()).collect();
    let mut previous = vec![0u8; SLOT_COUNT];
    let mut visited = vec![false; SLOT_COUNT];
    let mut rewrites_by_column: std::collections::BTreeMap<usize, u32> =
        std::collections::BTreeMap::new();
    let mut sampled_frames = 0u64;
    let mut furthest = 0u16;
    let lives_at_start = read_state(&machine).lives;
    for elapsed in 1..=1400u64 {
        // Long hops on a fixed cadence: a policy that stops at the first pipe
        // would only ever measure the first screen of level.
        let mut action = ControllerButtons::RIGHT | ControllerButtons::B;
        if elapsed % 60 < 20 {
            action |= ControllerButtons::A;
        }
        let state = read_state(&machine);
        if state.lives < lives_at_start {
            println!(
                "  stopped measuring at frame {elapsed}: died at absolute X {}",
                state.absolute_x
            );
            break;
        }
        if !state.is_playing() || state.player_state != STATE_NORMAL {
            // A transition rebuilds the whole nametable, which is not the game
            // editing terrain the player can see.
            machine.run_frame([action, ControllerButtons::NONE])?;
            continue;
        }
        furthest = furthest.max(state.absolute_x);
        let frame = snapshot(&machine);
        let origin_x = scroll_x(&frame);
        for row in 0..SLOT_ROWS {
            for column in 0..TILE_COLUMNS {
                let x = u16::try_from(column * 8 + 4).expect("screen fits in u16");
                let y = u16::try_from(row * 8 + 12).expect("screen fits in u16");
                let tile = tile_id_at(&frame, origin_x, 0, x, y);
                let slot = usize::from((origin_x + x) % 512 / 8) + row * SLOT_COLUMNS;
                if visited[slot] && previous[slot] != tile {
                    *rewrites_by_column.entry(column).or_insert(0) += 1;
                }
                visited[slot] = true;
                previous[slot] = tile;
                if !ids[slot].contains(&tile) {
                    ids[slot].push(tile);
                }
            }
        }
        machine.run_frame([action, ControllerButtons::NONE])?;
        sampled_frames = elapsed;
    }
    let mut histogram: std::collections::BTreeMap<usize, u32> = std::collections::BTreeMap::new();
    for (slot, seen) in ids.iter().enumerate() {
        if visited[slot] {
            *histogram.entry(seen.len()).or_insert(0) += 1;
        }
    }
    let tracked: u32 = histogram.values().sum();
    let stable = histogram.get(&1).copied().unwrap_or_default();
    let rewrites: u32 = rewrites_by_column.values().sum();
    let mut hottest: Vec<(u32, usize)> = rewrites_by_column
        .iter()
        .map(|(column, count)| (*count, *column))
        .collect();
    hottest.sort_unstable_by(|a, b| b.cmp(a));
    let columns: Vec<String> = hottest
        .iter()
        .take(6)
        .map(|(count, column)| format!("col {column}: {count}"))
        .collect();
    println!("\nlookahead, watching every nametable slot the camera passes over:");
    println!(
        "  {sampled_frames} sampled playing frames to absolute X {furthest}: {stable} of \
         {tracked} slots ({:.0}%) kept one tile id the whole time",
        f64::from(stable) / f64::from(tracked.max(1)) * 100.0,
    );
    println!("  distinct ids per slot: {histogram:?}");
    println!(
        "  {rewrites} changes were seen in a slot the player could still see, at {}; \
         column {} is the edge the camera is exposing, so it is new terrain, not a rewrite",
        columns.join(", "),
        TILE_COLUMNS - 1,
    );
    println!(
        "  a slot is not a stable address for a level cell: the period is 512 pixels and the run \
         reached {furthest}, so {} slots held more than one id",
        tracked - stable
    );

    // Rewind to the root and confirm the machine is exactly where it started.
    machine.restore(&root)?;
    assert_eq!(machine.state_hash(), root_hash);
    println!("rewind to root reproduces the root hash exactly");
    Ok(())
}
