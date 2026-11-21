//! Skills, their envelopes, and the script that strings them together.
//!
//! Two halves, and the split is the point. The first half is the library's *rules*,
//! asked of hand-built [`Situation`]s with no cartridge in sight: when a skill may
//! start, when it is only waiting, and when a landing it cannot see is a refusal. Those
//! run on any machine, which matters because they are the claims that survive a core
//! change — the second half are measurements, and a measurement is only true of the
//! bits it was taken from.
//!
//! The second half replays a committed tape and asks the game. An envelope is a claim
//! about what twenty frames of A do to a body at running speed, and like every other
//! claim in this project it is worth exactly what its measurement is worth: `measure`
//! re-runs the buttons and the test holds the result against the constant the skills
//! were written with. A core change that moves the jump fails here, by name, rather
//! than leaving a script running on numbers that stopped being true.
//!
//! The pit gym is the reason any of it exists, so it is also the proof: the same tape,
//! the same objective, the reflex against the script, and the frame the script chose.

use engine_nes::{ControllerButtons, NesMachine};
use smb_ai::{
    gym::{self, Gym, LedgerSeed, Start, Tape},
    gyms,
    policy::{BaseBot, Bot},
    skills::{
        self, CLEAR_GAP, CLEAR_WALL, Call, HOP, HOP_OBSTACLE, LONG_JUMP, Running, SCRIPT,
        ScriptedBot, Situation,
    },
};
use smb_model::{
    affordances::{Gap, Wall, body_last_pixel},
    cartridge,
    profile::{MODE_END_GAME, read_state},
};

/// The pit as the model read it on the frames that matter, from `limit` and the
/// scripted bot's own trace: the hole starts at world pixel 1376, the ground returns at
/// 1424, and the camera's window ends about where a long jump comes down.
const PIT: Gap = Gap {
    near_x: 1376,
    far_x: 1424,
    row: 13,
    far_edge_seen: true,
};

/// Standing on the lip of nothing, with a hole ahead the picture has bounded.
fn on_ground_at(x: u16, gap: Option<Gap>) -> Situation {
    Situation {
        x,
        row: PIT.row,
        grounded: true,
        in_control: true,
        gap,
        ..Situation::default()
    }
}

/// A hole whose far lip is inside the picture, a landing box that reaches it, and the
/// ground under that box: the one situation `clear-gap` starts on.
#[test]
fn clear_gap_starts_when_the_landing_is_on_ground_the_picture_shows() {
    let at = Situation {
        landing_is_solid: true,
        ..on_ground_at(1296, Some(PIT))
    };
    assert_eq!(
        CLEAR_GAP.call(&at),
        Call::Start,
        "the long jump's box comes down at {} on ground the picture showed, and the \
         skill refused",
        LONG_JUMP.landing_last_pixel(at.x),
    );
}

/// The near edge of the commit window, as a rule rather than as a sweep.
///
/// The bracket is measured, not inferred: the same twenty frames of A, replayed from
/// this tape, come off the last grounded frame at 1291 and are in the hole, and come off
/// 1294 and come down at 1411 and stand. The rule has to put its own edge inside that
/// one-frame gap, and it does — with the envelope's own carry of 118 and the landing
/// asked at the last pixel of the box, the first position it will accept is 1293.
///
/// It is worth naming what the rule cannot see from here. The carry is 118 at the frame
/// the envelope is pinned to and a flat 115 from thirty frames on, so the edge the rule
/// computes is two or three pixels more generous than the game's across most of the
/// window. On this tape nothing can tell the difference, because the far lip does not
/// exist to the picture until absolute X 1296 and the camera, not the reach, sets the
/// first frame the skill may start. On a level that shows its far lip earlier the same
/// rule would commit to a jump that comes down a pixel short, and that is a gap to close
/// by measuring a second frame rather than by fudging this one.
#[test]
fn clear_gap_waits_for_the_frame_the_far_lip_is_reachable() {
    let short = Situation {
        landing_is_solid: true,
        ..on_ground_at(1291, Some(PIT))
    };
    assert_eq!(
        CLEAR_GAP.call(&short),
        Call::Wait,
        "the game returns a jump taken from here, and a skill that starts here is the \
         base policy"
    );
    assert_eq!(
        HOP_OBSTACLE.call(&short),
        Call::None,
        "the hop has no business with a hole, and answering Wait here would let the \
         script read this frame as handled"
    );
    let crossing = Situation {
        landing_is_solid: true,
        ..on_ground_at(1294, Some(PIT))
    };
    assert_eq!(
        CLEAR_GAP.call(&crossing),
        Call::Start,
        "the game scores a jump taken from 1294 as a crossing, so the rule has to take \
         it too"
    );
}

