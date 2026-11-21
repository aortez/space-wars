//! World-model rollout check: can a planner branch this emulator forward from
//! any frame and come back to it exactly?
//!
//! Boots Super Mario Bros., dumps the level geometry straight out of the
//! nametables, then branches four macro-actions from one checkpoint and rewinds
//! between each. A planner only works if the rewind is bit-exact, so every
//! restore is checked against the root state hash.
//!
//! The reader it drives lives in `tests/support/smb_world_model.rs`, shared
//! with the optional `smb_world_model` test.

use std::{error::Error, fs, time::Instant};

#[path = "../tests/support/smb_world_model.rs"]
mod smb;

use engine_nes::ControllerButtons;
use smb::{LIVES, PLAYER_STATE, X_PAGE, X_SCREEN, absolute_x, boot_to_gameplay, read_state};

fn main() -> Result<(), Box<dyn Error>> {
    let rom = fs::read(
        std::env::args()
            .nth(1)
            .as_deref()
            .unwrap_or("data/roms/smb.nes"),
    )?;

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

    // Rewind to the root and confirm the machine is exactly where it started.
    machine.restore(&root)?;
    assert_eq!(machine.state_hash(), root_hash);
    println!("rewind to root reproduces the root hash exactly");
    Ok(())
}
