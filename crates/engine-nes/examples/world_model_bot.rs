//! A bot that only ever sees the world through the reader, and where that stops
//! working.
//!
//! Every decision is made from the decoded model: the tile grid, the player's own
//! cell from the feet offset, the enemy slots. Nothing about level 1-1's shape is
//! hardcoded, only the load-bearing tile ids the reader learned by watching what
//! the game holds the player up on. The base policy itself lives with the reader,
//! in `tests/support/smb_world_model.rs`, so that the optional test and this
//! example cannot disagree about what "acting on the model" means.
//!
//! That policy is the simplest one the model supports: run right, jump a hole or an
//! enemy the reader can see. Measured on 1-1 it reaches absolute X 899 and then
//! stands there pushing right forever, with no deaths and nothing wrong in the
//! reading, because a set of tiles learned from what he stands on has no word for a
//! wall: a pipe's top row is in the set and its body is not. See
//! `docs/nes-world-model-reader.md`, where this is trap 7.
//!
//! `--reflex` adds the rule that stands in for the missing wall ids: if the
//! controller says right and the world will not move, the game is telling the bot
//! that its model is wrong. It backs off for a moment, runs at the obstruction and
//! jumps on the approach. That buys distance and no reliability: the furthest X
//! swings from 595 to 2601 with `--reflex-look` alone, and the best setting still
//! ends in a game over rather than the flagpole. A reaction rule on top of a model
//! with a hole in it is a lottery, which is the result worth having.
//!
//! `--keep-going` runs past a level change instead of stopping at it, which is how
//! to watch the same bot go blind in 1-2: the load-bearing set is 1-1's vocabulary,
//! gathered by watching what the game holds the player up on in that one level, and
//! an underground area is drawn with ids it never saw.
//!
//! `--shots DIR` writes a PNG of every frame to `DIR`, so the run can be watched
//! rather than read, and names the frames where something happened
//! (`event-jump-*`, `event-death-*`, `event-stuck-*`, `event-clear-*`).
//! `--overlay` adds a second copy of each frame with the reader's own opinion
//! dotted on: green where the model calls a cell ground, red where a cell draws
//! something it does not, blue on the cell he is standing in. `--every N` thins the
//! film to one frame in N.

use std::{
    env,
    error::Error,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
};

#[path = "../tests/support/smb_world_model.rs"]
mod smb;

use engine_nes::{
    ControllerButtons, FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH, NesMachine, write_rgb888,
};
use smb::{
    JUMP_HOLD_FRAMES, MODE_END_GAME, PpuBytes, TILE_CELLS, TILE_COLUMNS, TILE_ROWS,
    TOP_CROP_PIXELS, boot_to_gameplay, is_blank_tile, is_load_bearing, obstruction_ahead,
    player_tile, read_state, run_right_buttons, scroll_x, snapshot, tile_at, tile_grid, wall_ahead,
};

/// Frames without any new ground covered before the bot believes it is blocked.
const BLOCKED_FRAMES: u64 = 6;
/// Frames without any new ground covered before the run is called over.
const STUCK_FRAMES: u64 = 600;
/// The `--reflex` cycle: walk away to get room, then run back and jump on the
/// approach. Pinned flat against a wall he has no run-up to jump with, so the
/// reflex has to leave the wall before it can get over it.
const BACKOFF_FRAMES: u64 = 24;
const CHARGE_FRAMES: u64 = 48;
const REFLEX_CYCLE: u64 = BACKOFF_FRAMES + CHARGE_FRAMES;
/// Absolute X below which the game has restarted the level, so a death is not read
/// as the bot running out of ideas.
const RESPAWN_X: u16 = 128;
/// How far ahead the reflex looks for something drawn before committing to the
/// jump. Measured, not guessed: at 3 columns the run dies at the first pipe, at 4
/// it dies at the second, at 5, 6 and 8 it stops at the third, and from 7 columns
/// on it reaches 2601 and never further. A single tuning constant moving the
/// outcome by 2000 pixels is the finding, not the tuning. See
/// `docs/nes-world-model-reader.md`.
const REFLEX_LOOK_COLUMNS: i16 = 7;
/// How much of the grid to print when the run ends.
const DUMP_COLUMNS: i16 = 11;
const DUMP_ROWS: i16 = 5;
/// Frames the game takes to place a level, counted from the mode byte saying play.
const LEVEL_LOAD_FRAMES: u64 = 300;

