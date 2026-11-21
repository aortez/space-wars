//! L5.5: skills — named behaviours with preconditions in affordance terms and an
//! action envelope measured to the frame.
//!
//! A skill is the unit `docs/smb-execution-plan.md` Phase 3 is about, and the reason
//! it is a separate thing from a policy is that a policy is a way of choosing and a
//! skill is a way of *doing*. Phase 5's search will spend skills; a script strings the
//! same skills together by hand; both have to be spending the identical measured
//! behaviour, or a search result says nothing about the library it searched.
//!
//! Two things make a skill rather than a line of code.
//!
//! **An envelope measured from the game.** SMB's jump is variable-height, so "press A"
//! is not an action. [`measure`] replays a tape, presses, and reports the carry, the
//! air, the rise and the headroom the buttons actually bought, which is where every
//! number on every envelope here came from.
//!
//! **A landing it can see.** A jump is a commitment made at one position about another,
//! and the whole content of the pit at world pixel 1376 is that the base policy
//! committed at absolute X 1264 to a landing at 1379 — inside the hole — on the
//! strength of an enemy twenty-two pixels ahead. It never stood on anything again. The
//! base policy's *decision* was right: an enemy was there, and a jump started on that
//! frame is what the measurement says clears it. What it had no way to ask was "where
//! would I come down", and the answer was one cell inside a hole forty-eight pixels
//! wide. So every skill here is gated on the same question — is the box coming down on
//! something the picture shows carrying weight.
//!
//! What that gate is worth turned out to need measuring rather than reasoning about. The
//! ablation in `tests/skills.rs` takes the hop out of the script and the pit is still
//! crossed, from the first frame the far lip is readable: the hole was never the hop's
//! problem, and a script with no enemy skill at all walks past the koomba the base policy
//! flinches at. The hop wins the gym two hundred and eighty-six pixels further along, on
//! a pair of koopas the gym is not named after. So a library is not a story about what a
//! level contains — it is a table of what a measurement says each entry is load-bearing
//! for, and where the answer is "not where you expected", that goes in the file.
//!
//! `None` from the reader — the landing is past the camera's window — is a refusal and
//! not an invitation, which is SMB-019's conclusion written as a precondition: a
//! hazard has to be readable from the picture before contact. A skill that cannot see
//! where it would land does not jump, even if that means he walks into the hole; a hole
//! the picture never shows is a model hole, and Phase 6 is where it gets built.
//!
//! A skill asks affordance vocabulary only, the four questions Phase 0 allows, so
//! [`Situation`] holds answers rather than addresses: no tile id, no address, no
//! pattern hash, nothing that would stop a skill being played on a fabricated level.

use engine_nes::{ControllerButtons, NesMachine};
use smb_model::{
    affordances::{
        ENEMY_TRIGGER_PIXELS, Gap, WALL_LOOK_COLUMNS, Wall, body_last_pixel, carries_weight_at,
        gap_ahead, wall_ahead,
    },
    frame::{PpuBytes, is_blank_tile, player_tile, scroll_x, snapshot, tile_at_world_pixel},
    profile::{SmbState, read_state},
    resistance::{BODY_ROWS, Motion, Refusal, Resistance},
};

use crate::{
    gym::{Driver, GymError, LedgerSeed, Start, Tape},
    policy::{BaseBot, Bot},
};

/// Columns of level a skill reads ahead of itself when it looks for a hole.
///
/// Not a look-ahead in any meaningful sense: at running speed the camera keeps him
/// about a hundred and twenty-nine pixels from the right edge of what it draws, so the
/// picture runs out before this does and [`gap_ahead`] reports the edge it stopped at.
/// The number only has to exceed the longest landing any envelope has, and it does by
/// sixty-four pixels.
const LOOK_COLUMNS: i16 = 24;

/// Rows of cell above his head that a measured jump is asked about.
///
/// The cap is the top of the window the nametable can be read through, so a jump under
/// open sky reports the cap rather than infinity.
const HEADROOM_ROWS: i16 = 12;

