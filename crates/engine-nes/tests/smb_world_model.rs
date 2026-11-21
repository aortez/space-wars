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

use engine_nes::{MachineCheckpoint, NesMachine};

use sha2::{Digest as _, Sha256};

#[path = "support/smb_world_model.rs"]
mod smb;

use smb::{
    BODY_ROWS, MODE_PLAYING, Motion, Refusal, Resistance, STATE_NORMAL, TRIGGER_COLUMNS,
    WALL_LOOK_COLUMNS, boot_to_gameplay, is_blank_tile, is_load_bearing,
    player_surface_row_from_sprites, player_tile, read_state, reconstruct_background,
    run_right_buttons, scroll_x, scroll_y, snapshot, sprite_covered_cells, tile_at, tile_grid,
    wait_for_handover, wall_ahead,
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

/// The sideways direction, and the seventh decode trap, in the form a model with no
/// memory can be caught in. A tile the game holds him up on and a tile the game
/// stops him against are different observations, and watching what carries him can
/// only ever yield the first, so a load-bearing set derived that way cannot name a
/// wall. This pins it by *playing*: run the reader-driven policy from
/// `tests/support/smb_world_model.rs` with nothing remembered yet, wait until the
/// world stops returning the input, and read the tile that did it. The game says he
/// is stopped; the reader says nothing solid is there.
///
/// The test below is the other half, where the same cell does become a wall.
///
/// The policy is the one the `world_model_bot` example runs, so this is the same
/// run the document reports rather than a fourth hand-written script. It reaches
/// the first pipe of 1-1 at absolute X 899 and stands there pushing right.
#[test]
fn the_tile_that_stops_the_player_is_not_one_the_reader_calls_solid() {
    let Some(rom) = rom_image() else { return };
    assert_pinned_revision(&rom);
    let (mut machine, _) = boot_to_gameplay(&rom).expect("setup script should reach normal play");

    wait_for_handover(&mut machine).expect("the game should hand the player over");

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
        // Nothing has been refused yet on this pass, which is the point: the
        // ledger this test argues with is the empty one.
        let wall = wall_ahead(&frame, origin_x, &state, &Resistance::default(), 0);
        let buttons = run_right_buttons(&state, &machine, &tiles, column, row, wall, &mut hold_a);
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
                stalled = Some((furthest_x, tile, wall));
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

/// One pass of the base policy, for the memory A/B below.
///
/// The ledger is passed in rather than made here because the point is a pass that
/// starts out holding another pass's memory, and it is written to as the pass runs
/// because a model that learns as it goes is the only honest amnesiac: a run that
/// cannot learn is not a run that does not yet know.
struct Pass {
    /// Every refusal the game made during the pass, in the order it made them.
    refusals: Vec<Refusal>,
    /// The first frame the ledger called a cell a wall and the tile ids did not.
    /// The only number here that measures the ledger rather than the level.
    ledger_wall: Option<(u64, u16)>,
    /// The pass ran off the end of the level it started on, so anything past that
    /// point is a different game's vocabulary.
    left_level: bool,
    /// The machine state as it stood on `Plan::checkpoint_at`, if that frame came.
    checkpoint: Option<(u64, MachineCheckpoint)>,
}

/// How a pass should end and what it should remember on the way.
struct Plan {
    /// A ceiling, not a target: a pass also ends on its checkpoint or its refusal.
    frames: u64,
    /// End as soon as the game refuses him a move, so that refusal can be the thing
    /// a later pass is handed.
    stop_at_first_refusal: bool,
    /// End on this frame id with the machine state as it stood there.
    checkpoint_at: Option<u64>,
}

fn play(machine: &mut NesMachine, ledger: &mut Resistance, plan: &Plan) -> Pass {
    let start = {
        let state = read_state(machine);
        (state.world, state.level)
    };
    let mut pass = Pass {
        refusals: Vec::new(),
        ledger_wall: None,
        left_level: false,
        checkpoint: None,
    };
    let mut hold_a = 0u64;
    for _ in 0..plan.frames {
        let frame_id = machine.ppu().frame_id();
        let state = read_state(machine);
        if (state.world, state.level) != start {
            pass.left_level = true;
            break;
        }
        if plan.checkpoint_at == Some(frame_id) {
            pass.checkpoint = Some((frame_id, machine.checkpoint()));
            break;
        }
        let frame = snapshot(machine);
        let origin_x = scroll_x(&frame);
        let tiles = tile_grid(&frame, origin_x, 0);
        let (column, row) = player_tile(&state, origin_x);
        let wall = wall_ahead(&frame, origin_x, &state, ledger, WALL_LOOK_COLUMNS);
        if wall
            && pass.ledger_wall.is_none()
            // The same question with the ledger wiped, so a yes here is something
            // this model was taught and not something the tile ids already carry.
            && !wall_ahead(
                &frame,
                origin_x,
                &state,
                &Resistance::default(),
                WALL_LOOK_COLUMNS,
            )
        {
            pass.ledger_wall = Some((frame_id, state.absolute_x));
        }
        let buttons = run_right_buttons(&state, machine, &tiles, column, row, wall, &mut hold_a);
        let asked = Motion::asked(&state, buttons);
        machine
            .run_frame([buttons, engine_nes::ControllerButtons::NONE])
            .expect("frame should run");
        let produced = read_state(machine);
        let after = snapshot(machine);
        if let Some(refusal) = ledger.observe(asked, &produced, &after, scroll_x(&after), frame_id)
        {
            pass.refusals.push(refusal);
            if plan.stop_at_first_refusal {
                break;
            }
        }
    }
    pass
}

/// Boot a cartridge and run it to the frame the game lets go of the player.
fn handed_over(rom: &[u8]) -> NesMachine {
    let (mut machine, _) = boot_to_gameplay(rom).expect("setup script should reach normal play");
    wait_for_handover(&mut machine).expect("the game should hand the player over");
    machine
}

/// Phase 1's claim, end to end: memory of a refusal is what turns scenery into a
/// wall.
///
/// The test above is the cold reader's half of the problem. Nothing in the picture
/// separates a pipe body from a cloud and no larger vocabulary will, because "stops
/// him sideways" is not a property of the picture at all: it is something the game
/// says only by refusing to move him. So the answer is a memory rather than a
/// decoder. The buttons asked for a move, the core handed back no move, and the cell
/// at the body's leading edge is what said no.
///
/// The claim needs an A/B whose only variable is that memory. Learn one refusal from
/// the first pipe, then take a checkpoint from a cold run some seconds *after* it,
/// and run the tail twice from that one state. Neither machine contains the refusal
/// it starts with: for one of them the refusal is only in its past. The amnesiac tail
/// has to be stopped by the next pipe before it can call it a wall; the remembering
/// tail has to call the same artwork a wall before it arrives, and never be stopped
/// at all. And because the wall it names is a different pipe further along, what it
/// recognized is the artwork, not the place where it happened to be standing.
#[test]
fn remembering_a_refusal_names_a_wall_the_tile_ids_do_not() {
    let Some(rom) = rom_image() else { return };
    assert_pinned_revision(&rom);

    // Frames between the refusal and the checkpoint both tails start from. Far
    // enough back that the pipe which refused him is behind him and the next one is
    // still ahead; close enough that the tail is the same level.
    const RUNUP_FRAMES: u64 = 120;
    // Room to reach the next pipe from that checkpoint. The measured events land
    // about a hundred frames in, so the rest is room for a core that decodes the
    // same level a little differently, and a pass ends on the level change anyway.
    const TAIL_FRAMES: u64 = 300;
    const LEARN_LIMIT: u64 = 3000;

    // Pass 1: learn. The first thing 1-1 refuses him is the first pipe.
    let mut learned = Resistance::default();
    let mut machine = handed_over(&rom);
    let discovery = play(
        &mut machine,
        &mut learned,
        &Plan {
            frames: LEARN_LIMIT,
            stop_at_first_refusal: true,
            checkpoint_at: None,
        },
    );
    assert!(
        !discovery.left_level,
        "the model left 1-1 before the game refused it anything"
    );
    let refusal = discovery
        .refusals
        .first()
        .expect("the game never refused the model a move, so there is nothing to remember");
    let (refused_x, refused_frame) = (refusal.absolute_x, refusal.frame);
    assert!(
        refused_x > 300,
        "the first refusal was at absolute X {refused_x}, which is the level load rather than          an obstacle",
    );
    assert!(
        !learned.is_empty(),
        "the game refused him at absolute X {refused_x} and the ledger still says nothing, so          the refusal was not attributed to a cell",
    );

    // Pass 2: a cold run to a checkpoint past the refusal. Cold, so the checkpoint
    // is where the same policy would be if it had never been told anything.
    let mut machine = handed_over(&rom);
    let mut cold = Resistance::default();
    let survey = play(
        &mut machine,
        &mut cold,
        &Plan {
            frames: refused_frame + 2 * RUNUP_FRAMES + LEARN_LIMIT,
            stop_at_first_refusal: false,
            checkpoint_at: Some(refused_frame + RUNUP_FRAMES),
        },
    );
    let (root_frame, root) = survey
        .checkpoint
        .expect("the cold run never reached the checkpoint frame");
    assert!(!survey.left_level, "the checkpoint is not in 1-1 any more");
    // The survey pass stops on the checkpoint frame, so the live state is the root.
    let root_hash = machine.state_hash();

    // Tail A: the same state, nothing remembered. A ledger of its own, wiped: the
    // survey pass learned as it went, and a tail that inherits that would be
    // remembering the very refusal this side of the A/B is not supposed to have.
    machine
        .restore(&root)
        .expect("the amnesiac tail should start from the checkpoint");
    assert_eq!(
        machine.state_hash(),
        root_hash,
        "restore has to be exact or the two tails are not the same start",
    );
    let mut amnesiac_ledger = Resistance::default();
    let amnesiac = play(
        &mut machine,
        &mut amnesiac_ledger,
        &Plan {
            frames: TAIL_FRAMES,
            stop_at_first_refusal: false,
            checkpoint_at: None,
        },
    );

    // Tail B: the same state, bit for bit, holding pass 1's refusals.
    machine
        .restore(&root)
        .expect("the remembering tail should start from the checkpoint");
    assert_eq!(
        machine.state_hash(),
        root_hash,
        "and from the same bits as the amnesiac tail, at frame {root_frame}",
    );
    let mut remembering_ledger = Resistance::from_log(learned.log());
    let remembered = play(
        &mut machine,
        &mut remembering_ledger,
        &Plan {
            frames: TAIL_FRAMES,
            stop_at_first_refusal: false,
            checkpoint_at: None,
        },
    );

    assert!(
        !amnesiac.left_level && !remembered.left_level,
        "a tail left 1-1, so the two are no longer comparing the same obstacle",
    );

    // The amnesiac tail has to meet the wall for real, or its failure to see it
    // coming would only mean the tail contains no wall.
    let stopped = amnesiac
        .refusals
        .first()
        .expect("the cold tail was never refused anything, so it proves nothing about memory");
    assert!(
        amnesiac
            .ledger_wall
            .is_none_or(|(frame, _)| frame >= stopped.frame),
        "the cold model called a cell a wall at frame {}, before the game had refused it          anything at frame {}",
        amnesiac.ledger_wall.map_or(0, |(frame, _)| frame),
        stopped.frame,
    );

    let (frame, x) = remembered
        .ledger_wall
        .expect("the ledger never named a wall, so memory did not reach the next pipe");
    assert!(
        remembered.refusals.is_empty(),
        "the remembering model was refused at absolute X {} on frame {} anyway, so it did not          act on what it remembered",
        remembered.refusals[0].absolute_x,
        remembered.refusals[0].frame,
    );
    assert!(
        frame < stopped.frame && x < stopped.absolute_x,
        "the remembering model saw the wall at frame {frame} and absolute X {x}, which is not          ahead of the cold model's collision at frame {} and absolute X {}",
        stopped.frame,
        stopped.absolute_x,
    );
    assert!(
        x > refused_x,
        "the wall it named was at absolute X {x}, at or behind the pipe that taught it at          {refused_x}: that repeats a position instead of recognizing the artwork drawn there",
    );
}