/// A hole the picture has not finished drawing is not a hole with no far side.
///
/// `far_edge_seen` is the difference between `None` and `Some(false)` from the reader,
/// and this is where it earns its keep: the pit's far lip comes into the camera's window
/// on frame 28 of this gym and not one frame earlier, so every frame before it has to be
/// a wait rather than a jump *and* rather than a refusal. A `Start` here is the base
/// policy jumping at an enemy it could see towards a hole it could not.
#[test]
fn clear_gap_waits_while_the_far_lip_is_off_the_picture() {
    let unseen = Gap {
        far_edge_seen: false,
        far_x: 1393,
        ..PIT
    };
    let at = on_ground_at(1264, Some(unseen));
    assert_eq!(
        CLEAR_GAP.call(&at),
        Call::Wait,
        "the reading stopped before the ground came back, which is neither a landing nor \
         a hole with no far side"
    );
}

/// The frame the base policy died on, asked of the library.
///
/// An enemy twenty-two pixels ahead, a hole one hundred and twelve pixels ahead whose
/// far lip is not yet drawn, and a hop's landing box at 1343 on ground the picture
/// shows. The long jump is right to wait. The hop is right to go, and it is the reason
/// the script survives this frame: the decision was never the problem, the envelope was.
#[test]
fn the_hop_takes_the_frame_the_long_jump_is_only_waiting_for() {
    let unseen = Gap {
        far_edge_seen: false,
        far_x: 1393,
        ..PIT
    };
    let at = Situation {
        landing_is_solid: true,
        enemy_pixels: Some(22),
        ..on_ground_at(1264, Some(unseen))
    };
    assert_eq!(CLEAR_GAP.call(&at), Call::Wait);
    assert_eq!(
        HOP_OBSTACLE.call(&at),
        Call::Start,
        "an enemy inside the trigger with a landing the picture vouches for is the hop's \
         situation, and the script's order must not lose it"
    );
    assert_eq!(
        HOP.landing_last_pixel(at.x),
        1342,
        "the hop's landing is the whole argument for its envelope, so the number moved"
    );
    assert!(
        HOP.landing_last_pixel(at.x) < PIT.near_x,
        "the hop has to come down short of the hole: {} vs {}",
        HOP.landing_last_pixel(at.x),
        PIT.near_x,
    );
}

/// The same two skills, twenty-odd pixels further along, where the hop's landing falls
/// inside the hole and only the long jump has a landing at all.
#[test]
fn the_hop_refuses_a_landing_inside_the_hole() {
    let at = Situation {
        landing_is_solid: false,
        enemy_pixels: Some(22),
        ..on_ground_at(1320, Some(PIT))
    };
    assert_eq!(
        HOP_OBSTACLE.call(&at),
        Call::None,
        "the hop's box comes down at {} over the hole, and a flinch is what killed the \
         reflex",
        HOP.landing_last_pixel(at.x),
    );
    let seen = Situation {
        landing_is_solid: true,
        ..on_ground_at(1320, Some(PIT))
    };
    assert_eq!(
        CLEAR_GAP.call(&seen),
        Call::Start,
        "the long jump's box comes down at {} and the hole is 48 pixels wide, so this is \
         the frame",
        LONG_JUMP.landing_last_pixel(seen.x),
    );
}

/// Nothing in the library starts on a body the game is not letting move.
#[test]
fn no_skill_starts_without_the_controller_or_the_ground() {
    for at in [
        Situation {
            grounded: false,
            in_control: true,
            landing_is_solid: true,
            enemy_pixels: Some(10),
            gap: Some(PIT),
            ..Situation::default()
        },
        Situation {
            grounded: true,
            in_control: false,
            landing_is_solid: true,
            enemy_pixels: Some(10),
            gap: Some(PIT),
            ..Situation::default()
        },
    ] {
        for skill in SCRIPT {
            assert_ne!(
                skill.call(&at),
                Call::Start,
                "{} started from {at:?}",
                skill.name,
            );
        }
    }
}

/// The first pipe in 1-1, as the model read it on the frame the first full-level run of
/// the script stopped at: the refusing column starts at world pixel 448 and the pipe
/// draws four rows above his feet, and the run was stuck there for six hundred frames.
const PIPE: Wall = Wall { x: 448, rows: 4 };

/// The first step of the stairs near the end of 1-1, at the frame the run after it
/// stopped: absolute X 2162, the step's near edge at 2176, two rows up.
///
/// The `gap` is not decoration — it is what the hole reading actually returned there.
/// The staircase is drawn with artwork the load-bearing set has never held a body on, and
/// `carries_weight_at` has no third answer for a cell it has no evidence about, so the
/// reading walked sixty-four pixels of "nothing to stand on" and stopped at its own cap
/// with no far side. That is a false hole, and it is the next thing to fix.
const STAIR_STEP: Wall = Wall { x: 2176, rows: 2 };

