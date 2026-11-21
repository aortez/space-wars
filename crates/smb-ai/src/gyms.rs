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

use crate::{
    gym::{self, Gym, LedgerSeed, Objective, Outcome, Tape, Verdict},
    skills,
};

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
    driver: gym::REFLEX,
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
    objective: CAMERA_LIMIT.objective,
    seed: LedgerSeed::Warm,
    tape: CAMERA_LIMIT_TAPE,
    driver: CAMERA_LIMIT.driver,
    frame_budget: CAMERA_LIMIT.frame_budget,
    stall_frames: CAMERA_LIMIT.stall_frames,
    recorded: Verdict {
        outcome: Outcome::Stuck,
        frames: 120,
        furthest_x: 1544,
        refusals: 0,
    },
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
/// Measured both ways from the same bits: the reflex still loses here — `Stuck` after
/// 248 frames at absolute X 1544, the hole doing what it did to it — and the script of
/// [`skills::SCRIPT`] crosses on frame 151. The two numbers are the same tape with one
/// field changed, which is the only kind of evidence that a library is worth the frames
/// it costs.
///
/// The verdict this gym carries is the script's, because a gym's recorded verdict is
/// what its own driver does; `tests/skills.rs` pins the reflex's alongside it so neither
/// half of the pair can rot on its own.
pub const PIT_1379: Gym = Gym {
    name: "pit-1379",
    objective: CAMERA_LIMIT.objective,
    seed: LedgerSeed::Cold,
    tape: PIT_1379_TAPE,
    driver: skills::SCRIPTED,
    frame_budget: CAMERA_LIMIT.frame_budget,
    stall_frames: CAMERA_LIMIT.stall_frames,
    recorded: Verdict {
        outcome: Outcome::Met,
        frames: 151,
        // The harness reads the objective before it reads the position, so the frame
        // that satisfies `at_least: 1600` reports the X it stood on one frame earlier.
        furthest_x: 1599,
        refusals: 0,
    },
};

/// The run-up the pipe gym starts from: the base policy from power-on to frame 616,
/// which is absolute X 546 on the ground at full running speed, sixty-two pixels short of
/// the second pipe in 1-1 and one refusal short of knowing what that pipe is.
///
/// Recorded, not authored — `gym --record 616 pipe-608` cut it and replayed it before
/// printing it. The run-up holds no refusal at all, so this gym's `Cold` and `Warm` seeds
/// are the same start state; the lesson about the pipe is earned inside the gym, which is
/// the whole point of asking it there.
const PIPE_608_TAPE: Tape = Tape {
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
        (0x82, 41),  // B|RIGHT
    ]),
    frame: 616,
    golden_hash: engine_nes::StateHash {
        version: 3,
        value: 0xdcc8_e428_5344_c231,
    },
};

/// A pipe, cut upstream of the contact, for the one skill in the library that acts on a
/// wall.
///
/// The first gym `clear-wall` needed and did not have: Phase 3's exit asks each skill for
/// a situation that fails without it, and the two skills that had one were both about the
/// hole. This is the wall, taken from the run that stopped the reader-only policy at
/// absolute X 899 — the same pipe, one pipe earlier in the level than the one SMB-007
/// named, and the one the reflex is refused at on frame 642.
///
/// The start state is sixty-two pixels short of it at full running speed with a cold
/// ledger, which is what makes the verdict about the library rather than about the
/// memory: the pipe's artwork is not in the load-bearing set, so nothing in the picture
/// says *wall* until the game has refused him, and the run therefore earns one refusal and
/// then has to use it. `Met` after 108 frames at absolute X 698 with exactly that one
/// refusal is the script being told the pipe is solid and jumping it anyway; take
/// `clear-wall` out of the library and the same start state has no answer to a solid
/// column at all, however well the model knows it.
pub const PIPE_608: Gym = Gym {
    name: "pipe-608",
    objective: Objective::ReachX { at_least: 700 },
    seed: LedgerSeed::Cold,
    tape: PIPE_608_TAPE,
    driver: skills::SCRIPTED,
    frame_budget: CAMERA_LIMIT.frame_budget,
    stall_frames: CAMERA_LIMIT.stall_frames,
    recorded: Verdict {
        outcome: Outcome::Met,
        frames: 108,
        // The harness reads the objective before it reads the position, so the frame that
        // satisfies `at_least: 700` reports the X it stood on one frame earlier.
        furthest_x: 698,
        refusals: 1,
    },
};

