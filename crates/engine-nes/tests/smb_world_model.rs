//! Optional world-model test against a real Super Mario Bros. cartridge.
//!
//! Every other test in this crate drives a `test_rom` builder, so nothing here
//! is a regression net for reading *semantic* state out of a shipped game. This
//! one is: it boots `smb.nes`, runs DirtSim's setup script, and asserts the
//! decoded RAM map, the tile-frame decode and checkpoint rollback.
//!
//! ROMs are not in Git (`.gitignore` keeps `data/` out), so the test skips
//! rather than fails when the cartridge is absent. Point it at an image with
//! `SPACEWARS_SMB_ROM=/path/to/smb.nes`, or leave it at `data/roms/smb.nes`.

use std::{env, fs, path::PathBuf};

use sha2::{Digest as _, Sha256};

#[path = "support/smb_world_model.rs"]
mod smb;

use smb::{
    BODY_ROWS, MODE_PLAYING, STATE_NORMAL, TRIGGER_COLUMNS, boot_to_gameplay, is_blank_tile,
    is_load_bearing, player_surface_row_from_sprites, player_tile, read_state,
    reconstruct_background, run_right_buttons, scroll_x, scroll_y, snapshot, sprite_covered_cells,
    tile_at, tile_grid, wall_ahead,
};

/// The one ROM revision this address map was verified against. A different
/// image can move every address in the map, so the test refuses it outright
/// rather than misreading the game and reporting a confusing failure.
const SMB_SHA256: &str = "f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de";