/// What one button program does to his body.
///
/// The four numbers are a program's identity, not decoration: a skill that carries
/// sixty-five and a skill that carries one hundred and fifteen are different answers to
/// the same enemy, and the pit at 1376 is the difference between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Envelope {
    /// What the buttons are called in a transcript.
    pub name: &'static str,
    /// Frames A is held for.
    pub hold_frames: u64,
    /// Frames off the ground the hold buys at full running speed.
    pub air_frames: u64,
    /// Pixels of absolute X carried while airborne.
    pub carry_pixels: u16,
    /// Pixels above the surface it left at the apex.
    pub rise_pixels: u16,
    /// Clear air the picture showed above his head on the frame this was measured from.
    ///
    /// A context, not a requirement, and the two come apart: the hop's own measurement
    /// reads thirty-two pixels of headroom at its takeoff and rises thirty-nine through
    /// it, because the ceiling is the brick row he is standing under when the button goes
    /// down and the top of the jump is twenty-odd pixels further along the level where
    /// that row has ended. Nothing in this library gates on the field yet; it is here
    /// because the plan asks an envelope to carry the conditions it was measured under,
    /// and because a jump that stops rising is the one failure this project has already
    /// had twice.
    pub headroom_pixels: u16,
}

impl Envelope {
    /// Where a jump started at `from_x` comes down.
    pub fn landing_x(&self, from_x: u16) -> u16 {
        from_x.saturating_add(self.carry_pixels)
    }

    /// The last world pixel his box occupies on the landing frame.
    ///
    /// The box, not the point: the shortest crossing the commit sweep came down at
    /// absolute X 1414 over a hole whose ground returns at 1424, and it stood because
    /// his box reaches past the position the profile reports. Judging a landing by the
    /// point alone refuses every jump the game allows. Asked at
    /// [`body_last_pixel`] rather than at the exclusive edge, which is where the game
    /// decides a landing: a landing at 1411 stands over that hole and one at 1409 does
    /// not, and the last pixel is the only number that answers both the same way the
    /// cartridge does.
    pub fn landing_last_pixel(&self, from_x: u16) -> u16 {
        body_last_pixel(self.landing_x(from_x))
    }

    /// One frame of the behaviour: A stays down for the whole hold, right stays down
    /// throughout, because a jump taken standing still is a different jump.
    pub fn buttons(&self, frame_in_skill: u64) -> ControllerButtons {
        let mut buttons = ControllerButtons::RIGHT | ControllerButtons::B;
        if frame_in_skill < self.hold_frames {
            buttons |= ControllerButtons::A;
        }
        buttons
    }
}

/// The full-speed jump: A held twenty frames from a running speed of `$28`.
///
/// Measured by [`measure`] from the end of the `pit-1379` tape, twenty-eight frames in,
/// which is the first frame this library can see a landing over that hole. He leaves the
/// ground on the frame after a last grounded frame at absolute X 1296, is airborne
/// forty-six frames, comes down one hundred and eighteen pixels further and rises
/// seventy-five under ninety-six pixels of clear air, which is the longest carry any
/// skill here has.
///
/// The carry is a range in the wild and a number here. Re-measured across the whole
/// commit window the same twenty frames of A carry 118 from the frame this envelope is
/// pinned to, 117 one frame earlier, and a flat 115 from every frame from thirty on:
/// one frame of flight either way, which is three pixels of landing. The envelope names
/// the frame it was measured from for that reason, and the near edge of the rule that
/// reads it is a pixel or two soft as a result — soft in the direction that matters,
/// since a jump the model thinks reaches and the game returns is a fall. On this tape
/// the softness is unreachable: the rule's own edge is absolute X 1293 and the far lip
/// does not come into the camera's window until 1296, so the first frame the skill is
/// allowed to ask the question is the first frame the game scores as a crossing.
pub const LONG_JUMP: Envelope = Envelope {
    name: "long-jump",
    hold_frames: 20,
    air_frames: 46,
    carry_pixels: 118,
    rise_pixels: 75,
    headroom_pixels: 96,
};

