//! Phase 1: does the game's own refusal say what a wall is?
//!
//! The reader can say what is drawn and what has held the player up, and neither
//! answers what stops him sideways: a pipe's top row is in the load-bearing set and
//! its body is not, and the body is exactly what stops him. This example tests the
//! claim written down in `docs/smb-execution-plan.md`, that the difference between
//! the motion the buttons asked for and the motion the core produced is a verdict
//! that can be attributed to a cell, with no tuning constant and no disassembly.
//!
//! Four passes, because the honest version of "the model saw it coming" needs a
//! before and an after that differ in nothing but memory.
//!
//! 1. **Discovery.** Run the base policy with an empty ledger over the opening and
//!    keep every refusal. That is the model learning what a wall is, the way a player
//!    learns one by walking into it. The first refusal is not the one to test on: the
//!    claim is about recognizing an artwork, so the level has to draw it twice, and
//!    `recurring_refusal` picks the first pattern that comes back. In 1-1 the first
//!    two refusals are airborne block bumps whose artwork never returns; the pipe body
//!    at absolute X 594 is the first lesson the level teaches twice.
//! 2. **Survey.** Run the same frames again from the handover with an empty ledger,
//!    taking a checkpoint `RUNUP_FRAMES` *before* the second meeting with it.
//! 3. **Amnesiac.** Restore that checkpoint and run the tail with an empty ledger.
//! 4. **Remembering.** Restore the same checkpoint and run the same tail holding what
//!    pass 1 was refused on that one frame.
//!
//! Passes 3 and 4 start from the same machine state bit for bit, run the same policy,
//! and can only differ through what they remember. The memory they differ on is of
//! something that happened before the checkpoint, so it cannot be a reaction to what
//! is in front of him: it is the generalization, which is the whole claim. The
//! obstacle that tests it is still ahead of both of them, which is why the checkpoint
//! sits before the second meeting rather than after the first.
//!
//! Pass 2 is there to keep pass 3 honest. A model that learns as it goes picks the
//! refusal up again mid-run, so a cold run is not a model that cannot know, only one
//! that has not yet been told.
//!
//! `--signature` prints the raw motion of every frame the ledger's own condition calls a
//! refusal, next to the cells that motion was refused by, so the census and the memory are
//! the same measurement and cannot drift apart. Read it before changing either.

use std::{env, error::Error, path::PathBuf};

use smb_ai::policy::{recurring_refusal, run_right_buttons};
use smb_model::affordances::{WALL_LOOK_COLUMNS, wall_within_reach};
use smb_model::frame::{
    PpuBytes, pattern_bytes, pattern_hash, player_tile, scroll_x, snapshot, tile_grid,
};
use smb_model::profile::{boot_to_gameplay, read_state, wait_for_handover};
use smb_model::resistance::{
    Motion, Refusal, Resistance, Side, cell_beyond, leading_pixel, refusal_signature,
};

use engine_nes::{ControllerButtons, MachineCheckpoint, NesMachine};

/// The least distance allowed between the two meetings with one artwork. The tails
/// start RUNUP_FRAMES before the second one, so this is what keeps the lesson in their
/// past rather than under them.
const LESSON_GAP: u64 = 180;

/// How far before the second meeting the two memory passes start. Short enough that
/// the amnesiac still arrives at the artwork with no run-up left, which is where
/// memory is worth the most, and long enough that the remembering pass has frames to
/// act on what it knows. Short enough, too, that the tail is the same level.
const RUNUP_FRAMES: u64 = 60;

/// What one pass is worth reporting.
struct Pass {
    frames_run: u64,
    furthest_x: u16,
    a_frames: u32,
    refusals: Vec<Refusal>,
    /// The first frame the model called a cell a wall, from either source.
    first_wall: Option<(u64, u16)>,
    /// The first frame where the ledger called a cell a wall and the load-bearing
    /// set did not. This is the only number in here that measures the ledger itself.
    first_ledger_wall: Option<(u64, u16)>,
    first_jump: Option<(u64, u16)>,
    ended_on_level_change: bool,
    checkpoint: Option<(u64, MachineCheckpoint)>,
}

/// What a pass should do beyond running the policy.
struct Plan {
    signature: bool,
    report: bool,
    /// Take a checkpoint on this frame id and keep it in the pass.
    checkpoint_at: Option<u64>,
}

