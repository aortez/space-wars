//! Optional world-model test against a real Super Mario Bros. cartridge.
//!
//! Every other test in the workspace drives a `test_rom` builder, so nothing else
//! here is a regression net for reading *semantic* state out of a shipped game.
//! This one is: it boots `smb.nes`, runs DirtSim's setup script, and asserts the
//! decoded RAM map, the tile-frame decode and checkpoint rollback.
//!
//! ROMs are not in Git (`.gitignore` keeps `data/` out), so the test skips rather
//! than fails when the cartridge is absent. Point it at an image with
//! `SPACEWARS_SMB_ROM=/path/to/smb.nes`, or leave it at `data/roms/smb.nes`.
//!
//! What a bot does with this reading is tested in `smb-ai`, the model's other
//! consumer and the only one that can be wrong about a wall.

use std::collections::BTreeSet;

use sha2::{Digest as _, Sha256};

use engine_nes::{ControllerButtons, CpuBus, NesMachine, StateHash};
use smb_model::cartridge;
use smb_model::frame::{
    is_blank_tile, is_load_bearing, player_surface_row_from_sprites, player_tile,
    reconstruct_background, scroll_x, scroll_y, snapshot, sprite_covered_cells, tile_at, tile_grid,
};
use smb_model::profile::{
    FLOAT_JUMPING, HANDOVER_FRAMES, LEVEL, LEVEL_LOAD_FRAMES, MODE_PLAYING, SETUP_PATCH_FRAME,
    SETUP_START_FRAME, STATE_NORMAL, STATE_TRAVEL, SmbState, WORLD, boot_to_gameplay,
    boot_to_level, buttons_the_game_read, level_slot, read_state, wait_for_handover,
};

/// The one ROM revision this address map was verified against, as the SHA-256 a
/// person can check against a file.
///
/// The model pins the same image as a `CartridgeIdentity` and refuses to read any
/// other, so this is not what stops a wrong cartridge being misread. It is the check
/// that the number documents quote is still the number for the file the profile was
/// written for, which is a hash the model deliberately does not implement.
const SMB_SHA256: &str = "f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de";

fn assert_pinned_revision(rom: &[u8]) {
    let mut hasher = Sha256::new();
    hasher.update(rom);
    let digest = format!("{:x}", hasher.finalize());
    assert_eq!(
        digest, SMB_SHA256,
        "cartridge is not the revision this RAM map was verified against",
    );
}

/// Drive Mario right, hopping on a fixed cadence so the float state is covered.
fn policy_buttons(frames_into_gameplay: u64) -> ControllerButtons {
    let mut buttons = ControllerButtons::RIGHT;
    if frames_into_gameplay % 90 < 12 {
        buttons |= ControllerButtons::A;
    }
    buttons
}

/// What a fixed input script did, so the assertions read as behaviour rather
/// than as a transcript of one particular frame.
struct ScriptOutcome {
    gameplay_frame: u64,
    furthest_x: u16,
    saw_airborne: bool,
    saw_enemy_ahead: bool,
    end_state: SmbState,
    end_hash: StateHash,
}

/// Run a fixed input script from a cold boot and decode it every frame.
fn run_script(rom: &[u8], frames: u64) -> ScriptOutcome {
    let (mut machine, gameplay_frame) =
        boot_to_gameplay(rom).expect("setup script should reach normal play");
    let mut outcome = ScriptOutcome {
        gameplay_frame,
        furthest_x: 0,
        saw_airborne: false,
        saw_enemy_ahead: false,
        end_state: read_state(&machine),
        end_hash: machine.state_hash(),
    };
    for elapsed in 1..=frames {
        machine
            .run_frame([policy_buttons(elapsed), ControllerButtons::NONE])
            .expect("frame should run");
        let state = read_state(&machine);
        outcome.furthest_x = outcome.furthest_x.max(state.absolute_x);
        outcome.saw_airborne |= state.float_state == FLOAT_JUMPING;
        outcome.saw_enemy_ahead |= state
            .enemies(&machine)
            .first()
            .is_some_and(|enemy| enemy.delta_x.abs() < 256);
        outcome.end_state = state;
        outcome.end_hash = machine.state_hash();
    }
    outcome
}

