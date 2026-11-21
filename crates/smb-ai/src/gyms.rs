//! The gyms this project has needed, as opposed to the ones it could have written.
//!
//! Every gym here came out of a run that stopped somewhere and could not say why.
//! That is the only source worth committing: a gym invented from imagination is a
//! situation the model may never meet, and Phase 6 of `docs/smb-execution-plan.md`
//! defers invented levels for exactly that reason.
//!
//! A gym is a `const`, its tape is committed as button runs, and `smb-ai`'s `gym`
//! example re-measures every verdict here against the one the gym carries.

use std::borrow::Cow;

use crate::gym::{Gym, LedgerSeed, Objective, Outcome, Tape, Verdict};

/// The run-up both camera-limit gyms start from: the base policy from power-on to
/// frame 1167, which is the frame absolute X has just stopped moving at 1544 while
/// the game still holds the controller.
///
/// Recorded, not authored — `gym --record 1167` cut it and replayed it before
/// printing it. The three setup runs at the front are DirtSim's Start press at
/// frame 120 and the level load after it; the policy's own first frame is 288.
const CAMERA_LIMIT_TAPE: Tape = Tape {
    cartridge: smb_model::cartridge::SMB_IDENTITY,
    runs: Cow::Borrowed(&[
        (0x00, 119), // NONE
        (0x08, 1),   // START
        (0x00, 167), // NONE: the level is still being placed
        (0x82, 119), // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 10),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 23),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 3),   // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 33),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 68),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 12),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 35),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 9),   // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 74),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 10),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 73),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 38),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 1),   // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 92),  // B|RIGHT
    ]),
    frame: 1167,
    golden_hash: engine_nes::StateHash {
        version: 3,
        value: 0xa73c_ebb1_ed21_4d50,
    },
};

/// The stop that ends every run of 1-1 today, and the first question the harness was
/// built to ask. Absolute X holds at 1544 while the camera is locked, he is falling
/// rather than blocked, no artwork is involved, and the model has nothing to say.
///
/// The objective is 16 pixels past where he stops, so the verdict is about the stop
/// and not about a distance the bot was already at.
pub const CAMERA_LIMIT: Gym = Gym {
    name: "camera-limit",
    objective: Objective::ReachX { at_least: 1600 },
    seed: LedgerSeed::Cold,
    tape: CAMERA_LIMIT_TAPE,
    frame_budget: 600,
    stall_frames: 120,
    recorded: Verdict {
        outcome: Outcome::Stuck,
        frames: 120,
        furthest_x: 1544,
        refusals: 0,
    },
};

/// The same frame, the same buttons and the same budget, with the refusals the
/// run-up already earned handed to the model instead of wiped.
///
/// This is the pair the tape exists to make possible: two gyms whose only difference
/// is what the model remembers, run from bits that are identical to each other. What
/// it measures here is that the difference is nothing, because the thing at the
/// camera limit never refused him and so was never available to be remembered.
pub const CAMERA_LIMIT_REMEMBERING: Gym = Gym {
    name: "camera-limit-remembering",
    seed: LedgerSeed::Warm,
    recorded: Verdict {
        outcome: Outcome::Stuck,
        frames: 120,
        furthest_x: 1544,
        refusals: 0,
    },
    ..CAMERA_LIMIT
};