/// The feet row both of those walls were read on.
const STAIR_STEP_ROW: i16 = 21;

const STAIR_HOLE: Gap = Gap {
    near_x: 2176,
    far_x: 2240,
    row: STAIR_STEP_ROW,
    far_edge_seen: false,
};

/// Standing on ground, in control, with `wall` in front of him and nothing else known.
fn facing(wall: Option<Wall>, x: u16) -> Situation {
    Situation {
        x,
        row: STAIR_STEP_ROW,
        grounded: true,
        in_control: true,
        wall,
        ..Situation::default()
    }
}

/// Phase 0's rule, applied to a wall: an affordance that cannot name its cell cannot be
/// postconditioned, and a skill whose postcondition is already true is not a skill.
///
/// This is the bug the first full-level run found at the first pipe. `hop-obstacle`'s
/// claim used to be "past the obstacle", computed as `x + enemy_pixels.unwrap_or(0)`.
/// With a pipe in front and no enemy in front of that, the pixel was `x` itself — which a
/// body fourteen pixels wide is always past — so the postcondition that [`Skill::call`]
/// asks *first* answered "already done", the skill cancelled its own start, and the run
/// sat against the pipe with the hop's precondition satisfied on every frame of it. The
/// wall reports a pixel now, and the pixel is in front of the body on the frame the skill
/// starts.
#[test]
fn a_wall_names_a_pixel_the_body_is_not_already_past() {
    let at = facing(Some(PIPE), 434);
    let past = CLEAR_WALL.past(&at);
    assert_eq!(
        past, PIPE.x,
        "the pixel the situation is about is the wall's"
    );
    assert!(
        body_last_pixel(at.x) < past,
        "a skill started about {} has to be started from behind it, not from {}",
        past,
        at.x,
    );
    assert!(
        !CLEAR_WALL.achieved(&at, past),
        "and its claim has to be one the world has not already granted"
    );
}

/// The hop's half of the same frame, and the partition the two skills now agree on.
///
/// The hop is enemies and nothing else: measured clearing a koomba, never measured
/// clearing a wall, and a four-frame hold reaches its top long after it reaches the pipe
/// in front of it. `clear-wall` is the long jump, which this project has filmed clearing
/// that pipe, and four rows is thirty-two pixels against a measured rise of seventy-five.
#[test]
fn the_hop_leaves_the_pipe_to_the_skill_that_can_get_over_it() {
    let at = facing(Some(PIPE), 434);
    assert_eq!(
        HOP_OBSTACLE.past(&at),
        at.x,
        "with no enemy ahead the hop has no pixel in front of the body to be past, which \
         is its way of saying this is not its situation"
    );
    assert_eq!(
        HOP_OBSTACLE.call(&at),
        Call::None,
        "and it declines the frame rather than cancelling on its own postcondition"
    );
    assert_eq!(
        CLEAR_WALL.call(&at),
        Call::Start,
        "four rows is {} pixels and the long jump rises {}; the pipe is this one's frame",
        PIPE.height_pixels(),
        LONG_JUMP.rise_pixels,
    );
}

/// The frame the landing gate cost, pinned.
///
/// `clear-wall` is the one skill in the library that does not ask
/// [`Situation::landing_is_solid`], and this is the frame that decided it. The long jump's
/// box comes down two pixels past the last column the PPU is drawing, so the direct
/// question has no answer; the fallback that saves a long jump over a hole is a hole whose
/// far lip the picture showed, and the hole reading here has never seen a far side because
/// the staircase's cells are not in the load-bearing set. A gate that cannot be answered is
/// a refusal, so the version with the gate stood in front of a two-row step — sixteen
/// pixels, against a rise of seventy-five — and refused it for six hundred frames.
///
/// The wall is the landing instead. `wall_ahead` reports a column only when the
/// load-bearing set has held a body on it or the ledger was refused by it, and a cell that
/// stops a body sideways is the same collision that holds one up.
#[test]
fn clear_wall_starts_where_the_landing_gate_cannot_be_answered() {
    let at = Situation {
        gap: Some(STAIR_HOLE),
        ..facing(Some(STAIR_STEP), 2162)
    };
    assert!(
        !at.landing_is_solid,
        "the box comes down at {}, and neither source the gate has can speak for it",
        LONG_JUMP.landing_last_pixel(at.x),
    );
    assert_eq!(
        CLEAR_GAP.call(&at),
        Call::Wait,
        "the gap skill reads the staircase as a hole with no far side and passes the frame \
         on, which is the right answer to a hole it cannot see the end of"
    );
    assert_eq!(
        CLEAR_WALL.call(&at),
        Call::Start,
        "two rows is {} pixels: the wall the model can name is a surface it can name",
        STAIR_STEP.height_pixels(),
    );
}