#[test]
fn the_smb_ram_map_decodes_a_booted_cartridge_and_a_repeated_script_agrees() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    assert_pinned_revision(&rom);

    let run = run_script(&rom, 300);
    let state = run.end_state;

    // The setup script is DirtSim's: one Start press on frame 120.
    assert_eq!(
        run.gameplay_frame, SETUP_START_FRAME,
        "Start on frame 120 should start the game on frame 120",
    );
    assert_eq!(state.game_mode, MODE_PLAYING, "should still be normal play");
    assert_eq!(
        (state.world, state.level),
        (0, 0),
        "both are zero-based, so 0-0 is level 1-1",
    );
    assert_eq!(state.powerup, 0, "Mario should still be small");
    assert_eq!(
        state.player_state, STATE_NORMAL,
        "and not mid-death or pipe"
    );
    assert!(state.lives >= 1, "the script never dies, so lives hold");

    // Walking right for five seconds has to move the split absolute-X pair,
    // and the periodic A press has to show up as a jump rather than nothing.
    assert!(
        run.furthest_x > 200,
        "running right for 300 frames should cross the screen, got {}",
        run.furthest_x,
    );
    assert!(
        run.saw_airborne,
        "the scripted hops should read back as airborne frames",
    );

    // The enemy slots are the other half of the map: a goomba has to show up
    // ahead of the player rather than only being readable as raw bytes.
    assert!(
        run.saw_enemy_ahead,
        "1-1 should put an enemy near the player while running right",
    );

    // Same ROM, same script, same machine: the reader is a pure function of a
    // reproducible core, which is what makes rollout search sound at all.
    let repeat = run_script(&rom, 300);
    assert_eq!(
        repeat.end_hash, run.end_hash,
        "a repeated script must reproduce the end state exactly",
    );
    assert_eq!(repeat.end_state, state, "and every decoded field");
    assert_eq!(repeat.furthest_x, run.furthest_x);
}

#[test]
fn checkpoint_restore_branches_a_frame_and_returns_to_it_exactly() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    assert_pinned_revision(&rom);

    let (mut machine, _) = boot_to_gameplay(&rom).expect("setup script should reach normal play");
    for elapsed in 1..=240 {
        machine
            .run_frame([policy_buttons(elapsed), ControllerButtons::NONE])
            .expect("frame should run");
    }

    // A planner explores by branching forward and rewinding, so the rewind has
    // to be bit-exact or every pruned branch is a lie.
    let root = machine.checkpoint();
    let root_hash = machine.state_hash();
    let actions = [
        ControllerButtons::NONE,
        ControllerButtons::RIGHT | ControllerButtons::B,
        ControllerButtons::RIGHT | ControllerButtons::A,
        ControllerButtons::A,
    ];
    let mut outcomes = Vec::new();
    for action in actions {
        machine.restore(&root).expect("root should restore");
        assert_eq!(
            machine.state_hash(),
            root_hash,
            "restore must reproduce the checkpoint exactly",
        );
        for _ in 0..120 {
            machine
                .run_frame([action, ControllerButtons::NONE])
                .expect("branch should run");
        }
        outcomes.push(read_state(&machine).absolute_x);
    }

    // Four different macro-actions from one frame must not collapse into one
    // outcome, or the branch is not actually reading the input.
    let mut distinct = outcomes.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() >= 3,
        "four macro-actions should not mostly land in the same place: {outcomes:?}",
    );

    machine.restore(&root).expect("final rewind should work");
    assert_eq!(
        machine.state_hash(),
        root_hash,
        "and land exactly on the root"
    );
}