/// The cartridge, or `None` when it is not installed. Absent hardware is a
/// skip, not a failure: the core builds and tests without any ROM checked in.
fn rom_image() -> Option<Vec<u8>> {
    let path = env::var_os("SPACEWARS_SMB_ROM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../data/roms/smb.nes"
            ))
        });
    match fs::read(&path) {
        Ok(rom) => Some(rom),
        Err(error) => {
            eprintln!(
                "skipping: no Super Mario Bros. cartridge at {} ({error}); \
                 set SPACEWARS_SMB_ROM to enable this test",
                path.display(),
            );
            None
        }
    }
}

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
fn policy_buttons(frames_into_gameplay: u64) -> engine_nes::ControllerButtons {
    let mut buttons = engine_nes::ControllerButtons::RIGHT;
    if frames_into_gameplay % 90 < 12 {
        buttons |= engine_nes::ControllerButtons::A;
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
    end_state: smb::SmbState,
    end_hash: engine_nes::StateHash,
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
            .run_frame([policy_buttons(elapsed), engine_nes::ControllerButtons::NONE])
            .expect("frame should run");
        let state = read_state(&machine);
        outcome.furthest_x = outcome.furthest_x.max(state.absolute_x);
        outcome.saw_airborne |= state.float_state == smb::FLOAT_JUMPING;
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
    let Some(rom) = rom_image() else { return };
    assert_pinned_revision(&rom);

    let run = run_script(&rom, 300);
    let state = run.end_state;

    // The setup script is DirtSim's: one Start press on frame 120.
    assert_eq!(
        run.gameplay_frame,
        smb::SETUP_START_FRAME,
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
    let Some(rom) = rom_image() else { return };
    assert_pinned_revision(&rom);

    let (mut machine, _) = boot_to_gameplay(&rom).expect("setup script should reach normal play");
    for elapsed in 1..=240 {
        machine
            .run_frame([policy_buttons(elapsed), engine_nes::ControllerButtons::NONE])
            .expect("frame should run");
    }

    // A planner explores by branching forward and rewinding, so the rewind has
    // to be bit-exact or every pruned branch is a lie.
    let root = machine.checkpoint();
    let root_hash = machine.state_hash();
    let actions = [
        engine_nes::ControllerButtons::NONE,
        engine_nes::ControllerButtons::RIGHT | engine_nes::ControllerButtons::B,
        engine_nes::ControllerButtons::RIGHT | engine_nes::ControllerButtons::A,
        engine_nes::ControllerButtons::A,
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
                .run_frame([action, engine_nes::ControllerButtons::NONE])
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
    let Some(rom) = rom_image() else { return };
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
                engine_nes::ControllerButtons::RIGHT | engine_nes::ControllerButtons::B,
                engine_nes::ControllerButtons::NONE,
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
    let frame = smb::snapshot(&machine);
    let Some(framebuffer) = machine.ppu().framebuffer() else {
        eprintln!("skipping the pixel comparison: video output is disabled");
        return;
    };

    let (column, row) = player_tile(&state, smb::scroll_x(&frame));
    let tiles = tile_grid(&frame, smb::scroll_x(&frame), smb::scroll_y(&frame));

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
                engine_nes::ControllerButtons::RIGHT | engine_nes::ControllerButtons::B,
                engine_nes::ControllerButtons::NONE,
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

/// The sideways direction, and the seventh decode trap. A tile the game holds him
/// up on and a tile the game stops him against are different observations, and
/// watching what carries him can only ever yield the first, so a load-bearing set
/// derived that way cannot name a wall. This pins it by *playing*: run the
/// reader-driven policy from `tests/support/smb_world_model.rs`, wait until the
/// world stops returning the input, and read the tile that did it. The game says
/// he is stopped; the reader says nothing solid is there.
///
/// The policy is the one the `world_model_bot` example runs, so this is the same
/// run the document reports rather than a fourth hand-written script. It reaches
/// the first pipe of 1-1 at absolute X 899 and stands there pushing right.
#[test]
fn the_tile_that_stops_the_player_is_not_one_the_reader_calls_solid() {
    let Some(rom) = rom_image() else { return };
    assert_pinned_revision(&rom);
    let (mut machine, _) = boot_to_gameplay(&rom).expect("setup script should reach normal play");

    // `$0770` says play before the game has handed him over. The level load holds
    // him at absolute X 0 in player state 0x00 for about 150 frames, and through
    // the first two of those the player bytes still hold the *previous* screen's
    // values, so a single frame of "normal, grounded, at X 40" is the ghost of a
    // level that is already gone. Wait for the state to hold instead of for it to
    // flicker: a policy started on the mode byte spends its first two seconds
    // pushing at a world that is not listening, and a frame-counted stall detector
    // then reads the bot as giving up before it has moved.
    let mut in_control_for = 0;
    for _ in 0..300 {
        let state = read_state(&machine);
        in_control_for = if state.is_in_control() && state.is_grounded() {
            in_control_for + 1
        } else {
            0
        };
        if in_control_for >= 8 {
            break;
        }
        machine
            .run_frame([
                engine_nes::ControllerButtons::NONE,
                engine_nes::ControllerButtons::NONE,
            ])
            .expect("frame should run");
    }
    assert!(
        in_control_for >= 8,
        "300 frames after the mode byte said play, the game still has not put the player in the \
         level, so the setup script or the level load has changed"
    );

    // Frames without new ground before "stopped" is a real observation. A jump, a
    // koopa walking into him and a ledge he reads as a hole each cost a few
    // frames, so a frozen frame or two proves nothing.
    const STALLED_FRAMES: u64 = 120;
    // The measured run reaches the pipe inside two thousand frames; the rest is
    // room for a core that decodes the same level a little differently.
    const FRAME_LIMIT: u64 = 3000;

    let mut furthest_x = 0;
    let mut no_progress_for = 0;
    let mut hold_a = 0;
    let mut stalled: Option<(u16, u8, bool)> = None;
    for _ in 1..=FRAME_LIMIT {
        let state = read_state(&machine);
        let frame = snapshot(&machine);
        let origin_x = scroll_x(&frame);
        let tiles = tile_grid(&frame, origin_x, 0);
        let (column, row) = player_tile(&state, origin_x);
        let buttons = run_right_buttons(&state, &machine, &tiles, column, row, &mut hold_a);
        if state.absolute_x > furthest_x {
            furthest_x = state.absolute_x;
            no_progress_for = 0;
        } else {
            no_progress_for += 1;
        }

        if no_progress_for >= STALLED_FRAMES && state.is_grounded() {
            // The same window the policy jumps on: the first cell ahead that draws
            // anything at all in the rows his body occupies.
            let ahead = (1..=TRIGGER_COLUMNS)
                .flat_map(|step| (1..=BODY_ROWS).map(move |rise| (column + step, row - rise)))
                .filter_map(|(column, row)| tile_at(&tiles, column, row))
                .find(|tile| !is_blank_tile(&frame, *tile));
            if let Some(tile) = ahead {
                stalled = Some((furthest_x, tile, wall_ahead(&tiles, column, row)));
                break;
            }
        }

        machine
            .run_frame([buttons, engine_nes::ControllerButtons::NONE])
            .expect("frame should run");
    }

    let (x, tile, wall) = stalled.unwrap_or_else(|| {
        panic!(
            "{FRAME_LIMIT} frames of the reader-driven policy never stalled: furthest absolute \
             X {furthest_x}, so either the policy got better or the level changed shape, and \
             trap 7 of docs/nes-world-model-reader.md needs re-measuring"
        )
    });
    assert!(
        x > 500,
        "the policy stalled at absolute X {x}, well short of the first pipe: the jump rule has \
         stopped clearing the early pits, so this is no longer measuring a wall",
    );
    assert!(
        !is_load_bearing(tile),
        "the game stopped him at absolute X {x} on tile ${tile:02x}, which the reader calls \
         load-bearing: the set has started to describe walls, so re-derive it and rewrite trap 7 \
         in docs/nes-world-model-reader.md",
    );
    assert!(
        !wall,
        "`wall_ahead` called the cell at absolute X {x} solid while the load-bearing set called \
         the same tile scenery: the reader contradicts itself",
    );
}