/// The height gate, at its edge.
///
/// Nine rows is seventy-two pixels, inside the long jump's measured rise; ten is eighty,
/// outside it. Neither refusal is reachable from the picture today: `MAX_WALL_ROWS` caps a
/// reported height at eight rows, sixty-four pixels, under the long jump's rise — and the
/// reason that cap is not lifted is measured and lives with the constant, because the
/// flagpole at the end of 1-1 is a twelve-row column no jump in the library clears and a
/// gate that fires on height alone stops the run fourteen pixels short of it. The rule is
/// still written for the wall that does not exist yet: a fabricated level can build one,
/// and a `Start` there is a jump that ends with him pressed against the same column.
#[test]
fn a_wall_taller_than_the_rise_is_nobody_s_frame() {
    for (rows, want) in [(9, Call::Start), (10, Call::Wait)] {
        let at = facing(Some(Wall { x: 2176, rows }), 2162);
        assert_eq!(
            CLEAR_WALL.call(&at),
            want,
            "a wall of {rows} rows is {} pixels and this envelope rises {}",
            rows * 8,
            LONG_JUMP.rise_pixels,
        );
        assert_eq!(
            HOP_OBSTACLE.call(&at),
            Call::None,
            "the hop declines it too, so the frame comes back to the caller unspent rather \
             than being spent on a jump that cannot finish it"
        );
    }
}

/// Run the tape with the script and keep what the bot did, which the harness's verdict
/// does not report: the frame each behaviour started on and where he was standing.
fn scripted(rom: &[u8], tape: &Tape, budget: u64) -> (Vec<Running>, u16, bool) {
    scripted_with(rom, tape, skills::SCRIPT, budget)
}

/// The same, with a library chosen by the caller, so a skill can be removed and the
/// difference attributed to it.
fn scripted_with(
    rom: &[u8],
    tape: &Tape,
    script: &'static [&'static skills::Skill],
    budget: u64,
) -> (Vec<Running>, u16, bool) {
    let Start {
        mut machine, bot, ..
    } = tape
        .replay(rom, LedgerSeed::Cold)
        .expect("a committed tape should replay");
    let mut player = ScriptedBot::with_script(bot, script);
    let opened = read_state(&machine);
    let mut furthest = opened.absolute_x;
    let mut died = false;
    for _ in 0..budget {
        let at_frame = machine.ppu().frame_id();
        let buttons = player.buttons(&machine);
        machine
            .run_frame([buttons, ControllerButtons::NONE])
            .expect("the core should keep running");
        let produced = read_state(&machine);
        furthest = furthest.max(produced.absolute_x);
        died |= produced.lives < opened.lives || produced.game_mode == MODE_END_GAME;
        player.observe(&produced, &machine, at_frame);
        if died {
            break;
        }
    }
    (player.started().to_vec(), furthest, died)
}

/// The gym, the objective and the tape, with the reflex in the driver's seat instead of
/// the script: one field apart, which is what makes the pair a measurement.
fn reflex(gym: Gym) -> Gym {
    Gym {
        driver: gym::REFLEX,
        ..gym
    }
}

/// The claim Phase 3 is built on, measured: the same start state, the same objective,
/// the reflex where it stops at the hole and the script where it crosses it.
///
/// The reflex's verdict is the one the gym carries beside its own, so this test fails if
/// either half stops being true — including the half where the reflex starts winning by
/// accident, which would mean the gym stopped being a problem.
#[test]
fn the_script_crosses_the_pit_the_reflex_falls_into() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let pit = gyms::PIT_1379;
    let crossed = pit.run(&rom).expect("the scripted gym should run");
    let fallen = reflex(pit)
        .run(&rom)
        .expect("the same tape under the reflex should run");
    assert_eq!(
        fallen.outcome,
        gym::Outcome::Stuck,
        "the reflex was expected to stop at the hole and instead reported {fallen:?}"
    );
    assert_eq!(
        crossed.outcome,
        gym::Outcome::Met,
        "the script did not cross: {crossed:?} against the reflex's {fallen:?}"
    );
    assert!(
        crossed.furthest_x > fallen.furthest_x,
        "a fall clamps absolute X at the screen edge, so the crossing has to be further
         than the stop: {crossed:?} vs {fallen:?}"
    );
}