/// The hop: A held four frames from the same running speed.
///
/// Also measured, from the same tape fifteen frames in — the frame the enemy reaches
/// twenty-two pixels ahead of him. It carries sixty-five pixels, a hundred and fifty
/// fewer than the long jump would, and that is the entire reason it exists: from that
/// frame the long jump's landing box falls inside the hole at 1376 and this one's
/// falls short of it, at 1342 — inside the lip. Thirty-nine pixels of rise is more than
/// enough over a koomba, and twenty-five frames of air is more than enough time.
///
/// Its headroom reads thirty-two and its rise is thirty-nine, which is the sweep's
/// lesson arriving inside the library: he takes that hop from under the brick row, and
/// by the top of it he is twenty pixels past the row's end. The number is recorded
/// because a hop taken somewhere with a ceiling over the *apex* is a different jump, and
/// the only way to know that from the picture is to ask before pressing.
pub const HOP: Envelope = Envelope {
    name: "hop",
    hold_frames: 4,
    air_frames: 25,
    carry_pixels: 65,
    rise_pixels: 39,
    headroom_pixels: 32,
};

/// The picture a skill is allowed to look at.
///
/// Body state and affordance answers, which is the whole question set Phase 0 allows.
/// Built by [`Skill::look`] rather than by a caller so that a script, a test and
/// Phase 5's search all see the identical reading, and so that the landing box in
/// [`Situation::landing_is_solid`] is the one belonging to the skill doing the looking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Situation {
    /// Absolute X, the position an envelope's carry is measured from.
    pub x: u16,
    /// The feet row the affordances were read on.
    pub row: i16,
    pub grounded: bool,
    pub in_control: bool,
    /// The hole ahead, named by the pixels that bound it.
    pub gap: Option<Gap>,
    /// Whether the picture shows this skill's landing box coming down on something that
    /// carries weight.
    ///
    /// Asked two ways, in `look_at`, because at running speed the direct question
    /// cannot always be asked: the pixel under the box is read first, and where the
    /// picture has run out past its own right edge the answer comes from the hole
    /// reading instead, which reports the pixel the ground comes back at and whether it
    /// saw that pixel at all. `false` still covers "it does not" and "nothing can be
    /// said", and both are refusals.
    pub landing_is_solid: bool,
    /// Pixels to the nearest enemy ahead.
    pub enemy_pixels: Option<i16>,
    /// The first world pixel ahead the model calls solid in the cells his body sweeps
    /// into, from the load-bearing set or from the ledger, and `None` when nothing in
    /// reach is.
    ///
    /// A pixel rather than a bit, because a postcondition is judged against a pixel.
    /// `hop-obstacle` is about getting past *this* cell, and with no cell in front of
    /// the body to be past, its claim was already true on the frame before the jump was
    /// taken: the skill cancelled its own start and refused every wall in the level,
    /// which a full-level run found at the first pipe.
    pub wall: Option<Wall>,
}

impl PartialEq for Skill {
    /// Two of these are the same behaviour when they are called the same thing.
    ///
    /// Written by hand rather than derived because the rules are function pointers, and
    /// Rust says plainly that comparing their addresses means nothing.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Skill {}

/// Whether a skill's precondition holds, and whether it might hold shortly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// Start the behaviour on this frame.
    Start,
    /// Not this skill's frame yet, and the situation is still this skill's.
    ///
    /// A commit window has a near edge, and a skill that cannot say "not yet" spends
    /// its jump on the first frame anything looked like a reason — which is the base
    /// policy. A caller reads `Wait` as "keep looking", not as "hold the frame": the
    /// frame still has an answer, and it may belong to another skill.
    Wait,
    /// This situation is no longer this skill's to handle.
    None,
}

/// A named behaviour: what it requires of the picture, what it claims afterwards, and
/// the buttons that are the behaviour.
///
/// The four rules are function pointers rather than a trait so that the library stays a
/// `const` table a search can index and a transcript can name. It is `Debug` and not
/// `PartialEq`: two of these are the same skill when they have the same name, and a
/// comparison of function addresses is not a comparison of rules.
#[derive(Clone, Copy, Debug)]
pub struct Skill {
    pub name: &'static str,
    pub envelope: Envelope,
    precondition: fn(&Situation, &Envelope) -> bool,
    /// Whether the skill still owns this situation while its precondition is unmet.
    waiting: fn(&Situation, &Envelope) -> bool,
    /// The pixel a situation is about, which the postcondition is later judged against.
    past: fn(&Situation) -> u16,
    postcondition: fn(&Situation, u16) -> bool,
}

