//! The gym harness, checked against a real cartridge.
//!
//! A gym is only an instrument if its start state is the same state every time, so
//! these tests are about the tape rather than about Mario: a committed run-up has to
//! land on the bits it was recorded at, a verdict has to reproduce, and a tape that
//! does not hold has to fail as itself instead of becoming a strange result two
//! hundred frames later.
//!
//! Like the reader's test, these skip when no cartridge is present.
//!
//! The determinism check is the one DirtSim's `ReplayFromRootIsDeterministic` makes,
//! and it is here because a gym whose replay drifts by a frame measures nothing. It
//! is also the check that a bot's buttons reach the game in the order they were
//! decided: the core latches a controller on the game's own strobe, so an input that
//! arrived at the wrong moment inside a frame would show up as two replays of one
//! tape disagreeing, which is exactly what this refuses to allow.

use engine_nes::{CartridgeIdentity, ControllerButtons};
use smb_ai::{
    gym::{Gym, LedgerSeed, Tape},
    gyms,
};
use smb_model::cartridge;
use smb_model::profile::read_state;

/// Every committed gym, so a new one cannot join without being measured.
const ALL: &[Gym; 2] = &[gyms::CAMERA_LIMIT, gyms::CAMERA_LIMIT_REMEMBERING];

/// Run a tape and then hold one button for a while, so a comparison can see the game
/// continue rather than only see it arrive.
fn replay_and_play(
    rom: &[u8],
    tape: &Tape,
    seed: LedgerSeed,
    play_frames: u64,
) -> (Vec<u16>, String) {
    let start = tape
        .replay(rom, seed)
        .expect("a committed tape should replay");
    let mut machine = start.machine;
    let mut positions = Vec::new();
    for _ in 0..play_frames {
        machine
            .run_frame([
                ControllerButtons::RIGHT | ControllerButtons::B,
                ControllerButtons::NONE,
            ])
            .expect("the core should keep running");
        positions.push(read_state(&machine).absolute_x);
    }
    (positions, machine.state_hash().to_string())
}

/// The frame a committed tape claims to start on has to be the frame it lands on, at
/// the state hash it was recorded at, with the player in the state the gym was cut
/// for. This is the golden hash doing its job: a core change that moves SMB fails
/// here, by name, instead of moving the goalposts of every gym at once.
#[test]
fn a_committed_tape_replays_to_the_bits_it_was_recorded_at() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    for gym in ALL {
        let start = gym
            .tape
            .replay(&rom, gym.seed)
            .unwrap_or_else(|error| panic!("{}: its own tape should replay: {error}", gym.name));
        let state = read_state(&start.machine);
        assert_eq!(
            start.machine.state_hash(),
            gym.tape.golden_hash,
            "{}: the tape replayed to different bits than it was recorded at",
            gym.name,
        );
        assert_eq!(start.frame, gym.tape.frame, "{}: wrong frame", gym.name);
        assert!(
            state.is_in_control(),
            "{}: the tape ends with the game not holding the player, so the gym would \
             start by pushing at a world that is not listening: {state:?}",
            gym.name,
        );
        // The game numbers worlds and levels from zero, so 1-1 is (0, 0).
        assert_eq!(
            (state.world, state.level),
            (0, 0),
            "{}: the tape was cut in 1-1 and landed elsewhere: {state:?}",
            gym.name,
        );
        assert_eq!(
            state.absolute_x, gym.recorded.furthest_x,
            "{}: the tape ends somewhere other than where its recorded verdict says",
            gym.name,
        );
    }
}

/// The same tape, replayed twice, has to land on the same bits and keep going the
/// same way: same positions frame by frame, same final hash. Nothing about the host
/// is allowed to reach into a run.
#[test]
fn replaying_the_same_tape_twice_lands_on_the_same_bits() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let gym = &gyms::CAMERA_LIMIT;
    let (first_positions, first_hash) = replay_and_play(&rom, &gym.tape, gym.seed, 60);
    let (second_positions, second_hash) = replay_and_play(&rom, &gym.tape, gym.seed, 60);
    assert_eq!(
        first_positions, second_positions,
        "the same buttons on the same tape moved him differently"
    );
    assert_eq!(
        first_hash, second_hash,
        "the same replay ended on different bits"
    );
    assert!(
        first_positions.windows(2).all(|pair| pair[0] >= pair[1]),
        "the recorded stop is not a stop any more: {first_positions:?}"
    );
}

/// A gym's verdict is a measurement, so it reproduces. `ALL` is every committed gym,
/// which is what stops a gym joining the list with a verdict nobody re-measured.
#[test]
fn every_committed_gym_still_measures_the_verdict_it_carries() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    for gym in ALL {
        let verdict = gym
            .run(&rom)
            .unwrap_or_else(|error| panic!("{}: could not run: {error}", gym.name));
        assert_eq!(
            verdict, gym.recorded,
            "{}: measured {verdict:?} where it is recorded at {:?}",
            gym.name, gym.recorded,
        );
    }
}

/// The two camera-limit gyms differ in one thing: whether the model arrives having
/// already been refused by the run-up. Measured, that difference is nothing here, and
/// "nothing" is the finding — the thing at the camera limit never refused him, so
/// there was never a lesson to remember, which is what makes it the first hole the
/// ledger cannot reach.
#[test]
fn remembering_the_run_up_changes_nothing_at_the_camera_limit() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let cold = gyms::CAMERA_LIMIT
        .run(&rom)
        .expect("the cold gym should run");
    let warm = gyms::CAMERA_LIMIT_REMEMBERING
        .run(&rom)
        .expect("the warm gym should run");
    assert_eq!(
        cold, warm,
        "the ledger was expected to be silent about a stop with no artwork attached, \
         and it was not"
    );
    assert!(
        warm.refusals == 0,
        "the camera limit refused him {warm:?}, which would make it a wall the model \
         could name and not a hole it cannot"
    );
}

/// A tape that does not hold fails as itself. Each of these is a mistake a careless
/// edit makes, and each has to be named rather than becoming a strange verdict.
#[test]
fn a_tape_that_does_not_hold_fails_as_itself() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let gym = &gyms::CAMERA_LIMIT;

    let short = Tape {
        runs: std::borrow::Cow::Owned(gym.tape.runs.iter().copied().take(4).collect()),
        ..gym.tape.clone()
    };
    let error = short
        .replay(&rom, gym.seed)
        .expect_err("four button runs cannot cover 1167 frames");
    assert!(
        matches!(error, smb_ai::gym::GymError::ShortTape { .. }),
        "a truncated tape failed as {error}"
    );

    let other = Tape {
        cartridge: CartridgeIdentity {
            byte_len: 1,
            fnv1a64: 2,
        },
        ..gym.tape.clone()
    };
    let error = other
        .replay(&rom, gym.seed)
        .expect_err("a tape recorded elsewhere is not this tape");
    assert!(
        matches!(error, smb_ai::gym::GymError::WrongCartridge { .. }),
        "a tape on the wrong cartridge failed as {error}"
    );

    let moved = Tape {
        golden_hash: engine_nes::StateHash {
            version: gym.tape.golden_hash.version,
            value: gym.tape.golden_hash.value ^ 1,
        },
        ..gym.tape.clone()
    };
    let error = moved
        .replay(&rom, gym.seed)
        .expect_err("a replay that lands elsewhere has to say so");
    assert!(
        matches!(error, smb_ai::gym::GymError::Drifted { .. }),
        "a core that moved the game failed as {error}"
    );
}