/// The ablation the plan asks for, and the answer is not the one the design expected.
///
/// Take the hop out of the script, leave everything else — the same tape, the same long
/// jump, the same rule about landings — and the script still crosses the hole, from the
/// very first frame the far lip is inside the picture. What it then does is die at
/// absolute X 1526, two hundred and eighty-six pixels past the gym's name, on a pair of
/// koopas walking left: the trace has them at seventy-nine pixels ahead on frame 1135,
/// closing at three and a half pixels a frame, and the run's X freezes at 1526 on frame
/// 1160 with the nearest one five pixels in front of him. The life counter still reads
/// two, because it lags the death by seconds, and the budget ends before the game admits
/// it.
///
/// Put the hop back and it fires twice on this tape: once at frame 1055 at the koomba
/// twenty-two pixels ahead, where the run did not need it, and once at frame 1147 at
/// forty-three pixels, where it did. The second hop is what reaches 1600. So the hop is
/// load-bearing for `pit-1379`'s verdict and is not load-bearing for the pit, and a gym
/// named after a hole measured a skill that has nothing to do with it.
///
/// That is worth keeping in the file rather than smoothing over, because it is the
/// ablation doing its job: the design assumed the enemy at 1264 was the hop's reason and
/// the measurement says the koopas past the lip are. Phase 3's exit asks each skill for a
/// measured envelope beside it and should also ask for a gym that fails without it — and
/// the gym that would fail without the hop is a pair of enemies on flat ground, which is
/// not in this repository yet.
#[test]
fn the_hop_wins_this_gym_past_the_pit() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let pit = gyms::PIT_1379;

    let long_only = scripted_with(&rom, &pit.tape, &[&CLEAR_GAP], 160);
    let crossings: Vec<&Running> = long_only
        .0
        .iter()
        .filter(|running| running.skill == &CLEAR_GAP)
        .collect();
    let [crossing] = crossings.as_slice() else {
        panic!(
            "expected one clear-gap from the long-jump-only script, got {:?}",
            long_only
                .0
                .iter()
                .map(|running| (running.skill.name, running.started_at, running.from_x))
                .collect::<Vec<_>>(),
        );
    };
    assert_eq!(
        crossing.from_x, 1296,
        "without the hop the script is free to commit on the first frame the far lip is
         readable, which is 1296; anything else means the reach rule moved"
    );
    assert!(
        long_only.1 >= PIT.far_x,
        "the long jump alone was expected to cross the hole and stopped at {}",
        long_only.1,
    );
    assert_eq!(
        long_only.1, 1526,
        "the long-jump-only run was expected to stop dead at the koopas past the lip, at
         1526; it reached {} instead, so something downstream changed",
        long_only.1,
    );

    let both = scripted(&rom, &pit.tape, 160);
    let names: Vec<(&str, u64, u16)> = both
        .0
        .iter()
        .map(|running| (running.skill.name, running.started_at, running.from_x))
        .collect();
    assert_eq!(
        names
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<&str>>(),
        vec![HOP_OBSTACLE.name, CLEAR_GAP.name, HOP_OBSTACLE.name],
        "the script's run on this tape is hop, cross, hop; anything else and the second \
         hop is not being attributed: {names:?}"
    );
    assert!(
        !both.2 && both.1 >= 1600,
        "with the hop back in the library the run was expected to reach the objective, \
         got {both:?}"
    );
    let second_hop = both.0.last().expect("the third start");
    assert!(
        (1490..=1510).contains(&second_hop.from_x),
        "the hop that wins this gym is the one taken at the koopas just past the lip, \
         measured at 1494; it started from {}",
        second_hop.from_x,
    );
}

/// Where the script chose to commit, against the window the game scored.
///
/// Two instruments bracket it. The sweep pressed every other frame and scored each press
/// over 420 frames: presses 28 to 58 get him across. The envelope sweep then pressed the
/// frames in between and reported where each came down, and it moved the near edge one
/// frame earlier — press 27, whose last grounded frame is absolute X 1294, comes down at
/// 1411 and stands, where press 26 at 1291 is in the hole. The positions the window
/// stands on are therefore 1294 to 1371, and press 60 at 1376 is already too late: he is
/// at the lip and the jump taken from there is spent inside the hole.
///
/// The script is not allowed to be lucky: it has to arrive inside that window by its own
/// rule, and the frame it picks is the frame the far lip first comes into the camera's
/// window, which is the first frame anything reading the screen could have made this
/// decision at all.
#[test]
fn the_script_commits_inside_the_measured_window() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let pit = gyms::PIT_1379;
    let (started, furthest, died) = scripted(&rom, &pit.tape, 160);
    let crossings: Vec<&Running> = started
        .iter()
        .filter(|running| running.skill == &CLEAR_GAP)
        .collect();
    let [crossing] = crossings.as_slice() else {
        panic!(
            "expected exactly one clear-gap, got {:?} (furthest X {furthest}, died {died})",
            started
                .iter()
                .map(|running| (running.skill.name, running.started_at, running.from_x))
                .collect::<Vec<_>>(),
        );
    };
    assert!(
        !died,
        "the crossing jump was made and he still died: {started:?}"
    );
    assert!(
        (1294..=1371).contains(&crossing.from_x),
        "the commit was made from absolute X {}, outside the window the game scored as \
         the only ones that cross",
        crossing.from_x,
    );
    assert!(
        crossing.past >= PIT.far_x,
        "the skill named the pixel it was getting past as {} and the ground returns at {}",
        crossing.past,
        PIT.far_x,
    );
    assert!(
        furthest >= 1600,
        "he crossed the hole and stopped at {furthest}, which is where a fall clamps"
    );
}