impl Skill {
    /// The machine as this skill sees it, ledger included: the same reading every
    /// caller gets, with this envelope's landing box already answered for.
    pub fn look(&self, machine: &NesMachine, ledger: &Resistance) -> Situation {
        look_at(machine, &self.envelope, ledger)
    }

    /// Whether this frame is the frame to start, still this skill's problem but not yet
    /// its frame, or neither.
    ///
    /// The postcondition is asked first, and it can cancel the start: a behaviour whose
    /// claim the world has already granted is not a behaviour worth pressing. This is
    /// not decoration. A long jump that comes down with one foot on the far lip leaves
    /// him standing at absolute X 1414 over a hole whose ground returns at 1424 —
    /// standing to the game and in the hole to the map, which is SMB-017's finding about
    /// the body rather than about the artwork — and the hole reading from there is a
    /// nine-pixel hole still ahead of him. Without this check the script asked for a
    /// second long jump on the frame it landed, from inside the hole it had just
    /// crossed, and the only reason that was survivable is that the jump happened to
    /// carry. A skill that cannot tell a hole ahead from the hole it is standing in is
    /// also a skill Phase 5's search will spend twice for no reason.
    pub fn call(&self, at: &Situation) -> Call {
        if (self.postcondition)(at, (self.past)(at)) {
            Call::None
        } else if (self.precondition)(at, &self.envelope) {
            Call::Start
        } else if (self.waiting)(at, &self.envelope) {
            Call::Wait
        } else {
            Call::None
        }
    }

    /// The world pixel this situation is about, which is the pixel a postcondition is
    /// later judged against. Affordance vocabulary asks for it: "which affordance holds
    /// right now, naming the cell it is about".
    pub fn past(&self, at: &Situation) -> u16 {
        (self.past)(at)
    }

    /// What the game has to have done for the behaviour to have worked.
    pub fn achieved(&self, at: &Situation, past: u16) -> bool {
        (self.postcondition)(at, past)
    }

    /// The buttons for the frame `frame_in_skill` of a run of this behaviour.
    pub fn buttons(&self, frame_in_skill: u64) -> ControllerButtons {
        self.envelope.buttons(frame_in_skill)
    }
}

/// Read the machine into the questions a skill asks.
///
/// One function for every skill and every caller, so that "the model saw it" means one
/// thing. The landing box is asked of [`carries_weight_at`] at the envelope's own reach,
/// which is the only place the two envelopes in the library genuinely differ.
fn look_at(machine: &NesMachine, envelope: &Envelope, ledger: &Resistance) -> Situation {
    let state = read_state(machine);
    let frame = snapshot(machine);
    let origin_x = scroll_x(&frame);
    let row = player_tile(&state, origin_x).1;
    let enemy_pixels = state
        .enemies(machine)
        .iter()
        .filter(|enemy| enemy.delta_x > 0)
        .map(|enemy| enemy.delta_x)
        .min();
    let gap = gap_ahead(&frame, origin_x, state.absolute_x, row, LOOK_COLUMNS);
    let landing = envelope.landing_last_pixel(state.absolute_x);
    Situation {
        x: state.absolute_x,
        row,
        grounded: state.is_grounded(),
        in_control: state.is_in_control(),
        gap,
        landing_is_solid: landing_is_solid(&frame, origin_x, row, landing, gap),
        enemy_pixels,
        wall: wall_ahead(&frame, origin_x, &state, ledger, WALL_LOOK_COLUMNS),
    }
}

/// Whether the box comes down on ground, from every source the picture can answer with.
///
/// The direct question first: one pixel, the last one the box occupies, is it over
/// something load-bearing. That is the only source for a landing with no hole in it, and
/// it is what a hop's landing at seventy-eight pixels gets.
///
/// It is not available to a long jump. The camera pins him at screen column 126 or 127
/// once he is at running speed and the reader reaches thirty-two columns, so the
/// picture's right edge is one hundred and twenty-eight or one hundred and twenty-nine
/// pixels past his position, and the long jump's box comes down at one hundred and
/// thirty-one. Measured, not reasoned: the same probe at one hundred and twenty-nine
/// alternates between an answer and none as the fine scroll moves, which is the last
/// readable pixel flickering. A skill that asked only the direct question would refuse
/// every long jump this game allows, and the first version of this library did exactly
/// that and walked into the pit.
///
/// So where the probe is off the picture the answer comes from the hole instead. A hole
/// whose far edge the picture showed has told us where the ground comes back, and a box
/// that reaches past that pixel is on ground — that is the same reading, from a pixel
/// inside the window rather than two outside it. `false` is still what a hole with no
/// visible far side gets, which is SMB-019's rule and the reason the base policy is not
/// simply allowed to guess.
fn landing_is_solid(
    frame: &PpuBytes,
    origin_x: u16,
    row: i16,
    landing: u16,
    gap: Option<Gap>,
) -> bool {
    match carries_weight_at(frame, origin_x, landing, row) {
        Some(solid) => solid,
        None => gap.is_some_and(|gap| gap.far_edge_seen && landing >= gap.far_x),
    }
}