/// The run-up the flagpole gym starts from: this gym's own driver — the script — from
/// power-on to frame 1900, which is absolute X 3005 on the ground at full running speed,
/// a hundred and sixty-three pixels short of the flagpole and one pit behind it.
///
/// Recorded, not authored — `gym --record 1900 flagpole-3168` cut it and replayed it
/// before printing it. This is the first tape here the reflex could not have cut: it
/// spends two lives at the hole four hundred pixels back, so every frame between the pit
/// and the level's exit is a run-up only the script can make. That is the whole reason
/// [`Gym::record`] runs the gym's own driver rather than the base policy.
const FLAGPOLE_3168_TAPE: Tape = Tape {
    cartridge: smb_model::cartridge::SMB_IDENTITY,
    runs: Cow::Borrowed(&[
        (0x00, 119), // NONE
        (0x08, 1),   // START
        (0x00, 167), // NONE
        (0x82, 119), // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 67),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 72),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 43),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 37),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 86),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 59),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 30),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 40),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 20),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 22),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 47),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 57),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 45),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 60),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 83),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 17),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 33),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 67),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 29),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 61),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 63),  // B|RIGHT
        (0x83, 4),   // A|B|RIGHT
        (0x82, 43),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 34),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 10),  // B|RIGHT
        (0x83, 20),  // A|B|RIGHT
        (0x82, 13),  // B|RIGHT
    ]),
    frame: 1900,
    golden_hash: engine_nes::StateHash {
        version: 3,
        value: 0x97f2_d3ee_150f_8352,
    },
};

/// 1-1's exit, cut upstream of it, for the one wall in the level no skill in the library
/// has an answer for.
///
/// The flagpole is a column the reader reports as solid and twelve rows tall, and the run
/// only gets past it because [`smb_model::affordances::MAX_WALL_ROWS`] caps a reported
/// height at eight, which is where the height gate cannot fire. That is the situation
/// Phase 3 still owes: a wall too tall to jump needs a skill whose postcondition is not
/// "past the top of it", and until that skill exists the level ends by accident rather
/// than by design. This gym is where the accident is on record and where the skill will
/// be measured when it lands.
///
/// The objective is the level's own ending rather than a pixel, because the pixel is the
/// thing under dispute: absolute X holds at 3161 for sixty-four frames while the game
/// takes the grab, and a `ReachX` past the pole would be met by the walk into the castle
/// whether or not anything understood the pole. `ClearLevel` asks the only question that
/// cannot be answered by drifting past a wall.
///
/// The two budgets are sized by the ending and not by the obstacle, which is worth
/// saying out loud because it is the one place a stall window has to be measured against
/// the game rather than against a bot: after the grab he walks to the castle door, and
/// the game then holds absolute X at 3266 for four hundred and eighty-eight frames while
/// it plays the level out. A stall window of the house's 120 frames calls that `Stuck`
/// two hundred and sixty-eight frames before the level says it is finished.
pub const FLAGPOLE_3168: Gym = Gym {
    name: "flagpole-3168",
    objective: Objective::ClearLevel,
    seed: LedgerSeed::Cold,
    tape: FLAGPOLE_3168_TAPE,
    driver: skills::SCRIPTED,
    frame_budget: 900,
    stall_frames: 600,
    recorded: Verdict {
        outcome: Outcome::Met,
        frames: 736,
        furthest_x: 3267,
        refusals: 1,
    },
};

/// Every committed gym, in the order a transcript prints them.
pub const ALL: [Gym; 5] = [
    CAMERA_LIMIT,
    CAMERA_LIMIT_REMEMBERING,
    FLAGPOLE_3168,
    PIPE_608,
    PIT_1379,
];

/// The gym a command line means, by the name it carries.
///
/// The examples take a gym name from `argv`, so the table lives here beside the gyms
/// rather than in one example's `match`: a name only one of them knows is a name that
/// quietly runs the wrong gym and prints a verdict for it anyway.
pub fn by_name(name: &str) -> Option<Gym> {
    ALL.into_iter().find(|gym| gym.name == name)
}