/// The order the script actually runs in, pinned rather than assumed.
///
/// The enemy is twenty-two pixels ahead on the frame the run-up ends and the hop takes
/// it, which costs the script the frame it would otherwise have committed on: with the
/// hop in the library the long jump starts from 1330 rather than 1296, because he is in
/// the air across the first frames the far lip is readable. Both are inside the window
/// and the run survives, but a second skill in a script is not only a gain, and
/// [`the_hop_is_not_what_wins_this_gym`] is the half of that measurement the ordering
/// cannot show on its own.
#[test]
fn the_script_hops_the_enemy_before_it_hops_the_pit() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let pit = gyms::PIT_1379;
    let (started, _, died) = scripted(&rom, &pit.tape, 160);
    let names: Vec<&str> = started.iter().map(|running| running.skill.name).collect();
    assert_eq!(
        names.first().copied(),
        Some(HOP_OBSTACLE.name),
        "the first thing the script did was {names:?}, and the enemy comes before the \
         hole on every frame the reader can see"
    );
    assert!(
        started.iter().any(|running| running.skill == &CLEAR_GAP),
        "the script never asked about the hole: {names:?}"
    );
    assert!(!died, "the script died on the way: {names:?}");
}

/// Every number on every envelope, re-measured against the cartridge.
///
/// Each envelope names the tape and the frame it was measured from, and this re-runs
/// exactly that: a replay, a run-up of plain running, then the buttons. A core change
/// that moves the jump by a frame moves the carry, and the constant has to move with it
/// or the library is describing a game nothing is playing.
#[test]
fn every_envelope_measures_the_numbers_it_carries() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let tape = &gyms::PIT_1379.tape;
    // The frame the enemy reaches the trigger and the frame the pit's far lip first
    // comes into the picture: the two decisions the library makes on this tape.
    for (envelope, run_up) in [(HOP, 15u64), (LONG_JUMP, 28u64)] {
        let measured = skills::measure(&rom, tape, &envelope, run_up)
            .unwrap_or_else(|error| panic!("{}: could not be measured: {error}", envelope.name));
        assert!(
            measured.landed,
            "{} did not come down inside the budget from frame {run_up}, so its carry is \
             not a number that can be pinned",
            envelope.name,
        );
        assert_eq!(
            measured.envelope(envelope.name, envelope.hold_frames),
            envelope,
            "{} measured from frame {run_up} of the pit tape came out differently",
            envelope.name,
        );
    }
}

/// A bot is a [`smb_ai::policy::Bot`] whatever is inside it, and the harness only knows
/// the trait. Checked here as a type assertion rather than a run: the day a search is
/// added, it joins the same table for the same reason.
#[test]
fn a_script_and_a_reflex_are_the_same_kind_of_player() {
    /// The only way the harness can hold two players: one function, one call.
    fn drives(player: &mut dyn Bot, machine: &NesMachine) -> ControllerButtons {
        player.buttons(machine)
    }
    let _: fn(BaseBot) -> Box<dyn Bot> = skills::scripted;
    let _: fn(BaseBot) -> Box<dyn Bot> = gym::reflex;
    let _: fn(&mut dyn Bot, &NesMachine) -> ControllerButtons = drives;
}

/// The script with `clear-wall` taken out of it, as a driver, so a gym can be run with it
/// and the difference attributed to one entry in a library.
fn without_a_wall_skill(bot: BaseBot) -> Box<dyn Bot> {
    Box::new(ScriptedBot::with_script(bot, &[&CLEAR_GAP, &HOP_OBSTACLE]))
}