/// Print the cells around the player, flagging the ones the model calls ground.
fn dump_grid(tiles: &[u8; TILE_CELLS], column: i16, row: i16) {
    println!("        the grid where he stopped, `*` = in the load-bearing set:");
    for rise in (0..DUMP_ROWS).rev() {
        let r = row - rise;
        let cells: String = (-2..DUMP_COLUMNS - 2)
            .map(|shift| match tile_at(tiles, column + shift, r) {
                Some(tile) if is_load_bearing(tile) => format!(" ${tile:02x}*"),
                Some(tile) => format!(" ${tile:02x} "),
                None => "  .  ".to_owned(),
            })
            .collect();
        println!("    r{r:>2}{cells}");
    }
}

/// The picture the PPU drew, as RGB, or `None` when video output is off.
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

/// Mark the top-left pixel of a grid cell. Cell (column, row) is the block of
/// framebuffer pixels at x = column * 8, y = row * 8 + the status-bar crop, which
/// is the same mapping the background reconstruction is checked against.
fn dot_cell(rgb: &mut [u8], column: i16, row: i16, colour: [u8; 3]) {
    let x = i32::from(column) * 8 + 1;
    let y = i32::from(row) * 8 + i32::from(TOP_CROP_PIXELS) + 1;
    if x < 0 || y < 0 || x >= FRAME_WIDTH as i32 || y >= FRAME_HEIGHT as i32 {
        return;
    }
    let pixel = usize::try_from(y).unwrap_or(0) * FRAME_WIDTH + usize::try_from(x).unwrap_or(0);
    rgb[pixel * 3..pixel * 3 + 3].copy_from_slice(&colour);
}