#[test]
fn the_tile_frame_rebuilds_the_background_the_ppu_actually_drew() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    assert_pinned_revision(&rom);

    let (mut machine, _) = boot_to_gameplay(&rom).expect("setup script should reach normal play");

    // Sample a stationary, grounded frame: a moving camera makes the tile grid
    // and the pixels disagree for reasons the decode is not responsible for.
    // Thirty run frames from the spawn keep Mario inside the first screen, so
    // the camera latch is still zero and the grid lines up with the pixels.
    let mut sampled = false;
    for elapsed in 1..=600 {
        machine
            .run_frame([
                ControllerButtons::RIGHT | ControllerButtons::B,
                ControllerButtons::NONE,
            ])
            .expect("frame should run");
        if elapsed >= 30 && read_state(&machine).is_grounded() {
            sampled = true;
            break;
        }
    }
    assert!(
        sampled,
        "Mario should be standing on something by frame 600"
    );

    let state = read_state(&machine);
    let frame = snapshot(&machine);
    let Some(framebuffer) = machine.ppu().framebuffer() else {
        eprintln!("skipping the pixel comparison: video output is disabled");
        return;
    };

    let (column, row) = player_tile(&state, scroll_x(&frame));
    let tiles = tile_grid(&frame, scroll_x(&frame), scroll_y(&frame));

    // Terrain has to agree with what the game is standing on, and "standing on"
    // is where the difficulty is. The row under his feet has to be a tile the
    // game actually lets him rest on, and the two rows his body occupies must not
    // be. Both halves are load-bearing: the first version of this check asked
    // only that the feet tile drew some pixels, and it passed four rows too high
    // because the decorative hill overhead is opaque.
    let feet = tile_at(&tiles, column, row).expect("the player is on the decoded grid");
    assert!(
        is_load_bearing(feet),
        "the tile under the player at ({column},{row}) is ${feet:02x}, an id the \
         game never holds him up on, so the feet offset is wrong",
    );
    for above in [row - 1, row - 2] {
        let body = tile_at(&tiles, column, above).expect("the player's body is on the grid");
        assert!(
            !is_load_bearing(body),
            "the player's body at ({column},{above}) is inside ${body:02x}, a tile \
             that carries weight, so the feet offset is wrong",
        );
    }

    // The feet offset is the one number in the reader that no RAM byte states
    // outright, so hold it against something the emulator produced rather than
    // against the constant: the row the player's own sprite list stands him on.
    let from_sprites = player_surface_row_from_sprites(&state, &frame)
        .expect("the player's sprite should be drawn on a grounded frame");
    assert_eq!(
        from_sprites, row,
        "the bottom edge of the player's sprite and PLAYER_FEET_OFFSET_PIXELS \
         disagree about which row he stands on",
    );

    // SMB's sky filler is `$24`, not `$00`. Emptiness has to come from the
    // pattern, or the whole sky reads as a wall of solid tiles.
    let filler = tile_at(&tiles, 0, 12).expect("row 12 is on the grid");
    assert_eq!(filler, 0x24, "SMB clears the sky with $24");
    assert!(
        is_blank_tile(&frame, filler),
        "and $24 draws nothing, so blankness is a property of the pattern",
    );

    // The real proof: rebuild every sprite-free cell from the nametables, the
    // cartridge's own mirroring and the CHR pattern table, and compare the
    // transparency mask with the pixels the core rendered.
    let covered = sprite_covered_cells(&frame);
    let exact = reconstruct_background(&frame, framebuffer, &covered, 0);
    assert!(
        exact.pixels > 40_000,
        "too few sprite-free pixels to prove anything: {}",
        exact.pixels,
    );
    let columns: Vec<(usize, u32)> = exact
        .wrong_pixels_by_column
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(column, count)| (column, *count))
        .collect();
    assert_eq!(
        exact.wrong_pixels, 0,
        "background reconstruction disagreed on {} of {} pixels across {columns:?}",
        exact.wrong_pixels, exact.pixels,
    );
    assert!(
        exact.cells > 500,
        "most of the screen should be sprite-free"
    );

    // One frame proves the decode lines up; a few hundred prove the split between
    // terrain and scenery is a property of the game rather than of where this
    // script happened to stop. SMB paints its hills and bushes with opaque tiles
    // and walks Mario straight through them, so a reader that reads solidity from
    // opacity parks a bot inside a hill.
    let mut grounded = 0;
    let mut carried = 0;
    for _ in 0..400 {
        machine
            .run_frame([
                ControllerButtons::RIGHT | ControllerButtons::B,
                ControllerButtons::NONE,
            ])
            .expect("frame should run");
        let state = read_state(&machine);
        if !state.is_grounded() {
            continue;
        }
        let frame = snapshot(&machine);
        let (origin_x, origin_y) = (scroll_x(&frame), scroll_y(&frame));
        let tiles = tile_grid(&frame, origin_x, origin_y);
        let (column, row) = player_tile(&state, origin_x);
        let Some(feet) = tile_at(&tiles, column, row) else {
            continue;
        };
        grounded += 1;
        carried += usize::from(is_load_bearing(feet));
        for above in [row - 1, row - 2] {
            if let Some(body) = tile_at(&tiles, column, above) {
                assert!(
                    !is_load_bearing(body),
                    "Mario's body sat inside ${body:02x} at ({column},{above}) while \
                     the game reported him grounded on row {row}",
                );
            }
        }
    }
    assert!(
        grounded > 100,
        "the sweep should find Mario standing on something often, got {grounded}"
    );
    // The residue is real, not slack: he reads as grounded on a few frames whose
    // feet row is sky, when the game's own flag lags a step behind a ledge.
    assert!(
        carried * 10 >= grounded * 9,
        "the feet row should be load-bearing on nine in ten grounded frames,          got {carried} of {grounded}",
    );
}