/// One pass of the base policy, recording refusals into `ledger` as it goes.
///
/// The ledger is passed in rather than made here because the whole point is a pass
/// that starts with another pass's memory.
fn run(
    machine: &mut NesMachine,
    frames: u64,
    ledger: &mut Resistance,
    plan: &Plan,
) -> Result<Pass, Box<dyn Error>> {
    let start = {
        let state = read_state(machine);
        (state.world, state.level)
    };
    let mut pass = Pass {
        frames_run: 0,
        furthest_x: read_state(machine).absolute_x,
        a_frames: 0,
        refusals: Vec::new(),
        first_wall: None,
        first_ledger_wall: None,
        first_jump: None,
        ended_on_level_change: false,
        checkpoint: None,
    };
    let mut hold_a = 0u64;
    let mut a_down = false;
    for _ in 0..frames {
        let frame_id = machine.ppu().frame_id();
        let state = read_state(machine);
        if (state.world, state.level) != start {
            pass.ended_on_level_change = true;
            if plan.report {
                println!("  frame {frame_id}: left level {}.{}", start.0, start.1);
            }
            break;
        }
        if plan.checkpoint_at == Some(frame_id) {
            pass.checkpoint = Some((frame_id, machine.checkpoint()));
            if plan.report {
                println!(
                    "  frame {frame_id}: checkpointed at absolute X {}",
                    state.absolute_x
                );
            }
        }
        let frame: PpuBytes = snapshot(machine);
        let origin_x = scroll_x(&frame);
        let tiles = tile_grid(&frame, origin_x, 0);
        let (column, row) = player_tile(&state, origin_x);
        let wall = wall_within_reach(&frame, origin_x, &state, ledger, WALL_LOOK_COLUMNS);
        if wall && pass.first_wall.is_none() {
            pass.first_wall = Some((frame_id, state.absolute_x));
            if plan.report {
                println!(
                    "  frame {frame_id}: the model calls a cell ahead a wall at absolute X {}",
                    state.absolute_x
                );
            }
        }
        // The same question with the ledger wiped, so a yes here is a fact the game
        // taught this model and nothing the tile ids already carry.
        if wall
            && pass.first_ledger_wall.is_none()
            && !wall_within_reach(
                &frame,
                origin_x,
                &state,
                &Resistance::default(),
                WALL_LOOK_COLUMNS,
            )
        {
            pass.first_ledger_wall = Some((frame_id, state.absolute_x));
            if plan.report {
                println!(
                    "  frame {frame_id}: the ledger alone called a cell a wall at absolute X {}, \
                     which the tile ids do not",
                    state.absolute_x
                );
            }
        }
        let buttons = run_right_buttons(&state, machine, &tiles, column, row, wall, &mut hold_a);
        let jumping = buttons.contains(ControllerButtons::A) && !a_down;
        a_down = buttons.contains(ControllerButtons::A);
        if jumping && pass.first_jump.is_none() {
            pass.first_jump = Some((frame_id, state.absolute_x));
            if plan.report {
                println!(
                    "  frame {frame_id}: jumped at absolute X {} (wall visible: {wall})",
                    state.absolute_x
                );
            }
        }
        let asked = Motion::asked(&state, buttons);
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        let produced = read_state(machine);
        let after: PpuBytes = snapshot(machine);
        let after_origin = scroll_x(&after);
        if plan.signature && refusal_signature(asked, &produced, Side::Right) {
            // The raw motion the ledger's condition is built from, next to the cells
            // that motion was refused by. Each cell is named by the world pixel its
            // left edge falls on, and read through the exact channel on purpose: the
            // decoded grid is indexed by screen column and the level is laid out in
            // world pixels, and those only line up when the fine scroll is zero. A
            // `!` marks a cell the ledger already says stops him.
            let (own, body) = player_tile(&produced, after_origin);
            let strip = |rise: i16| -> String {
                (0..=3)
                    .map(|step| {
                        cell_beyond(&after, after_origin, &produced, Side::Right, step, rise)
                            .map_or_else(
                                || "      off     ".to_owned(),
                                |tile| {
                                    let known = ledger.refused(
                                        pattern_hash(pattern_bytes(&after, tile)),
                                        -rise,
                                        Side::Right,
                                    );
                                    let edge = i32::from(leading_pixel(&produced, Side::Right))
                                        + i32::from(step) * 8;
                                    format!(
                                        " w{:>4}=${tile:02x}{}",
                                        edge / 8 * 8,
                                        if known { "!" } else { " " }
                                    )
                                },
                            )
                    })
                    .collect()
            };
            let (feet_row, head_row) = (strip(1), strip(2));
            println!(
                "  frame {frame_id}: pinned at X {} speed {:+} -> {:+}, body reaches pixel {}, \
                 own column {own:>3}\n      r{body_row1} {feet_row}\n      \
                 r{body_row2} {head_row}",
                asked.x,
                asked.h_speed,
                produced.h_speed,
                leading_pixel(&produced, Side::Right),
                body_row1 = body - 1,
                body_row2 = body - 2,
            );
        }
        if let Some(refusal) = ledger.observe(asked, &produced, &after, after_origin, frame_id) {
            pass.refusals.push(refusal);
            if plan.report {
                println!(
                    "  frame {}: the game refused {} at row {:+}, drawn ${:02x}, pattern {:016x}, at absolute X {}",
                    refusal.frame,
                    match refusal.side {
                        Side::Right => "rightward motion",
                        Side::Left => "leftward motion",
                    },
                    refusal.row_offset,
                    refusal.tile,
                    refusal.pattern,
                    refusal.absolute_x,
                );
            }
        }
        if a_down {
            pass.a_frames += 1;
        }
        pass.furthest_x = pass.furthest_x.max(produced.absolute_x);
        pass.frames_run += 1;
    }
    if plan.report {
        println!(
            "  furthest absolute X {}, {} frames with A down, {} refusals",
            pass.furthest_x,
            pass.a_frames,
            pass.refusals.len()
        );
    }
    Ok(pass)
}