/// Dot the reader's opinion onto the picture: green where the model calls a cell
/// ground, red where a cell draws something the model does not call ground, blue on
/// the cell he is standing in. A red dot against a Mario who will not move is
/// trap 7 in one image: the pipe's body is scenery as far as the model goes.
fn paint_model(rgb: &mut [u8], frame: &PpuBytes, tiles: &[u8; TILE_CELLS], column: i16, row: i16) {
    for grid_row in 0..TILE_ROWS as i16 {
        for grid_column in 0..TILE_COLUMNS as i16 {
            let Some(tile) = tile_at(tiles, grid_column, grid_row) else {
                continue;
            };
            let colour = if is_load_bearing(tile) {
                [0, 255, 0]
            } else if !is_blank_tile(frame, tile) {
                [255, 0, 0]
            } else {
                continue;
            };
            dot_cell(rgb, grid_column, grid_row, colour);
        }
    }
    dot_cell(rgb, column, row, [60, 130, 255]);
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let rom_path = PathBuf::from(
        args.next()
            .ok_or("usage: world_model_bot ROM [FRAMES] [--reflex] [--keep-going] [--shots DIR] [--overlay] [--every N]")?,
    );
    let mut reflex = false;
    let mut keep_going = false;
    let mut overlay = false;
    let mut shots = None;
    let mut every = 1u64;
    let mut start_delay = 0u64;
    let mut reflex_look = REFLEX_LOOK_COLUMNS;
    let mut expect_look = false;
    let mut budget = None;
    let mut expect_dir = false;
    let mut expect_every = false;
    let mut expect_delay = false;
    for arg in args {
        if expect_delay {
            start_delay = arg.parse()?;
            expect_delay = false;
        } else if expect_dir {
            shots = Some(PathBuf::from(arg));
            expect_dir = false;
        } else if expect_every {
            every = arg.parse()?;
            expect_every = false;
        } else if expect_look {
            reflex_look = arg.parse()?;
            expect_look = false;
        } else if arg == "--every" {
            expect_every = true;
        } else if arg == "--start-delay" {
            expect_delay = true;
        } else if arg == "--reflex-look" {
            expect_look = true;
        } else if arg == "--reflex" {
            reflex = true;
        } else if arg == "--keep-going" {
            keep_going = true;
        } else if arg == "--overlay" {
            overlay = true;
        } else if arg == "--shots" {
            expect_dir = true;
        } else {
            budget = Some(arg.parse()?);
        }
    }
    let frames = budget.unwrap_or(6000);
    let shots = shots
        .map(|dir| {
            fs::create_dir_all(&dir)?;
            Ok::<_, Box<dyn Error>>(dir)
        })
        .transpose()?;

    let rom = fs::read(&rom_path)?;
    let (mut machine, gameplay_frame) = boot_to_gameplay(&rom)?;
    println!(
        "frame {gameplay_frame}: gameplay; policy = run right, jump what the reader sees{}",
        if reflex {
            ", plus the blocked-means-run-at-it reflex"
        } else {
            ""
        }
    );

    // The mode byte says play from the first frame of the level load, and the load
    // takes about 150 more frames to place the level and hand him over. Starting on
    // the mode byte would spend the first two seconds of the run pushing at a world
    // that is not listening, and a stuck detector counted in frames would call that
    // the bot giving up.
    let mut in_control_for = 0;
    for _ in 0..LEVEL_LOAD_FRAMES {
        let state = read_state(&machine);
        if state.is_in_control() && state.is_grounded() {
            in_control_for += 1;
            if in_control_for >= 8 {
                break;
            }
        } else {
            in_control_for = 0;
        }
        machine.run_frame([ControllerButtons::NONE, ControllerButtons::NONE])?;
    }
    if in_control_for < 8 {
        return Err("the game never handed the player over".into());
    }
    for _ in 0..start_delay {
        machine.run_frame([ControllerButtons::NONE, ControllerButtons::NONE])?;
    }
    let handed_over = machine.ppu().frame_id();
    println!(
        "frame {handed_over}: the game handed him over at absolute X 0{}\
         ({start_delay} idle frames)",
        if start_delay > 0 { ", then waited" } else { "" }
    );

    let opened = read_state(&machine);
    let start_level = (opened.world, opened.level);
    let mut furthest_x = 0u16;
    let mut lives = opened.lives;
    let mut deaths = 0u32;
    let mut a_frames = 0u32;
    let mut blocked_frames = 0u32; // frames spent believing the model is wrong
    let mut hold_a = 0u64;
    let mut a_down = false;
    let mut awaiting_respawn = false;
    let mut cleared = false;
    let mut no_progress_for = 0u64;
    let mut stuck = false;
    let mut out_of_lives = false;

    for elapsed in 1..=frames {
        let state = read_state(&machine);
        if state.lives < lives {
            lives = state.lives;
            deaths += 1;
            awaiting_respawn = true;
            println!(
                "  frame {:>5}: died at absolute X {}",
                gameplay_frame + elapsed,
                state.absolute_x
            );
            if let (Some(dir), Some(rgb)) = (&shots, capture(&machine)) {
                let name = format!(
                    "event-death-f{}-x{}.png",
                    gameplay_frame + elapsed,
                    state.absolute_x
                );
                write_png(&dir.join(name), &rgb)?;
            }
        }
        if state.game_mode == MODE_END_GAME {
            // Two deaths at the same obstruction reads as "the bot ran out of
            // ideas" until the game says otherwise by taking the controller away.
            // Say which of the two it was.
            out_of_lives = true;
            println!(
                "  frame {:>5}: the game ended at absolute X {} after {deaths} deaths",
                gameplay_frame + elapsed,
                state.absolute_x
            );
            if let (Some(dir), Some(picture)) = (&shots, capture(&machine)) {
                let name = format!(
                    "event-game-over-f{}-x{}.png",
                    machine.ppu().frame_id(),
                    state.absolute_x
                );
                write_png(&dir.join(name), &picture)?;
            }
            break;
        }
        if !keep_going && (state.world, state.level) != start_level {
            cleared = true;
            println!(
                "  frame {:>5}: cleared world {}.{} at absolute X {} and landed in world {}.{}",
                gameplay_frame + elapsed,
                start_level.0,
                start_level.1,
                furthest_x,
                state.world,
                state.level
            );
            break;
        }
        if awaiting_respawn && state.absolute_x < RESPAWN_X {
            awaiting_respawn = false;
            furthest_x = state.absolute_x;
            no_progress_for = 0;
        }

        let frame: PpuBytes = snapshot(&machine);
        let origin_x = scroll_x(&frame);
        let tiles = tile_grid(&frame, origin_x, 0);
        let (column, row) = player_tile(&state, origin_x);
        let wall = wall_ahead(&tiles, column, row);

        // The reflex reads the game's refusal to move as "your model is wrong about
        // this cell". It backs off to get a run-up, runs at the obstruction and jumps
        // on the approach. The decision has to be made before the frame's buttons are
        // built: setting the hold afterwards presses A one frame late, and one frame
        // is the difference between clearing the first pipe and dying against it.
        let blocked = no_progress_for >= BLOCKED_FRAMES;
        let mut backing_off = false;
        if reflex && blocked {
            blocked_frames += 1;
            let cycle = no_progress_for - BLOCKED_FRAMES;
            if cycle % REFLEX_CYCLE < BACKOFF_FRAMES {
                backing_off = true;
            } else if let Some(step) = obstruction_ahead(&frame, &tiles, column, row, reflex_look)
                && state.is_grounded()
                && state.h_speed > 0
            {
                println!(
                    "  frame {:>5}: reflex jump at x={} hspeed={:+}, obstruction {step} ahead",
                    gameplay_frame + elapsed,
                    state.absolute_x,
                    state.h_speed
                );
                hold_a = JUMP_HOLD_FRAMES;
            }
        }
        let mut buttons = run_right_buttons(&state, &machine, &tiles, column, row, &mut hold_a);
        // A jump starts on the frame A goes down. `run_right_buttons` spends the
        // hold in the same call that sets it, so the counter is already one short
        // by the time the buttons are back.
        let jumping = buttons.contains(ControllerButtons::A) && !a_down;
        a_down = buttons.contains(ControllerButtons::A);
        if backing_off {
            buttons = ControllerButtons::LEFT | ControllerButtons::B;
        }
        if buttons.contains(ControllerButtons::A) {
            a_frames += 1;
        }

        if state.absolute_x > furthest_x {
            furthest_x = state.absolute_x;
            no_progress_for = 0;
        } else {
            no_progress_for += 1;
        }

        if let Some(dir) = &shots {
            let picture =
                capture(&machine).ok_or("video output is disabled, so there is nothing to film")?;
            let frame_id = machine.ppu().frame_id();
            if jumping {
                let name = format!("event-jump-f{frame_id}-x{}.png", state.absolute_x);
                write_png(&dir.join(name), &picture)?;
            }
            if elapsed % every == 0 {
                let name = format!("frame-{frame_id:06}-x{:05}.png", state.absolute_x);
                write_png(&dir.join(&name), &picture)?;
                if overlay {
                    let mut model = picture;
                    paint_model(&mut model, &frame, &tiles, column, row);
                    write_png(&dir.join(name.replace("frame-", "model-")), &model)?;
                }
            } else if overlay {
                let mut model = picture;
                paint_model(&mut model, &frame, &tiles, column, row);
                let name = format!("model-{frame_id:06}-x{:05}.png", state.absolute_x);
                write_png(&dir.join(name), &model)?;
            }
        }

        machine.run_frame([buttons, ControllerButtons::NONE])?;

        if elapsed % 600 == 0 {
            println!(
                "  frame {:>5}: world {}.{} mode={:#04x} x={:>5} furthest={furthest_x:>5} \
                 col={column:>3} row={row:>2} lives={lives} blocked={blocked} wall={wall}",
                gameplay_frame + elapsed,
                state.world,
                state.level,
                state.game_mode,
                state.absolute_x,
            );
        }

        if no_progress_for >= STUCK_FRAMES {
            stuck = true;
            println!(
                "  frame {:>5}: covered no new ground for {STUCK_FRAMES} frames at absolute X {} \
                 with hspeed={:+}, screen y={}/{}, player state {:#04x}, float {}, \
                 and wall_ahead={wall}",
                gameplay_frame + elapsed,
                state.absolute_x,
                state.h_speed,
                state.screen_x,
                state.screen_y,
                state.player_state,
                state.float_state,
            );
            dump_grid(&tiles, column, row);
            if let Some(dir) = &shots {
                let mut picture = capture(&machine)
                    .ok_or("video output is disabled, so there is nothing to film")?;
                let name = format!(
                    "event-stuck-f{}-x{}.png",
                    machine.ppu().frame_id(),
                    state.absolute_x
                );
                write_png(&dir.join(&name), &picture)?;
                paint_model(&mut picture, &frame, &tiles, column, row);
                write_png(&dir.join(name.replace("event-", "model-event-")), &picture)?;
            }
            break;
        }
    }

    println!(
        "furthest absolute X {furthest_x}: {deaths} deaths, {a_frames} frames with A held, \
         {blocked_frames} blocked frames, {}",
        if cleared {
            "cleared the level, which is as far as 1-1's tile vocabulary reaches"
        } else if out_of_lives {
            "ran out of lives, which is not the model running out of answers"
        } else if stuck {
            "stopped by something the model calls scenery"
        } else {
            "ran out of frames"
        }
    );
    if let Some(dir) = &shots {
        println!(
            "wrote frames to {}; a film: ffmpeg -framerate 60 -pattern_type glob \
             -i '{}/frame-*.png' -pix_fmt yuv420p out.mp4",
            dir.display(),
            dir.display()
        );
    }
    Ok(())
}