/// A press pattern that reaches every button and both edges of every button.
///
/// Held presses matter as much as single frames: an envelope is a hold, and a copy
/// that updated on the transition but not while a button stayed down would look
/// correct through half of any real run.
fn press_pattern(frame: u64) -> ControllerButtons {
    let right = ControllerButtons::RIGHT;
    match frame % 12 {
        0..=4 => right | ControllerButtons::A,
        5 => right | ControllerButtons::A | ControllerButtons::B,
        6 => ControllerButtons::NONE,
        7 => ControllerButtons::LEFT,
        8 => right | ControllerButtons::B,
        9 => right | ControllerButtons::SELECT,
        10 => right | ControllerButtons::START,
        _ => right | ControllerButtons::DOWN,
    }
}

/// The assumption under every measured envelope, pinned.
///
/// A skill's carry is a claim about what twenty frames of A did, and the claim is
/// only worth the frame the A arrived on. The buttons are a host-side argument to
/// `run_frame` that nothing else in the tree keeps, so the only way to check the
/// alignment is to ask the game: `$074A` is its own copy of what player 1 is holding,
/// and it is written inside the frame, from the port the frame was handed.
///
/// So the two assertions below are the whole input path in one line each. Before a
/// frame runs, the byte still holds the previous frame's ask — the copy has not
/// happened yet, which is why reading it early is not the same as reading it late.
/// After the frame runs, it holds this frame's ask exactly, on every frame, including
/// the frame a button is released and the frames a button is held.
///
/// This is DirtSim's `gameInputCopied` check, which will not time a jump response
/// until the game's input mask has moved to what it asked for. Here it is a test
/// rather than a probe, because the thing it protects is a measurement already
/// committed to a document.
#[test]
fn the_game_takes_the_input_a_frame_was_handed_on_that_frame() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    assert_pinned_revision(&rom);
    let (mut machine, _) = boot_to_gameplay(&rom).expect("setup script should reach normal play");
    wait_for_handover(&mut machine).expect("the game should hand the player over");

    let mut asked = ControllerButtons::NONE;
    for _ in 0..60 {
        assert_eq!(
            buttons_the_game_read(&machine),
            asked,
            "the game's input byte moved before the frame that was handed it ran, so the              copy is not inside the frame",
        );
        asked = press_pattern(machine.ppu().frame_id());
        machine
            .run_frame([asked, ControllerButtons::NONE])
            .expect("frame should run");
        assert_eq!(
            buttons_the_game_read(&machine),
            asked,
            "the game did not take the buttons its frame was handed",
        );
    }
}

