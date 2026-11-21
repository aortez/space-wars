//! World-model reader probe: decode Super Mario Bros. game state out of a
//! running cartridge and print it as the game plays.
//!
//! Boots the ROM, runs DirtSim's scripted setup (one Start press on frame 120),
//! then walks Mario right and prints the decoded RAM map every second. This is
//! the read side of a bot: everything a policy would sense.
//!
//! The address map and decoders live in `tests/support/smb_world_model.rs`,
//! shared with the optional `smb_world_model` test.

use std::{env, error::Error, fs, path::PathBuf};

#[path = "../tests/support/smb_world_model.rs"]
mod smb;

use engine_nes::ControllerButtons;
use smb::{FLOAT_GROUNDED, FLOAT_JUMPING, boot_to_gameplay, read_state};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let rom_path = PathBuf::from(args.next().ok_or("usage: world_model_probe ROM [FRAMES]")?);
    let frames: u64 = args.next().unwrap_or_else(|| "700".to_owned()).parse()?;

    let rom = fs::read(&rom_path)?;
    let (mut machine, gameplay_frame) = boot_to_gameplay(&rom)?;
    println!("frame {gameplay_frame}: reached gameplay (game mode $0770 = 0x01)");

    let mut max_x = 0u16;
    for elapsed in 1..=frames {
        // Run right and hop on a fixed cadence so the float state gets used.
        let mut buttons = ControllerButtons::RIGHT;
        if elapsed % 90 < 12 {
            buttons |= ControllerButtons::A;
        }
        machine.run_frame([buttons, ControllerButtons::NONE])?;

        if elapsed % 60 != 0 {
            continue;
        }
        let state = read_state(&machine);
        max_x = max_x.max(state.absolute_x);
        let grounded = match state.float_state {
            FLOAT_GROUNDED => "ground",
            FLOAT_JUMPING => "air",
            _ => "other",
        };
        let enemies: Vec<String> = state
            .enemies(&machine)
            .iter()
            .map(|enemy| format!("{}@{:+}/{}", enemy.kind, enemy.delta_x, enemy.screen_y))
            .collect();
        println!(
            "  frame {:>5} world {}-{} x={:>5} screen=({:>3},{:>3}) \
             state={:>3} {grounded:>6} power={} lives={} facing={} \
             hspeed={:>+4} vspeed={:>+4} enemies=[{}]",
            gameplay_frame + elapsed,
            state.world,
            state.level,
            state.absolute_x,
            state.screen_x,
            state.screen_y,
            state.player_state,
            state.powerup,
            state.lives,
            state.facing,
            state.h_speed,
            state.v_speed,
            enemies.join(", "),
        );
    }

    println!("\nmax absolute X seen: {max_x}");
    Ok(())
}
