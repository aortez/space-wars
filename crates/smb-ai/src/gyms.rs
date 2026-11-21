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

/// The run-up the pit gym starts from: the base policy from power-on to frame 1040,
/// which is absolute X 1226 at full running speed, mid-jump, upstream of the pit.
///
/// He is in the air on that frame rather than on the ground — the base policy jumps on
/// its own way — and comes down on frame 1060 at absolute X 1276, a hundred pixels of
/// level short of the lip he walks off on frame 1100.
///
/// Recorded, not authored — `gym --record 1040 pit-1379` cut it and replayed it
/// before printing it. It is the same run the camera-limit tape is running through,
/// stopped 127 frames earlier, and it is committed separately rather than derived
/// because a tape is the bytes that produced a verdict, not an expression.
const PIT_1379_TAPE: Tape = Tape {
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
        (0x83, 6),   // A|B|RIGHT
    ]),
    frame: 1040,
    golden_hash: engine_nes::StateHash {
        version: 3,
        value: 0x2197_cfcb_e64d_892a,
    },
};

/// The same stop cut upstream of the fall, so that a skill has something to do about
/// it.
///
/// `camera-limit` is cut at frame 1167, which measurement has since put *inside* the
/// fall: he leaves the ground on frame 1100 and that tape starts 67 frames later, so
/// the frame the jump had to start on is already behind the start state and no policy
/// can be measured against it — its verdict can only ever be `Stuck`, however good the
/// bot gets. This tape stops 153 pixels short of the pit's near edge, which makes it
/// the first start state here a bot can win from rather than only fail on.
///
/// The objective stays at 1600 and now discriminates rather than merely sits past the
/// stop: a pit fall ends with absolute X clamped at 1549 by the screen edge, so
/// reaching 1600 means he crossed rather than fell.
///
/// Measured, the base policy still loses from here — `Stuck` after 248 frames at
/// absolute X 1544 — so the gym is an unsolved problem and not a formality. What it
/// takes is known: a jump started between 28 and 58 frames in.
pub const PIT_1379: Gym = Gym {
    name: "pit-1379",
    seed: LedgerSeed::Cold,
    tape: PIT_1379_TAPE,
    recorded: Verdict {
        outcome: Outcome::Stuck,
        frames: 248,
        furthest_x: 1544,
        refusals: 0,
    },
    ..CAMERA_LIMIT
};