/// Standing, on the far side of the pixel the skill was started about.
///
/// Deliberately modest: it claims the game let him get past, not that the behaviour
/// looked the way it was meant to. Whether the jump was the right one is the gym's
/// verdict, not the skill's.
fn standing_past(at: &Situation, past: u16) -> bool {
    at.grounded && body_last_pixel(at.x) >= past
}

/// Jump a hole the model can see, and land on the side of it the model can see.
///
/// Precondition: standing, in control, a hole ahead whose far edge the picture showed,
/// and this envelope's landing box past that edge on ground the picture also showed.
///
/// The near edge matters as much as the far one. The commit window the sweep measured
/// is thirty-one frames wide and its first frame is the first frame the far lip is
/// inside the camera's window at all — the picture ends about where a long jump lands,
/// so a longer look is not available to anything playing this game from the screen.
/// Before that frame this skill answers [`Call::Wait`], and a policy that answered
/// `Start` to the earlier frames is the base policy, which jumped at absolute X 1264 at
/// an enemy and came down in the hole.
pub const CLEAR_GAP: Skill = Skill {
    name: "clear-gap",
    envelope: LONG_JUMP,
    precondition: |at, envelope| {
        at.grounded
            && at.in_control
            && at.landing_is_solid
            && at.gap.is_some_and(|gap| {
                gap.far_edge_seen && envelope.landing_last_pixel(at.x) >= gap.far_x
            })
    },
    waiting: |at, envelope| {
        at.grounded
            && at.in_control
            && at.gap.is_some_and(|gap| {
                !gap.far_edge_seen || envelope.landing_last_pixel(at.x) < gap.far_x
            })
    },
    past: |at| at.gap.map_or(at.x, |gap| gap.far_x),
    postcondition: standing_past,
};

/// Jump an enemy in reach with the same landing rule as every other skill and an
/// envelope short enough to stop short of the hole beyond it.
///
/// This is the base policy's reflex with the one thing it was missing. The reflex asked
/// "is there a reason to jump" and acted at once; this asks that and "does the picture
/// show where I would come down", and answers with the hop rather than the long jump
/// because the hop is the program whose landing the picture can vouch for.
///
/// Enemies only, and that split is a measurement and not a tidy-up: the hop has been
/// measured clearing a koomba and has never been measured clearing a wall, and the
/// first full-level run under this script stopped at the first pipe with the hop's
/// precondition satisfied on every frame of it. A four-frame hold reaches its top long
/// after it reaches the pipe in front of it, so the pipe is [`CLEAR_WALL`]'s, which is
/// the program this project has filmed clearing that pipe.
pub const HOP_OBSTACLE: Skill = Skill {
    name: "hop-obstacle",
    envelope: HOP,
    precondition: |at, _| {
        at.grounded
            && at.in_control
            && at.landing_is_solid
            && at
                .enemy_pixels
                .is_some_and(|delta| delta <= ENEMY_TRIGGER_PIXELS)
    },
    waiting: |_, _| false,
    past: nearest_obstacle,
    postcondition: standing_past,
};

/// The pixel the obstacle situation is about: whichever of the wall and the enemy is
/// nearer, because that is the one a jump has to clear first.
///
/// `at.x` when neither is ahead, which makes [`standing_past`] true and the skill's
/// [`Call`] `None`: with nothing in front of the body there is no situation to own.
fn nearest_obstacle(at: &Situation) -> u16 {
    at.enemy_pixels
        .and_then(|delta| u16::try_from(i32::from(at.x) + i32::from(delta)).ok())
        .unwrap_or(at.x)
}

