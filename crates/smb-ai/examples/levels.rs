//! Phase 4's instrument: start the game at any level, and report what the game says it
//! started.
//!
//! [`boot_to_level`] can already put him anywhere in the cartridge. Whether the game
//! agrees is a different question, and the byte that patch writes is not evidence for
//! it — the byte is the thing under test. This asks three questions that are not the
//! byte: what the game's own status bar has drawn, what the screen under the camera is
//! made of, and what the player runs into when he is turned loose there.
//!
//! Seven modes, all of them reading the same two primitives:
//!
//! ```text
//! levels [ROM]                     the 32-level table, one line per start
//! levels [ROM] map W L             the playfield as a map, one letter per tile pattern
//! levels [ROM] watch W L [FRAMES]  every frame the load changes state, byte or screen
//! levels [ROM] slot W BYTE [FRAMES] [--shots DIR]
//!                              the same watch from a raw level byte, which is how the
//!                              pipe travel slot — the one `level_slot` will not name —
//!                              gets watched and filmed
//! levels [ROM] drive W L [FRAMES] [--shots DIR]
//!                              the scripted player loose on a start, its stop, and a
//!                              PNG per frame for the same cutter the gyms use
//! levels [ROM] drive-all [FRAMES]  all thirty-two starts, eight worlds in parallel
//! levels [ROM] natural [FRAMES]    clear 1-1 the game's own way and report where it says it landed
//! ```
//!
//! `W` is 1-8 and `L` is 1-4, both as the game numbers them, which is the one place in
//! this file where a level number means what the television says rather than what
//! [`level_slot`] writes.

use std::{
    collections::BTreeMap,
    error::Error,
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
};

use engine_nes::{
    ControllerButtons, CpuBus, FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH, NesMachine, write_rgb888,
};
use smb_ai::{policy::BaseBot, skills};
use smb_model::{
    affordances::{WALL_LOOK_COLUMNS, wall_ahead},
    cartridge,
    frame::{
        PpuBytes, pattern_bytes, pattern_hash, player_tile, scroll_x, scroll_y, snapshot, tile_grid,
    },
    profile::{
        GAME_MODE, LEVEL, MODE_END_GAME, MODE_PLAYING, SETUP_PATCH_FRAME, SETUP_START_FRAME,
        SmbState, WORLD, boot_to_gameplay, boot_to_level, boot_to_level_load, level_slot,
        read_state, wait_for_handover,
    },
};

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

/// Frames with no new ground covered before a driven run is called stopped. The house
/// window, from `bot`, because a stop is only comparable across levels if it means the
/// same thing in every one of them.
const STUCK_FRAMES: u64 = 600;