/// What the PPU is drawing below the status bar, at the scroll every fresh start uses.
///
/// The nametables rather than the CHR patterns, because the question asked of this is
/// only whether two starts are in the same place, and 1,536 tile ids answer that without
/// copying 12 KiB of pattern memory per start. Both nametables, because a level's second
/// screen is generated during its load and is part of where the load put him, and rows
/// 3..27 because the two above them are the score, the coin count and the timer, which
/// are the same in every level in the game.
fn playfield_fingerprint(machine: &NesMachine) -> u64 {
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

/// Start every level of every world with the two-byte patch, and ask the game rather
/// than the patch what it got.
///
/// Four things have to hold together before Phase 4's inventory can be run at all, and
/// each fails in its own way. The game has to hand the player over, or the start is
/// unusable. The byte has to read back the slot that was written, or the patch did not
/// reach the loader. The level the reader decodes has to be the level that was asked
/// for, or every gym in the project is filed under the wrong name. And the screen under
/// the camera has to differ from every other start's, or the patch moved a byte and not
/// a world.
///
/// The third of those is the one that was wrong before it was measured. The obvious
/// reading of `$0760` — the level minus one, since a boot leaves 0 there while the status
/// bar reads 1-1 — puts four of the eight worlds one level short of where the game says
/// they are, and nothing in the byte itself can tell you so.
///
/// The eight worlds boot in parallel threads, which is not how the rest of this file
/// works: the debug core runs somewhere around eighty frames a second, thirty-two
/// sequential starts put this one test past two minutes, and the wall clock a test costs
/// is what decides whether it gets run at all.
#[test]
fn a_patched_start_hands_the_player_over_at_every_level_and_the_game_names_it_back() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    assert_pinned_revision(&rom);
    let rom = rom.as_slice();

    let starts = std::thread::scope(|scope| {
        let booted: Vec<_> = (0..8u8)
            .map(|world| {
                scope.spawn(move || {
                    (1..=4u8)
                        .map(|level| Start {
                            world,
                            level,
                            machine: boot_to_level(rom, world, level).unwrap_or_else(|error| {
                                panic!(
                                    "world {} level {level} never handed the player over: {error}",
                                    world + 1,
                                )
                            }),
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        booted
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    let mut screens = BTreeSet::new();
    for start in starts {
        let world = start.world + 1;
        let level = start.level;
        let (machine, _) = start.machine;
        let state = read_state(&machine);
        assert!(
            state.is_in_control() && state.is_grounded(),
            "world {world} level {level} reported a handover in state {:02X}, float {:02X}, x {}",
            state.player_state,
            state.float_state,
            state.absolute_x,
        );
        assert_eq!(
            state.world, start.world,
            "world {world} level {level} moved the world byte and not the level byte"
        );
        assert_eq!(
            Some(state.level),
            level_slot(start.world, level),
            "world {world} level {level} is not playing the slot the start wrote",
        );
        assert_eq!(
            state.level_number(),
            Some(level),
            "world {world} is playing slot {} while the start asked for level {level}",
            state.level,
        );
        assert!(
            screens.insert(playfield_fingerprint(&machine)),
            "world {world} level {level} draws a screen some other start already drew",
        );
    }
    assert_eq!(screens.len(), 32, "the 32 levels are not 32 screens");
}

/// One start that booted, kept whole because the assertions belong on the thread that
/// joined the workers and not on the worker that cannot report a failure cleanly.
struct Start {
    world: u8,
    level: u8,
    machine: (NesMachine, u64),
}

/// The slot the level-start primitive never writes, measured because it is the reason
/// the handover budget is what it is.
///
/// In the four pipe worlds `$01` is not a level: it is the game's own instruction to
/// travel. A start written there loads for 157 frames, then walks him right across the
/// *previous* level's screen for 334 frames in player state `$07` with his buttons
/// ignored, advances the level byte to `$02` two frames before that walk ends, loads
/// again, and hands him over 577 frames after the press — 591 in worlds 2 and 7. That is
/// more than twice what a plain level needs and more than the 300 that used to be the
/// constant. A caller that patches a byte by hand rather than going through
/// [`boot_to_level`] can land here, and on the old number it would have concluded that
/// four of the eight worlds cannot be started at all.
///
/// The wait is written out rather than called, because the frames
/// [`wait_for_handover`] runs through and throws away are exactly what this test is
/// about; the loop below is its loop, with the interior kept.
#[test]
fn the_pipe_travel_slot_hands_the_player_over_too_and_only_past_a_levels_budget() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    assert_pinned_revision(&rom);

    let mut machine = cartridge::machine(&rom).expect("cartridge should load");
    for frame in 1..=SETUP_START_FRAME {
        if frame == SETUP_PATCH_FRAME {
            machine.bus_mut().write(WORLD, 0);
            machine.bus_mut().write(LEVEL, 1);
        }
        let buttons = if frame == SETUP_START_FRAME {
            ControllerButtons::START
        } else {
            ControllerButtons::NONE
        };
        machine
            .run_frame([buttons, ControllerButtons::NONE])
            .expect("frame should run");
    }

    let mut in_control_for = 0;
    let mut travel_for = 0;
    let mut longest_travel = 0;
    let mut byte_moved_midwalk = false;
    let mut handed_over = 0;
    for _ in 0..LEVEL_LOAD_FRAMES {
        let state = read_state(&machine);
        in_control_for = if state.is_in_control() && state.is_grounded() {
            in_control_for + 1
        } else {
            0
        };
        travel_for = if state.player_state == STATE_TRAVEL {
            travel_for + 1
        } else {
            0
        };
        byte_moved_midwalk |= state.player_state == STATE_TRAVEL && state.level != 1;
        longest_travel = longest_travel.max(travel_for);
        if in_control_for >= HANDOVER_FRAMES {
            handed_over = machine.ppu().frame_id();
            break;
        }
        machine
            .run_frame([ControllerButtons::NONE, ControllerButtons::NONE])
            .expect("frame should run");
    }
    assert_ne!(
        handed_over, 0,
        "the travel slot should hand the player over inside the {LEVEL_LOAD_FRAMES}-frame \
         budget the model is built on",
    );

    let idle_frames = handed_over - SETUP_START_FRAME;
    assert!(
        idle_frames > 300,
        "the travel walk was measured at 577 to 591 idle frames, which is what \
         {LEVEL_LOAD_FRAMES} of budget buys; this cleared it in {idle_frames}, so the \
         constant should be lowered rather than left oversized",
    );
    assert!(
        idle_frames <= 640,
        "{idle_frames} idle frames is past the 577 to 591 this was measured at, so either \
         the walk has grown or a start is failing to hand over and timing out; either way \
         {LEVEL_LOAD_FRAMES} needs a second look",
    );
    assert!(
        longest_travel >= 300,
        "the walk is the whole reason this slot is slow: measured at 334 consecutive frames \
         of state {STATE_TRAVEL:#04x}, this run held it for {longest_travel}",
    );
    assert!(
        byte_moved_midwalk,
        "the level byte is documented to reach `$02` while he is still in {STATE_TRAVEL:#04x}, \
         which is the clearest single frame showing the byte is not the label",
    );

    let state = read_state(&machine);
    assert_eq!(
        state.level_number(),
        Some(2),
        "the travel slot delivers the world's second level, and the byte it arrives on \
         is the reason the byte is not the label",
    );
}