/// Where a moment happened, in the words the report needs.
fn at(when: Option<(u64, u16)>) -> String {
    when.map(|(frame, x)| format!("frame {frame} at absolute X {x}"))
        .unwrap_or_else(|| "never".to_owned())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let chosen = args.next().map(PathBuf::from);
    let mut signature = false;
    let mut frames = None;
    for arg in args {
        if arg == "--signature" {
            signature = true;
        } else {
            frames = Some(arg.parse()?);
        }
    }
    let frames: u64 = frames.unwrap_or(1200);
    let Some(rom) = smb_model::cartridge::read_from(chosen)? else {
        return Ok(());
    };
    let (mut machine, gameplay_frame) = boot_to_gameplay(&rom)?;
    let handed_over = wait_for_handover(&mut machine)?;
    println!("frame {gameplay_frame}: gameplay, frame {handed_over}: handed over");
    let root = machine.checkpoint();
    let root_hash = machine.state_hash();

    println!("\npass 1, discovery: an empty ledger over the opening, keeping every refusal");
    machine.restore(&root)?;
    let discovery = run(
        &mut machine,
        frames,
        &mut Resistance::default(),
        &Plan {
            signature,
            report: true,
            checkpoint_at: None,
        },
    )?;
    let Some((taught, met)) = recurring_refusal(&discovery.refusals, LESSON_GAP) else {
        println!(
            "  {} refusals and none of them an artwork the level draws twice again, so there \
             is nothing to predict",
            discovery.refusals.len(),
        );
        return Ok(());
    };
    let refused_frame = taught.frame;
    let refused_x = taught.absolute_x;
    // Everything the game refused on that one frame, which is the whole of what a body
    // learns from being stopped once.
    let memory: Vec<Refusal> = discovery
        .refusals
        .iter()
        .copied()
        .filter(|r| r.frame == refused_frame)
        .collect();
    let known = Resistance::from_log(&memory);

    let checkpoint_at = met.frame - RUNUP_FRAMES;
    println!(
        "\npass 2, survey: an empty ledger from the handover, checkpoint at {checkpoint_at}, \
         {} frames after the lesson on frame {refused_frame} and {RUNUP_FRAMES} before the same \
         artwork at absolute X {}",
        checkpoint_at - refused_frame,
        met.absolute_x,
    );
    machine.restore(&root)?;
    assert_eq!(machine.state_hash(), root_hash, "restore must be exact");
    let survey = run(
        &mut machine,
        frames,
        &mut Resistance::default(),
        &Plan {
            signature,
            report: true,
            checkpoint_at: Some(checkpoint_at),
        },
    )?;
    let Some((checkpoint_frame, start)) = survey.checkpoint else {
        return Err("the survey pass never reached the checkpoint frame".into());
    };
    let tail = survey.frames_run - (checkpoint_frame - handed_over);

    println!(
        "\npass 3, amnesiac: the same {tail} frames from the frame {checkpoint_frame} checkpoint, \
         remembering nothing"
    );
    machine.restore(&start)?;
    let amnesiac = run(
        &mut machine,
        tail,
        &mut Resistance::default(),
        &Plan {
            signature: false,
            report: true,
            checkpoint_at: None,
        },
    )?;

    println!(
        "\npass 4, remembering: the same {tail} frames from the same checkpoint, holding {} \
         refused cell(s) from pass 1",
        known.len()
    );
    machine.restore(&start)?;
    let remembering = run(
        &mut machine,
        tail,
        &mut known.clone(),
        &Plan {
            signature: false,
            report: true,
            checkpoint_at: None,
        },
    )?;

    println!(
        "\nThe game refused him the artwork drawn ${:02x} on frame {refused_frame} at absolute \
         X {refused_x}, and drew it again on frame {} at absolute X {}.\n\
         Passes 3 and 4 start from the same frame {checkpoint_frame} state and differ only in \
         that memory.\n\
         Amnesiac: first wall the ledger knows but the tile ids do not: {}.\n\
         Remembering: {}.\n\
         Amnesiac furthest absolute X {}, {} refusals. \
         Remembering furthest absolute X {}, {} refusals.",
        taught.tile,
        met.frame,
        met.absolute_x,
        at(amnesiac.first_ledger_wall),
        at(remembering.first_ledger_wall),
        amnesiac.furthest_x,
        amnesiac.refusals.len(),
        remembering.furthest_x,
        remembering.refusals.len(),
    );
    Ok(())
}