/// The library's answer to a pipe, measured both ways on one tape.
///
/// Phase 3's exit asks every skill for a situation that fails without it, and until now
/// both gyms with that property were about the hole: `clear-gap` and `hop-obstacle` are
/// load-bearing for `pit-1379` and `clear-wall` had nothing. `pipe-608` is the wall, and
/// the pair is one field of one gym apart.
///
/// With the library as written: `Met` after 108 frames at absolute X 698, on one
/// behaviour — `clear-wall` started on frame 643 from absolute X 594, about pixel 608,
/// which is the frame *after* the game refused him there. That ordering is the whole
/// gym's design. The pipe's artwork is not in the load-bearing set and the ledger starts
/// empty, so nothing in the picture calls that column solid until the game has said so;
/// the run earns one refusal and then spends it.
///
/// Take the skill out and the same start state, the same objective and the same model
/// — which by then knows the pipe, and knows it eighteen times over — come to `Stuck`
/// after 142 frames at absolute X 595 with **eighteen refusals**. The ablated transcript
/// says what the library does instead: `hop-obstacle`, twice, from absolute X 594 about
/// pixel 642, because there is an enemy forty-eight pixels ahead and that is the only
/// affordance in reach that a skill knows how to act on. The hop carries sixty-five
/// pixels and rises thirty-nine, and a four-row pipe needs more than that, so he hops
/// into the column, is refused, and hops again three hundred frames later.
///
/// That is the sharpest form this evidence could have come in, because the failure is not
/// the model's. The refusal signature is recorded against the CHR pattern that made it,
/// the ledger has held it since the first of the eighteen, and `wall_ahead` names the
/// column at 608 on the frames he is in front of it — the model knows the pipe is solid and
/// the library still has no thing to do about it. Knowing a wall and clearing one are two layers, and only one of
/// them had a gym.
#[test]
fn the_pipe_gym_fails_without_the_skill_that_acts_on_a_wall() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let pipe = gyms::PIPE_608;
    let cleared = pipe.run(&rom).expect("the scripted pipe gym should run");
    assert_eq!(
        cleared.outcome,
        gym::Outcome::Met,
        "the script is expected to clear the pipe it carries a skill for: {cleared:?}"
    );

    let (started, furthest, died) =
        scripted_with(&rom, &pipe.tape, &[&CLEAR_GAP, &HOP_OBSTACLE], 600);
    assert!(
        !started.iter().any(|run| run.skill == &CLEAR_WALL),
        "the ablated library has no wall skill to start, and the transcript says it did \
         not invent one: {started:?}"
    );
    assert!(
        started.iter().all(|run| run.skill == &HOP_OBSTACLE),
        "what the ablated library offers at a pipe is the hop it borrowed from an enemy, \
         twice, from the same pixel: {started:?}"
    );
    assert!(
        !died,
        "the ablated run stops rather than dying, so the verdict below is about the stop"
    );
    assert!(
        furthest <= 608,
        "eighteen refusals and no hop long enough is a body that never gets its box past \
         the column it was refused by: furthest absolute X {furthest} against the wall at 608"
    );

    let stalled = Gym {
        driver: gym::Driver {
            name: "script minus clear-wall",
            player: without_a_wall_skill,
        },
        ..pipe
    }
    .run(&rom)
    .expect("the same tape under the ablated script should run");
    assert_eq!(
        stalled.outcome,
        gym::Outcome::Stuck,
        "the gym exists to fail without this skill, and it did not: {stalled:?} against \
         the scripted {cleared:?}"
    );
    assert!(
        stalled.furthest_x < cleared.furthest_x,
        "a wall the model names and the library cannot act on stops him short of the \
         objective: {stalled:?} vs {cleared:?}"
    );
    assert!(
        stalled.refusals > cleared.refusals,
        "the ablated run is refused many times by a cell the scripted run is refused once \
         and then jumps: {stalled:?} vs {cleared:?}"
    );
}

/// The library minus one named entry, as a driver, so a gym can be run with an
/// ablation the transcript can also print.
///
/// A `Driver`'s player is a function pointer and cannot capture the filtered list, so
/// each ablation this project has measured gets its own one-line function naming the
/// library it removes. Three is the whole library, so the table is complete.
fn without_a_gap_skill(bot: BaseBot) -> Box<dyn Bot> {
    Box::new(ScriptedBot::with_script(bot, &[&CLEAR_WALL, &HOP_OBSTACLE]))
}

fn without_a_hop_skill(bot: BaseBot) -> Box<dyn Bot> {
    Box::new(ScriptedBot::with_script(bot, &[&CLEAR_GAP, &CLEAR_WALL]))
}

/// A driver with one entry deleted, named for the deletion so a verdict prints beside
/// the thing it is a measurement of.
fn minus(name: &'static str) -> gym::Driver {
    let player: fn(BaseBot) -> Box<dyn Bot> = match name {
        "clear-wall" => without_a_wall_skill,
        "clear-gap" => without_a_gap_skill,
        "hop-obstacle" => without_a_hop_skill,
        other => panic!("no skill named {other} to delete"),
    };
    gym::Driver {
        name: Box::leak(format!("script minus {name}").into_boxed_str()),
        player,
    }
}

