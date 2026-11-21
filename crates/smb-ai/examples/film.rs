//! Film a gym under its own driver, so a verdict can be watched.
//!
//! `film [GYM] [DIR] [FRAMES]` replays a committed gym's tape, hands the machine to the
//! player that gym names in [`Gym::driver`], and writes a PNG per emulator frame to `DIR`
//! in the naming `tools/nes-reference/film-smb-run.py` reads. The frame a jump starts on
//! gets an extra `event-jump-*` still, which is what a contact sheet is made of, and the
//! frame the gym's own objective first holds gets an `event-clear-*` still, because a
//! verdict's last frame is the one a film of a verdict should show. `bot --shots` already
//! marks the level's ending; this is the same still for a run that starts mid-level.
//!
//! The tape, the ledger seed and the player all come off the `Gym` itself, which is the
//! whole reason this exists rather than a screenshot: the film is cut by the same code
//! that measured the verdict, so it cannot show a run the gym did not score. `data/shots
//! /pit-cross` is that film — the `pit-1379` tape under the script — and these two
//! commands re-cut it from a commit:
//!
//! ```sh
//! cargo run --release --locked -p smb-ai --example film -- pit-1379 data/shots/pit-cross 200
//! python3 tools/nes-reference/film-smb-run.py data/shots/pit-cross
//! ```
//!
//! Frames are written at the tape's own frame ids, so the stills line up with the frames
//! the journal and the tests quote. There is no `--overlay` here: the model's dots are the
//! `bot` example's, and this run is too short to want a second copy of every frame.

use std::{
    env,
    error::Error,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
};

use engine_nes::{
    ControllerButtons, FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH, NesMachine, write_rgb888,
};
use smb_ai::{gym::Start, gyms};
use smb_model::{cartridge, profile::read_state};

/// The current picture as RGB888, or `None` while the PPU has nothing to show.
fn capture(machine: &NesMachine) -> Option<Vec<u8>> {
    let indices = machine.ppu().framebuffer()?;
    let mut rgb = vec![0u8; FRAME_PIXELS * 3];
    write_rgb888(indices, &mut rgb);
    Some(rgb)
}

fn write_png(path: &Path, rgb: &[u8]) -> Result<(), Box<dyn Error>> {
    let output = BufWriter::new(File::create(path)?);
    let mut encoder = png::Encoder::new(output, FRAME_WIDTH as u32, FRAME_HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(rgb)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let name = args.first().map_or("pit-1379", String::as_str);
    let Some(gym) = gyms::by_name(name) else {
        println!("no gym named {name}; the committed ones are:");
        for gym in gyms::ALL {
            println!("  {}", gym.name);
        }
        return Ok(());
    };
    let dir = PathBuf::from(
        args.get(1)
            .cloned()
            .unwrap_or_else(|| format!("data/shots/{}", gym.name)),
    );
    let frames: u64 = args.get(2).map_or(200, |arg| arg.parse().unwrap_or(200));
    let Some(rom) = cartridge::read()? else {
        println!("no cartridge: {}", cartridge::path().display());
        return Ok(());
    };

    let Start {
        mut machine,
        bot,
        frame,
    } = gym.tape.replay(&rom, gym.seed)?;
    fs::create_dir_all(&dir)?;
    let mut player = (gym.driver.player)(bot);
    let opened = read_state(&machine);
    let started_at = (opened.world, opened.level);
    let mut scored = false;
    let mut holding = false;
    println!(
        "{}: tape ends at frame {frame}, filming {frames} frames under the {} with a {:?} ledger",
        gym.name, gym.driver.name, gym.seed,
    );
    for _ in 0..frames {
        let at_frame = machine.ppu().frame_id();
        let buttons = player.buttons(&machine);
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        let produced = read_state(&machine);
        player.observe(&produced, &machine, at_frame);
        let picture = capture(&machine).ok_or("the PPU has no frame to write")?;
        let pressed = buttons.contains(ControllerButtons::A) && !holding;
        holding = buttons.contains(ControllerButtons::A);
        if pressed {
            let name = format!("event-jump-f{at_frame}-x{:05}.png", produced.absolute_x);
            write_png(&dir.join(name), &picture)?;
            println!(
                "  frame {at_frame}: A goes down at absolute X {}",
                produced.absolute_x
            );
        }
        if !scored && gym.objective.held(&produced, started_at) {
            scored = true;
            // The frame the verdict stops on, from the same `Objective::held` the verdict
            // is scored with. On a `ClearLevel` gym the PPU has already moved to the next
            // area by it, so the still holds the ending and not the jump that bought it.
            let name = format!("event-clear-f{at_frame}-x{:05}.png", produced.absolute_x);
            write_png(&dir.join(name), &picture)?;
            println!(
                "  frame {at_frame}: {} held at absolute X {} — the verdict's last frame",
                gym.objective, produced.absolute_x
            );
        }
        let name = format!("frame-{at_frame:06}-x{:05}.png", produced.absolute_x);
        write_png(&dir.join(name), &picture)?;
    }
    println!("wrote {frames} frames to {}", dir.display());
    Ok(())
}