/// Jump a wall the model can see, with the envelope tall enough to get over the top of
/// it and a landing past the far side.
///
/// The height is the whole reason this is a separate skill from [`HOP_OBSTACLE`] rather
/// than a bigger hold inside it: an envelope is a measured program, and picking one by
/// how tall the thing in front is only means anything if the number came from the
/// picture. [`Wall::height_pixels`] is counted from what draws, so a pipe body the
/// load-bearing set has never been shown still reports its height.
///
/// The rise term is a refusal and not a formality. A wall taller than this envelope's
/// measured rise is not jumpable from where he is standing, and a skill that says so
/// leaves the situation to a skill that does not exist yet instead of spending a jump
/// that ends with him pressed against the same column.
///
/// **This is the one skill that does not ask [`Situation::landing_is_solid`], and it is
/// not an oversight.** The gate exists to stop a jump committed to a landing the picture
/// cannot vouch for, and at a wall the picture cannot vouch for the long jump's box at
/// all: the wall is fourteen to forty pixels ahead and the box comes down one hundred and
/// thirty-one ahead, two pixels past the last column the PPU is drawing (SMB-021 measured
/// that gap from the other side). The fallback that saves a long jump over a hole — a hole
/// whose far lip the picture showed — is no use either, because the cells of an untried
/// wall are not in the load-bearing set and the hole reading calls the whole of one a
/// sixty-four-pixel hole with no far side. The first version of this skill carried the
/// gate anyway and stopped at the stairs at absolute X 2162, refusing a two-row step it
/// could see, could name and could easily clear.
///
/// So the landing claim here is the wall. `wall_ahead` reports a column only when the
/// load-bearing set has held a body on it or the ledger was refused by it, which is
/// already a claim about solidity, and a cell that stops a body sideways is the same
/// collision that holds one up: the top of a wall the model can see is a surface, and the
/// height term is what says this envelope can reach it. What that gives up is the question
/// *behind* the wall — a hole just past the far side sits in the two pixels the picture
/// does not have — and that is the same risk the base reflex has always taken and filmed,
/// not a new one. A pipe with a hole directly behind it is the gym that would price it.
pub const CLEAR_WALL: Skill = Skill {
    name: "clear-wall",
    envelope: LONG_JUMP,
    precondition: |at, envelope| {
        at.grounded
            && at.in_control
            && at.wall.is_some_and(|wall| {
                wall.height_pixels() <= envelope.rise_pixels && body_last_pixel(at.x) < wall.x
            })
    },
    waiting: |at, _| at.grounded && at.in_control && at.wall.is_some(),
    past: |at| at.wall.map_or(at.x, |wall| wall.x),
    postcondition: standing_past,
};

/// The script Phase 3 asks for: the skills in the order they get first refusal.
///
/// A gap the model can see a landing over outranks an obstacle, because the long jump
/// that crosses it also carries him over anything standing in the hole's approach. The
/// order is the whole arbitration: a skill that answers [`Call::Wait`] does not stop
/// the frame, it passes it on.
///
/// The order has a measured cost, and it is not zero. On the pit tape the long jump is
/// only waiting on frame 1055 — the far lip is off the picture — and the hop takes the
/// frame, so the crossing is committed from 1330 instead of the 1296 the long-jump-only
/// script gets. Both are inside the window the game scored, so the order costs nothing
/// *here*, and it is exactly the kind of thing that costs a window three frames wide
/// somewhere else. A script is a policy, and a policy can be wrong in both directions.
pub const SCRIPT: &[&Skill] = &[&CLEAR_GAP, &CLEAR_WALL, &HOP_OBSTACLE];

/// The script, as a gym driver: the same bot the harness builds, under a name a verdict
/// can be printed beside.
pub fn scripted(bot: BaseBot) -> Box<dyn Bot> {
    Box::new(ScriptedBot::new(bot))
}

/// [`scripted`] with a name attached, which is what a gym holds.
pub const SCRIPTED: Driver = Driver {
    name: "script",
    player: scripted,
};

