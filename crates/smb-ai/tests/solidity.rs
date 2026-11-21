//! What the model believes about solidity, tested by playing.
//!
//! These are the two claims Phase 1 of `docs/smb-execution-plan.md` was built on,
//! and they are behaviour rather than decoding: each one plays a run out and argues
//! with what the game did. They live here rather than with the reader because a
//! claim about a wall is a claim about what a bot may do, and the reader's own tests
//! in `smb-model` have no business knowing what a bot is.
//!
//! Both run the `BaseBot` the `bot` example runs, so a document, a test and a film
//! cannot each mean a different policy. Like the reader's test, these skip when no
//! cartridge is installed.

use engine_nes::{ControllerButtons, MachineCheckpoint, NesMachine};
use sha2::{Digest as _, Sha256};
use smb_ai::policy::BaseBot;
use smb_model::affordances::TRIGGER_COLUMNS;
use smb_model::cartridge;
use smb_model::frame::{
    is_blank_tile, is_load_bearing, player_tile, scroll_x, snapshot, tile_at, tile_grid,
};
use smb_model::profile::{boot_to_gameplay, read_state, wait_for_handover};
use smb_model::resistance::{BODY_ROWS, Refusal, Resistance};

/// The one ROM revision the model was verified against, as the SHA-256 a person can
/// check against a file.
///
/// `cartridge::SMB_IDENTITY` is what stops a wrong image being misread; this says the
/// number documents quote still names this file.
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

/// The sideways direction, and the seventh decode trap, in the form a model with no
/// memory can be caught in. A tile the game holds him up on and a tile the game
/// stops him against are different observations, and watching what carries him can
/// only ever yield the first, so a load-bearing set derived that way cannot name a
/// wall. This pins it by *playing*: run the reader-driven policy in `policy`
/// with nothing remembered yet, wait until the
/// world stops returning the input, and read the tile that did it. The game says he
/// is stopped; the reader says nothing solid is there.
///
/// The test below is the other half, where the same cell does become a wall.
///
/// The policy is the `BaseBot` the `bot` example runs, with its look-ahead pinned to
/// zero, so this is the same run the document reports rather than a fourth
/// hand-written script. It reaches the first pipe of 1-1 at absolute X 899 and stands
/// there pushing right.
#[test]
fn the_tile_that_stops_the_player_is_not_one_the_reader_calls_solid() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
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
    // Nothing has been refused yet on this pass, which is the point: the ledger this
    // test argues with is the empty one, and it is not allowed to vouch for a cell
    // further than the one that actually refused him.
    let mut bot = BaseBot::with_look_columns(Resistance::default(), 0);
    let mut stalled: Option<(u16, u8, bool)> = None;
    for _ in 1..=FRAME_LIMIT {
        let state = read_state(&machine);
        let frame = snapshot(&machine);
        let origin_x = scroll_x(&frame);
        let tiles = tile_grid(&frame, origin_x, 0);
        let (column, row) = player_tile(&state, origin_x);
        let buttons = bot.buttons(&machine);
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
                stalled = Some((furthest_x, tile, bot.saw_wall()));
                break;
            }
        }

        machine
            .run_frame([buttons, ControllerButtons::NONE])
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

fn play(machine: &mut NesMachine, bot: &mut BaseBot, plan: &Plan) -> Pass {
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
        let buttons = bot.buttons(machine);
        if bot.ledger_named_wall() && pass.ledger_wall.is_none() {
            pass.ledger_wall = Some((frame_id, state.absolute_x));
        }
        machine
            .run_frame([buttons, ControllerButtons::NONE])
            .expect("frame should run");
        let produced = read_state(machine);
        if let Some(refusal) = bot.observe(&produced, machine, frame_id) {
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
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
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
    let mut learner = BaseBot::default();
    let mut machine = handed_over(&rom);
    let discovery = play(
        &mut machine,
        &mut learner,
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
        !learner.ledger.is_empty(),
        "the game refused him at absolute X {refused_x} and the ledger still says nothing, so          the refusal was not attributed to a cell",
    );

    // Pass 2: a cold run to a checkpoint past the refusal. Cold, so the checkpoint
    // is where the same policy would be if it had never been told anything.
    let mut machine = handed_over(&rom);
    let mut survey_bot = BaseBot::default();
    let survey = play(
        &mut machine,
        &mut survey_bot,
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
    let mut amnesiac_bot = BaseBot::default();
    let amnesiac = play(
        &mut machine,
        &mut amnesiac_bot,
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
    let mut remembering_bot = BaseBot::new(Resistance::from_log(learner.ledger.log()));
    let remembered = play(
        &mut machine,
        &mut remembering_bot,
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