/// The tile ids the PPU is drawing, below the status bar, from both nametables.
///
/// Cheap on purpose: the table asks for it 32 times and the question is only whether two
/// starts are in the same place. Rows 3 and up because the two above are the score, the
/// coins and the timer, which look the same in every level in the game.
fn screen_fingerprint(machine: &NesMachine) -> u64 {
    let nametables = machine.ppu().nametables();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (address, &tile) in nametables.iter().enumerate().take(2 * 0x400) {
        let row = address % 0x400 / 32;
        if !(3..27).contains(&row) {
            continue;
        }
        hash ^= u64::from(tile);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

/// The world field the game drew in its own status bar, as text.
///
/// In this font the tile id *is* the digit — `$00` is "0" through `$09` is "9", and
/// `$28` is the dash — so the label can be read off the nametable without believing a
/// byte. It is the game's claim about where it is, which is why it is printed beside the
/// byte rather than instead of it: the field is drawn when a level changes and not again
/// after, so a fresh start shows whatever the last label was and a respawn shows the
/// label from before that.
fn hud_world(picture: &PpuBytes) -> String {
    let (ox, oy) = (scroll_x(picture), scroll_y(picture));
    let tiles = tile_grid(picture, ox, oy);
    (19..22)
        .map(|column| {
            let glyph = match tiles[2 * 32 + column] {
                0x28 => b'-',
                digit if digit < 10 => b'0' + digit,
                _ => b'?',
            };
            char::from(glyph)
        })
        .collect()
}

/// One line of the table: what was asked for, and the three independent answers.
///
/// `machine` is `None` on a start that never happened, and an `error` outranks a
/// machine: either way the line still names the ask and the slot byte written for it,
/// so a failed row reads beside the row above it instead of vanishing from the table.
fn describe(
    world: u8,
    level: u8,
    machine: Option<&NesMachine>,
    idle_frames: u64,
    error: Option<&str>,
) -> String {
    let slot = level_slot(world, level).unwrap_or(0);
    // `world` is a byte and `level` is already the number the status bar prints.
    let asked = format!("{}.{}", world + 1, level);
    let Some(machine) = machine.filter(|_| error.is_none()) else {
        return format!(
            "{asked}  {slot:#04x}   {}",
            error.unwrap_or("the start itself failed")
        );
    };
    let state = read_state(machine);
    let picture = snapshot(machine);
    let (ox, oy) = (scroll_x(&picture), scroll_y(&picture));
    let tiles = tile_grid(&picture, ox, oy);
    let drawn = |row: usize, column: usize| {
        !pattern_bytes(&picture, tiles[row * 32 + column])
            .iter()
            .all(|&byte| byte == 0)
    };
    // Three counts that say what kind of area this is without naming a tile. `ceiling`
    // is how much of the playfield's top row draws something, `wall` is how far down the
    // left column does, and `full` is the first row that is nearly all one thing — the
    // ground surface at 25 in an overworld and an underground, and the brick ceiling at 3
    // in a castle, which is the only family whose top row is full across the whole width.
    // The palette cannot be that test: 5-2 and 8-2 are underground areas that draw the
    // overworld's blue, so geometry is the channel that survives contact with the game.
    let ceiling = (12..32).filter(|&column| drawn(3, column)).count();
    let wall = (5..21).filter(|&row| drawn(row, 0)).count();
    let full = (3..28)
        .find(|&row| (0..32).filter(|&column| drawn(row, column)).count() > 28)
        .unwrap_or(0);
    format!(
        "{asked}  {slot:#04x}  {:#04x}  {:>5}  {:<4}  {:>6}  {:>5}  {:>4}  {:>4} {:>4}  {:016x}",
        state.level,
        state
            .level_number()
            .map_or_else(|| "?".to_owned(), |number| number.to_string()),
        hud_world(&picture),
        idle_frames,
        state.absolute_x,
        ceiling,
        wall,
        full,
        screen_fingerprint(machine),
    )
}

/// The header the table's columns are described by, so the output explains itself.
const TABLE_HEADER: &str = "asked  slot  byte  level  hud    idle    x     ceil wall full   screen";

/// The directory `--shots` asked for, if the arguments named one.
fn shots_dir(args: &[String]) -> Option<PathBuf> {
    args.iter()
        .position(|arg| arg == "--shots")
        .and_then(|at| args.get(at + 1))
        .map(PathBuf::from)
}

/// Every level in the cartridge, started and described.
///
/// The eight worlds boot on parallel threads because the debug core runs around eighty
/// frames a second and thirty-two sequential starts cost two minutes of wall clock,
/// which is the difference between a table that gets regenerated and one that does not.
fn table(rom: &[u8]) -> Result<(), Box<dyn Error>> {
    let rows = std::thread::scope(|scope| {
        let started: Vec<_> = (0..8u8)
            .map(|world| {
                scope.spawn(move || {
                    (1..=4u8)
                        .map(|level| match boot_to_level_load(rom, world, level) {
                            Ok((mut machine, load_frame)) => {
                                match wait_for_handover(&mut machine) {
                                    Ok(handed_over) => {
                                        let idle = handed_over - load_frame;
                                        describe(world, level, Some(&machine), idle, None)
                                    }
                                    Err(reason) => describe(
                                        world,
                                        level,
                                        Some(&machine),
                                        0,
                                        Some(&reason.to_string()),
                                    ),
                                }
                            }
                            Err(reason) => {
                                describe(world, level, None, 0, Some(&reason.to_string()))
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        started
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    println!("{TABLE_HEADER}");
    for row in rows {
        println!("{row}");
    }
    Ok(())
}

/// The playfield under the camera at the handover, as a map.
///
/// One letter per distinct tile pattern, so two levels that differ only in artwork the
/// reader has never seen still print differently, and `.` is an all-zero pattern rather
/// than a tile id that happens to repeat.
fn map(rom: &[u8], world: u8, level: u8) -> Result<(), Box<dyn Error>> {
    // The load frame rather than `boot_to_level`, so the `idle` column of the line above
    // the map means what it means in the table: frames from the mode byte to the handover.
    let (mut machine, load_frame) = boot_to_level_load(rom, world, level)?;
    let handed_over = wait_for_handover(&mut machine)?;
    println!(
        "{}",
        describe(world, level, Some(&machine), handed_over - load_frame, None,)
    );
    println!("  {TABLE_HEADER}");
    let picture = snapshot(&machine);
    let (ox, oy) = (scroll_x(&picture), scroll_y(&picture));
    let tiles = tile_grid(&picture, ox, oy);
    let mut legend: BTreeMap<u64, char> = BTreeMap::new();
    let mut next = b'a';
    for row in tiles.chunks_exact(32) {
        let mut line = String::new();
        for tile in row {
            let key = pattern_hash(pattern_bytes(&picture, *tile));
            let mark = match legend.get(&key) {
                Some(known) => *known,
                None => {
                    let blank = pattern_bytes(&picture, *tile).iter().all(|&byte| byte == 0);
                    let mark = if blank { '.' } else { next as char };
                    if !blank {
                        next += 1;
                    }
                    legend.insert(key, mark);
                    mark
                }
            };
            line.push(mark);
        }
        println!("    {line}");
    }
    let named: Vec<String> = legend
        .iter()
        .filter(|(_, mark)| **mark != '.')
        .map(|(key, mark)| format!("{mark}={key:08x}"))
        .collect();
    println!("    legend: {}", named.join(" "));
    println!(
        "    palette $3F00..$3F0F: {}",
        picture.palette[..0x10]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    Ok(())
}

/// Watch a load happen, without ever touching the controller.
fn watch(
    rom: &[u8],
    world: u8,
    level: u8,
    frames: u64,
    shots: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let (machine, load_frame) = boot_to_level_load(rom, world, level)?;
    println!("frame {load_frame}: the mode byte says play");
    watch_load(machine, frames, shots)
}

/// The same watch from a raw [`LEVEL`] byte rather than a world and a level.
///
/// This exists for one byte value. In the four worlds entered by pipe, `$01` is the
/// game's own travel between the first and second levels — 334 frames of walking right
/// over the *previous* level's screen with the buttons ignored — and [`level_slot`] will
/// not name it, because it is not a level. It is still the slowest thing a start can do,
/// and the only way to see it is to write the byte.
fn slot(
    rom: &[u8],
    world: u8,
    byte: u8,
    frames: u64,
    shots: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let mut machine = cartridge::machine(rom)?;
    for frame in 1..=SETUP_START_FRAME {
        if frame == SETUP_PATCH_FRAME {
            machine.bus_mut().write(WORLD, world);
            machine.bus_mut().write(LEVEL, byte);
        }
        let buttons = if frame == SETUP_START_FRAME {
            ControllerButtons::START
        } else {
            ControllerButtons::NONE
        };
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        if machine.bus().peek(GAME_MODE) == MODE_PLAYING {
            println!("frame {frame}: the mode byte says play, byte was {byte:#04x}");
            return watch_load(machine, frames, shots);
        }
    }
    Err("the setup script never reached normal play".into())
}

/// The frames [`wait_for_handover`] runs through and throws away, printed, and optionally
/// written out as the PNG series the cutter expects.
///
/// A byte that changes while the screen holds is a pointer running ahead of the area it
/// names; a screen that changes with the byte is a load. Both happen inside one travel.
fn watch_load(
    mut machine: NesMachine,
    frames: u64,
    shots: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    if let Some(dir) = shots {
        std::fs::create_dir_all(dir)?;
    }
    let mut last = (0u8, 0u8, 0u64);
    for _ in 0..frames {
        machine.run_frame([ControllerButtons::NONE, ControllerButtons::NONE])?;
        let frame_id = machine.ppu().frame_id();
        let state = read_state(&machine);
        let now = (
            state.player_state,
            state.level,
            screen_fingerprint(&machine),
        );
        if now != last && (now.0 != last.0 || now.1 != last.1 || now.2 != last.2) {
            println!(
                "frame {frame_id}: state {:#04x} byte {:#04x} level {} x {} screen {:016x}",
                now.0,
                now.1,
                state
                    .level_number()
                    .map_or_else(|| "?".into(), |n| n.to_string()),
                state.absolute_x,
                now.2,
            );
        }
        if let Some(dir) = shots
            && let Some(picture) = capture(&machine)
        {
            write_png(
                &dir.join(format!("frame-{frame_id:06}-x{:05}.png", state.absolute_x)),
                &picture,
            )?;
        }
        last = now;
    }
    Ok(())
}

/// Turn the scripted player loose on a start and say where it stopped and what the
/// model had to say about that pixel.
///
/// This is the inventory the plan asks for: the same player, the same rules, thirty-two
/// beginnings. What a level contributes is the stop — the position, the reason the run
/// ended, and whether the model had a name for the thing there. One line per start, with
/// the level-byte changes folded in above it, because `drive-all` runs eight of these on
/// eight threads and two threads writing half a line each is not a report.
fn drive_line(
    rom: &[u8],
    world: u8,
    level: u8,
    frames: u64,
    shots: Option<&Path>,
) -> Result<String, Box<dyn Error>> {
    // `world` is a byte and `level` is already the number the status bar prints.
    let asked = format!("{}.{}", world + 1, level);
    let (mut machine, _) = boot_to_level(rom, world, level)?;
    let opened = read_state(&machine);
    let mut player = (skills::SCRIPTED.player)(BaseBot::default());
    let mut last = (opened.world, opened.level);
    let mut furthest = opened.absolute_x;
    let mut lives = opened.lives;
    // Where each life was lost, in the only coordinate that survives a respawn: the
    // furthest X reached before it. A death rewinds him to the level's own start, so the
    // stuck detector below fires on the respawn rather than on the obstacle, and without
    // this the inventory would report thirty "no new ground" stops and no obstacle.
    let mut deaths: Vec<(u64, u16)> = Vec::new();
    let mut refusals = 0u32;
    let mut no_progress_for = 0u64;
    let mut stopped = "ran out of frames";
    let mut changes = Vec::new();
    for _ in 0..frames {
        let frame_id = machine.ppu().frame_id();
        if let Some(dir) = shots
            && let Some(picture) = capture(&machine)
        {
            // The names `film-smb-run.py` expects, so a start anywhere in the cartridge
            // is filmable with the same cutter the gyms use.
            let opened = read_state(&machine);
            write_png(
                &dir.join(format!("frame-{frame_id:06}-x{:05}.png", opened.absolute_x)),
                &picture,
            )?;
        }
        let state = read_state(&machine);
        if state.lives < lives {
            lives = state.lives;
            deaths.push((frame_id, furthest));
            if let Some(dir) = shots
                && let Some(picture) = capture(&machine)
            {
                write_png(
                    &dir.join(format!("event-death-f{frame_id}-x{furthest}.png")),
                    &picture,
                )?;
            }
        }
        if state.game_mode == MODE_END_GAME {
            stopped = "the game ended the run";
            break;
        }
        if (state.world, state.level) != last {
            changes.push(format!(
                "  frame {frame_id}: byte {:#04x} -> {:#04x} at absolute X {furthest}, \
                 level {} -> {}",
                last.1,
                state.level,
                SmbState {
                    world: last.0,
                    level: last.1,
                    ..state
                }
                .level_number()
                .map_or_else(|| "?".into(), |n| n.to_string()),
                state
                    .level_number()
                    .map_or_else(|| "?".into(), |n| n.to_string()),
            ));
            last = (state.world, state.level);
        }
        let buttons = player.buttons(&machine);
        if state.absolute_x > furthest {
            furthest = state.absolute_x;
            no_progress_for = 0;
        } else {
            no_progress_for += 1;
            if no_progress_for > STUCK_FRAMES {
                stopped = "no new ground for 600 frames";
                break;
            }
        }
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        let produced = read_state(&machine);
        if player.observe(&produced, &machine, frame_id).is_some() {
            refusals += 1;
        }
    }
    let state = read_state(&machine);
    let picture = snapshot(&machine);
    let origin_x = scroll_x(&picture);
    let (column, row) = player_tile(&state, origin_x);
    let wall = wall_ahead(
        &picture,
        origin_x,
        &state,
        player.ledger(),
        WALL_LOOK_COLUMNS,
    );
    let control = if state.is_in_control() {
        "the game still has him"
    } else {
        "the game has taken the controller"
    };
    let died = if deaths.is_empty() {
        "never died".to_owned()
    } else {
        let places = deaths
            .iter()
            .map(|(frame, x)| format!("absolute X {x} on frame {frame}"))
            .collect::<Vec<_>>()
            .join(", then ");
        format!("died at {places}")
    };
    let report = format!(
        "{asked}: {stopped} at absolute X {furthest}; {died}; {refusals} refused buttons; \
         feet on cell ({column}, {row}); the model there is {}; {control}; \
         {} cells in the ledger",
        wall.map_or_else(
            || "nothing standing against him".to_owned(),
            |wall| format!(
                "a wall {} px tall starting at pixel {}",
                wall.height_pixels(),
                wall.x
            ),
        ),
        player.ledger().len(),
    );
    Ok([changes.join("\n"), report]
        .iter()
        .filter(|line| !line.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n"))
}

/// [`drive_line`], printed.
fn drive(
    rom: &[u8],
    world: u8,
    level: u8,
    frames: u64,
    shots: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    if let Some(dir) = shots {
        std::fs::create_dir_all(dir)?;
        println!("writing a PNG per frame to {}", dir.display());
    }
    println!("{}", drive_line(rom, world, level, frames, shots)?);
    Ok(())
}

/// Every level in the cartridge driven by the same player, eight worlds in parallel.
///
/// The scripted driver is the control, not the subject: what is being inventoried is the
/// levels, and a stop is only worth reading as a level's difficulty if the thing that
/// stopped is identical in all thirty-two runs.
fn drive_all(rom: &[u8], frames: u64) -> Result<(), Box<dyn Error>> {
    let lines = std::thread::scope(|scope| {
        let started: Vec<_> = (0..8u8)
            .map(|world| {
                scope.spawn(move || {
                    (1..=4u8)
                        .map(|level| {
                            drive_line(rom, world, level, frames, None).unwrap_or_else(|reason| {
                                format!("{}.{}: never started — {reason}", world + 1, level)
                            })
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        started
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

/// Clear 1-1 the way the game intends and report where it says it put him.
///
/// The control for every claim this file makes about a patched start: the same screen,
/// reached without writing a byte, with the status bar the game drew itself.
fn natural(rom: &[u8], frames: u64) -> Result<(), Box<dyn Error>> {
    let (mut machine, _) = boot_to_gameplay(rom)?;
    wait_for_handover(&mut machine)?;
    let mut player = (skills::SCRIPTED.player)(BaseBot::default());
    let opened = read_state(&machine);
    let start = (opened.world, opened.level);
    for _ in 0..frames {
        let frame_id = machine.ppu().frame_id();
        let buttons = player.buttons(&machine);
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        let produced = read_state(&machine);
        player.observe(&produced, &machine, frame_id);
        if (produced.world, produced.level) != start {
            println!(
                "frame {frame_id}: the byte moved {:#04x} -> {:#04x} in player state {:#04x}",
                start.1, produced.level, produced.player_state,
            );
            break;
        }
    }
    let handed_over = wait_for_handover(&mut machine)?;
    let state = read_state(&machine);
    let picture = snapshot(&machine);
    println!(
        "the game says byte {:#04x} (level {}), its own status bar says {}, handed over on \
         frame {handed_over}, screen {:016x}",
        state.level,
        state
            .level_number()
            .map_or_else(|| "?".to_owned(), |number| number.to_string()),
        hud_world(&picture),
        screen_fingerprint(&machine),
    );
    Ok(())
}

/// `W L` from the command line, as the game numbers them.
fn level_pair(args: &[String], at: usize) -> Result<(u8, u8), Box<dyn Error>> {
    let world = args[at].parse::<u8>()?.checked_sub(1).ok_or("world 0")?;
    let level = args[at + 1].parse::<u8>()?;
    if level == 0 || level > 4 {
        return Err("levels are numbered 1 to 4".into());
    }
    Ok((world, level))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(rom) = cartridge::read_from(args.first().map(PathBuf::from))? else {
        return Ok(());
    };
    let rom = rom.as_slice();
    match args.get(1).map(String::as_str) {
        Some("map") => map(rom, level_pair(&args, 2)?.0, level_pair(&args, 2)?.1),
        Some("watch") => {
            let (world, level) = level_pair(&args, 2)?;
            watch(
                rom,
                world,
                level,
                args.get(4).map_or(900, |a| a.parse().unwrap_or(900)),
                shots_dir(&args).as_deref(),
            )
        }
        Some("slot") => {
            let world = args
                .get(2)
                .and_then(|a| a.parse::<u8>().ok())
                .filter(|w| (1..=8).contains(w))
                .ok_or("world must be 1-8")?
                - 1;
            let byte = args
                .get(3)
                .map(|a| a.strip_prefix("0x").unwrap_or(a))
                .and_then(|a| u8::from_str_radix(a, 16).ok())
                .ok_or("byte must be a hex value like 01 or 0x01")?;
            slot(
                rom,
                world,
                byte,
                args.get(4).map_or(900, |a| a.parse().unwrap_or(900)),
                shots_dir(&args).as_deref(),
            )
        }
        Some("drive") => {
            let (world, level) = level_pair(&args, 2)?;
            let shots = shots_dir(&args);
            drive(
                rom,
                world,
                level,
                args.get(4).map_or(6000, |a| a.parse().unwrap_or(6000)),
                shots.as_deref(),
            )
        }
        Some("drive-all") => {
            drive_all(rom, args.get(2).map_or(6000, |a| a.parse().unwrap_or(6000)))
        }
        Some("natural") => natural(rom, args.get(2).map_or(6000, |a| a.parse().unwrap_or(6000))),
        _ => table(rom),
    }
}