/// A skill in progress, kept so that a transcript and a test can name the frame a
/// behaviour started on and the pixel it was started to get past.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Running {
    pub skill: &'static Skill,
    /// The frame id the first frame of the program was issued on.
    pub started_at: u64,
    /// Absolute X on that frame, which is where [`Envelope::landing_last_pixel`] was
    /// read from, and the position a measured commit window is quoted in.
    pub from_x: u16,
    /// The pixel [`Skill::achieved`] is judged against.
    pub past: u16,
    /// Frames of the program issued so far.
    pub elapsed: u64,
}

/// The scripted bot: the script above, over the ledger the base bot keeps.
///
/// It is a [`crate::policy::Bot`] and nothing more, so the gym harness runs it exactly
/// as it runs the base policy and the two differ in the library behind `buttons` and
/// nowhere else. It keeps a [`BaseBot`] inside for the ledger rather than reimplementing
/// the learning: a scripted bot that has not been refused anything is a different bot
/// from one that has, and Phase 1 measured that difference.
#[derive(Debug)]
pub struct ScriptedBot {
    base: BaseBot,
    script: &'static [&'static Skill],
    running: Option<Running>,
    asked: Option<Motion>,
    started: Vec<Running>,
}

impl Default for ScriptedBot {
    fn default() -> Self {
        Self::new(BaseBot::default())
    }
}

impl ScriptedBot {
    /// The script, handed a model with whatever it has already been refused.
    pub fn new(base: BaseBot) -> Self {
        Self::with_script(base, SCRIPT)
    }

    /// The same bot with a library of its own, which is how a skill gets asked whether
    /// it is load-bearing: run the gym with the script and without one entry in it and
    /// the difference is that skill's contribution, measured in frames and outcomes on
    /// a tape neither of them chose.
    pub fn with_script(base: BaseBot, script: &'static [&'static Skill]) -> Self {
        Self {
            base,
            script,
            running: None,
            asked: None,
            started: Vec::new(),
        }
    }

    /// The behaviour in progress, if one is.
    pub fn running(&self) -> Option<Running> {
        self.running
    }

    /// Every behaviour this bot has started, in order, with the frame and position each
    /// started on. This is what a test compares against a measured commit window.
    pub fn started(&self) -> &[Running] {
        &self.started
    }

    /// The model's memory of solidity, which is what the skills read.
    pub fn ledger(&self) -> &Resistance {
        &self.base.ledger
    }

    fn start(&mut self, skill: &'static Skill, at: &Situation, at_frame: u64) -> ControllerButtons {
        let running = Running {
            skill,
            started_at: at_frame,
            from_x: at.x,
            past: skill.past(at),
            elapsed: 1,
        };
        self.running = Some(running);
        self.started.push(running);
        skill.buttons(0)
    }
}

impl Bot for ScriptedBot {
    fn buttons(&mut self, machine: &NesMachine) -> ControllerButtons {
        let at_frame = machine.ppu().frame_id();
        let buttons = match self.running {
            // A program's hold is the envelope, so it runs to its measured length and
            // nothing interrupts it: an envelope measured as twenty frames of A and
            // issued as eleven is a different envelope.
            Some(mut running) if running.elapsed < running.skill.envelope.hold_frames => {
                running.elapsed += 1;
                self.running = Some(running);
                running.skill.buttons(running.elapsed - 1)
            }
            Some(_) => {
                self.running = None;
                self.choose(machine, at_frame)
            }
            None => self.choose(machine, at_frame),
        };
        self.asked = Some(Motion::asked(&read_state(machine), buttons));
        buttons
    }