/// 1-1's exit, measured four ways from one tape, and the answer is not the one the plan
/// had for it.
///
/// The plan left the flagpole as the situation the library has no skill for: a column
/// twelve rows tall that no jump in the library gets over, needing "a skill whose
/// postcondition is not 'past it'". `flagpole-3168` is that column with a verdict
/// attached, and the verdict says the missing skill is not the missing thing.
///
/// **The script wins it, and so does the reflex.** `Met` after 736 frames at absolute X
/// 3267 for the script, `Met` after 757 at 3266 for the reflex — the first gym in this
/// repository the reflex wins, and it wins because the flagpole is not a wall to the
/// game. The run's one behaviour is `clear-wall` on frame 1971 from absolute X 3154 about
/// pixel 3168, and what that jump buys is not height: absolute X holds at 3161 for
/// sixty-four frames while the game takes the grab, and the level ends at 3266. The
/// reflex gets there twenty-one frames later by the same route, because its rule — jump
/// what the reader reports and what the game has refused — fires at the pole exactly as
/// the skill's does. The height gate never fires, because `MAX_WALL_ROWS` caps the
/// reported column at eight rows and the flagpole's twelve are never a number a skill is
/// shown.
///
/// **The exit is load-bearing on `clear-wall` all the same.** Delete that one entry and
/// the same tape, the same objective and the same model come to `Stuck` after 666 frames
/// at absolute X 3155 with ninety-two refusals — a body pressed against a column the
/// ledger has named since the first of them, short of the pole at 3168, short of the
/// level. Delete either of the other two and the verdict is unchanged to the frame, which
/// is what makes the ninety-two refusals an attribution and not a coincidence.
///
/// **And the cap and the deletion are the same measurement.** `MAX_WALL_ROWS` raised from
/// eight to twelve — the honest height of the flagpole, measured once already and
/// reported by `affordances` against the constant — moves this gym to `Stuck` after 666
/// frames at absolute X 3155 with ninety-two refusals, and moves none of the other four.
/// Frame count, position and refusal count identical to the ablation above, because a
/// height gate that fires on the only wall in a gym *is* a deleted skill, and this is the
/// gym where that equivalence shows up. It is why the cap is still eight and why the
/// tall-wall skill is still owed: the level's exit is the one place in 1-1 where the
/// model's wall and the game's wall disagree, and the disagreement is currently paid for
/// by a number chosen so that nothing asks the question.
#[test]
fn the_level_exit_needs_the_wall_skill_and_not_a_taller_one() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let flagpole = gyms::FLAGPOLE_3168;

    let cleared = flagpole
        .run(&rom)
        .expect("the scripted flagpole gym should run");
    assert_eq!(
        cleared.outcome,
        gym::Outcome::Met,
        "the script is expected to finish the level from three hundred pixels out: {cleared:?}"
    );

    let reflex = Gym {
        driver: gym::REFLEX,
        ..flagpole.clone()
    }
    .run(&rom)
    .expect("the same tape under the reflex should run");
    assert_eq!(
        reflex.outcome,
        gym::Outcome::Met,
        "the flagpole is the level's exit and the game takes a body that arrives at it, so \
         the reflex wins here too: {reflex:?} against the scripted {cleared:?}"
    );

    let stalled = Gym {
        driver: minus(CLEAR_WALL.name),
        ..flagpole.clone()
    }
    .run(&rom)
    .expect("the same tape with the wall skill deleted should run");
    assert_eq!(
        stalled.outcome,
        gym::Outcome::Stuck,
        "the level's exit was expected to fail without the wall skill, and it did not: \
         {stalled:?} against the scripted {cleared:?}"
    );
    assert!(
        stalled.furthest_x < 3168,
        "ninety-two refusals is a body that never gets its box past the pole at 3168: \
         furthest absolute X {}",
        stalled.furthest_x,
    );
    assert!(
        stalled.refusals > 10 * cleared.refusals,
        "the ablated run is refused over and over by the column the scripted run is \
         refused once and then jumps into: {stalled:?} vs {cleared:?}"
    );

    for name in [CLEAR_GAP.name, HOP_OBSTACLE.name] {
        let other = Gym {
            driver: minus(name),
            ..flagpole.clone()
        }
        .run(&rom)
        .expect("the same tape with one other skill deleted should run");
        assert_eq!(
            other, cleared,
            "deleting {name} changes this gym's verdict, so the ninety-two refusals above \
             are about the wall skill and nothing else: {other:?} vs {cleared:?}"
        );
    }
}