    fn observe(
        &mut self,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal> {
        let asked = self.asked.take()?;
        self.learn(asked, produced, machine, at_frame)
    }

    fn learn(
        &mut self,
        asked: Motion,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal> {
        self.base.learn(asked, produced, machine, at_frame)
    }

    fn ledger(&self) -> &Resistance {
        ScriptedBot::ledger(self)
    }

    fn started(&self) -> &[Running] {
        ScriptedBot::started(self)
    }
}

impl ScriptedBot {
    /// The first skill whose precondition holds takes the frame.
    fn choose(&mut self, machine: &NesMachine, at_frame: u64) -> ControllerButtons {
        for skill in self.script {
            let at = skill.look(machine, self.ledger());
            if skill.call(&at) == Call::Start {
                return self.start(skill, &at, at_frame);
            }
        }
        ControllerButtons::RIGHT | ControllerButtons::B
    }
}

/// What a program did when the game ran it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measured {
    /// Absolute X on the last frame he stood on anything before the press.
    pub from_x: u16,
    /// Pixels of absolute X between that frame and the frame he stood on anything again.
    pub carry_pixels: u16,
    /// Frames between those two.
    pub air_frames: u64,
    /// Pixels above the takeoff surface at the apex.
    pub rise_pixels: u16,
    /// Clear air above his head on the takeoff frame.
    pub headroom_pixels: u16,
    /// Whether he stood on anything again inside the budget.
    pub landed: bool,
}

impl Measured {
    /// The envelope these numbers describe, named after the program that made them.
    pub fn envelope(&self, name: &'static str, hold_frames: u64) -> Envelope {
        Envelope {
            name,
            hold_frames,
            air_frames: self.air_frames,
            carry_pixels: self.carry_pixels,
            rise_pixels: self.rise_pixels,
            headroom_pixels: self.headroom_pixels,
        }
    }
}

/// Frames after the press to wait for him to come down.
const MEASURE_BUDGET: u64 = 200;

/// Run a program from the end of a tape and report what the game did with it.
///
/// `run_up` frames of plain running, then A held for the envelope's hold, then running
/// until he stands on something again. The run-up is counted in frames rather than read
/// from a bot because an envelope belongs to a button program and not to a player: the
/// same twenty frames of A measured from a different position in the level is a
/// different claim, which is why every number on an envelope here names the tape and the
/// frame it was measured from.
///
/// The tape is replayed from power-on, so this is the same machine a gym starts from and
/// the same buttons a gym would press, and it costs a replay per sample.
pub fn measure(
    rom: &[u8],
    tape: &Tape,
    envelope: &Envelope,
    run_up: u64,
) -> Result<Measured, GymError> {
    let Start { mut machine, .. } = tape.replay(rom, LedgerSeed::Cold)?;
    for _ in 0..run_up {
        machine.run_frame([
            ControllerButtons::RIGHT | ControllerButtons::B,
            ControllerButtons::NONE,
        ])?;
    }
    let opened = read_state(&machine);
    let mut from_x = opened.absolute_x;
    let mut top = i16::from(opened.screen_y);
    let mut headroom = 0u16;
    let mut airborne = false;
    let mut air_frames = 0u64;
    let mut landed = false;
    for elapsed in 0..MEASURE_BUDGET {
        machine.run_frame([envelope.buttons(elapsed), ControllerButtons::NONE])?;
        let state = read_state(&machine);
        if state.is_grounded() {
            if airborne {
                landed = true;
                break;
            }
            from_x = state.absolute_x;
            continue;
        }
        if !airborne {
            airborne = true;
            headroom = headroom_pixels(&machine, &state);
        }
        air_frames += 1;
        top = top.min(i16::from(state.screen_y));
    }
    let landed_state = read_state(&machine);
    Ok(Measured {
        from_x,
        carry_pixels: if landed {
            landed_state.absolute_x.saturating_sub(from_x)
        } else {
            0
        },
        air_frames,
        rise_pixels: u16::try_from(i32::from(opened.screen_y) - i32::from(top)).unwrap_or(0),
        headroom_pixels: headroom,
        landed,
    })
}

/// Clear air above his head, in pixels, over the rows a body two tiles tall can hit.
///
/// The rule `limit` measures with: a ceiling is a cell that draws something, since the
/// question is "did this jump get stopped", and a cell the lookup cannot answer for —
/// past the camera's window — counts as clear because an unreadable row has never
/// stopped a jump.
fn headroom_pixels(machine: &NesMachine, state: &SmbState) -> u16 {
    let frame = snapshot(machine);
    let origin_x = scroll_x(&frame);
    let row = player_tile(state, origin_x).1;
    for up in 0..HEADROOM_ROWS {
        let drawn = tile_at_world_pixel(&frame, origin_x, state.absolute_x, row - BODY_ROWS - up)
            .is_some_and(|cell| !is_blank_tile(&frame, cell));
        if drawn {
            return u16::try_from(up * 8).unwrap_or(0);
        }
    }
    u16::try_from(HEADROOM_ROWS * 8).unwrap_or(0)
}
