//! The gym harness: a start state, an objective, a frame budget and a verdict.
//!
//! This is an instrument and not a rung. The mission's L6 is the route, and no route
//! exists yet; what lives here is the thing that will measure one when it does, which
//! is why it was built first and why it knows nothing about skills.
//!
//! The harness exists before the skills do, because it is the instrument and
//! instruments are built before the things they measure. Without it every question
//! about one pipe costs a whole level run; with one, a question about a pipe costs
//! the frames between the tape and the pipe.
//!
//! A committed gym stores a **tape**, not a savestate: the cartridge's identity, the
//! buttons that were actually pressed from power-on, and the frame the gym starts
//! on. The state is re-derived by replaying it. A savestate silently mismatches
//! exactly when the core changes, which is precisely when a gym is worth having; a
//! tape fails loudly instead, and [`GymError::Drifted`] names the two hashes that
//! disagree.
//!
//! Phase 1 split "start state" into two things, and both are pinned here. The tape
//! pins the machine's frame. [`LedgerSeed`] pins what the model remembers of having
//! been refused, which is host-side state no checkpoint can hold: restoring a frame
//! and handing over a cold ledger is a different gym from restoring the same frame
//! with the lessons an earlier run learned, and the difference between the two is
//! the experiment.
//!
//! Budgets are in frames, never in wall clock. This machine is usually loaded, and a
//! verdict that depends on how loaded it was is not a verdict.

use std::{borrow::Cow, error::Error, fmt};

use engine_nes::{CartridgeIdentity, ControllerButtons, MachineError, NesMachine, StateHash};
use smb_model::cartridge::{self, Unusable};
use smb_model::profile::{
    MODE_END_GAME, SmbState, boot_to_gameplay, read_state, setup_buttons, wait_for_handover,
};
use smb_model::resistance::Motion;

use crate::policy::{BaseBot, Bot};

/// Which of the two start states a gym carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerSeed {
    /// Nothing has been refused yet. The model meets the obstacle for the first
    /// time, which is the only way to find out whether it can be met.
    Cold,
    /// Every refusal the tape's own run made, rebuilt by watching the replay. The
    /// model arrives having already been told, which is what a bot with a memory of
    /// its own actually looks like.
    Warm,
}

/// Who plays a gym, given the model memory the tape brought.
///
/// A named function pointer rather than a trait object in the `Gym`, because a gym is a
/// `const` that a table of gyms holds and a test iterates, and because a verdict printed
/// beside no name is a number nobody can re-run. The seed still arrives through the
/// argument: the tape decides what the model remembers, and this decides what the player
/// does about it. Phase 3's script and Phase 5's search are two of these beside each
/// other on the same tape, which is the comparison the mission asks for.
#[derive(Clone, Copy, Debug)]
pub struct Driver {
    /// What a transcript calls this player.
    pub name: &'static str,
    pub player: fn(BaseBot) -> Box<dyn Bot>,
}

/// The reflex, for a gym that is measuring the reflex.
pub fn reflex(bot: BaseBot) -> Box<dyn Bot> {
    Box::new(bot)
}

/// [`reflex`] with a name attached, which is what a gym holds.
pub const REFLEX: Driver = Driver {
    name: "reflex",
    player: reflex,
};

/// A recorded run-up: the buttons that get a gym to where it starts.
///
/// The recording is of *buttons*, not of a policy, so a gym keeps meaning the same
/// run even after the policy that produced it changes. It is stored as run-length
/// pairs because a held button is the unit: a bot holds right for hundreds of frames
/// at a time, and a tape of one entry per frame would be a list of numbers nobody
/// can read.
#[derive(Clone, Debug)]
pub struct Tape {
    /// The image the recording was made on. The same buttons on a different
    /// cartridge are a different game's run.
    pub cartridge: CartridgeIdentity,
    /// Button latch value held for this many frames, from power-on.
    ///
    /// Borrowed for a committed tape, owned for one that [`Gym::record`] has only
    /// just made, so the recorder can replay and verify the tape it is about to
    /// print rather than printing something nothing has run.
    pub runs: Cow<'static, [(u8, u32)]>,
    /// The frame id the gym starts on. Recorded rather than derived so that a tape
    /// truncated by a careless edit fails as [`GymError::ShortTape`] instead of
    /// quietly starting somewhere else.
    pub frame: u64,
    /// The machine state at [`Tape::frame`], so a core change that moves the game is
    /// caught here rather than becoming a mysterious failure two hundred frames on.
    pub golden_hash: StateHash,
}

impl Tape {
    /// Frames the recording covers.
    pub fn played(&self) -> u64 {
        self.runs.iter().map(|(_, count)| u64::from(*count)).sum()
    }

    /// Replay the recording and hand back the machine as it stood at the end of it,
    /// with a bot seeded as the gym asked.
    pub fn replay(&self, rom: &[u8], seed: LedgerSeed) -> Result<Start, GymError> {
        let played = self.played();
        if played != self.frame {
            return Err(GymError::ShortTape {
                played,
                frame: self.frame,
            });
        }
        let mut machine = cartridge::machine(rom)?;
        let actual = machine.cartridge_identity();
        if actual != self.cartridge {
            return Err(GymError::WrongCartridge {
                expected: self.cartridge,
                actual,
            });
        }
        let mut bot = BaseBot::default();
        let mut frame = 0u64;
        for &(mask, count) in self.runs.iter() {
            let buttons = ControllerButtons::from_bits(mask);
            for _ in 0..count {
                // A warm replay has to watch the game refuse things as they happen,
                // which means knowing what each frame's buttons were asking for. A
                // cold one does not, and skipping the read keeps the replay cheap.
                let asked = (seed == LedgerSeed::Warm)
                    .then(|| Motion::asked(&read_state(&machine), buttons));
                let at_frame = machine.ppu().frame_id();
                machine.run_frame([buttons, ControllerButtons::NONE])?;
                frame += 1;
                if let Some(asked) = asked {
                    let produced = read_state(&machine);
                    bot.learn(asked, &produced, &machine, at_frame);
                }
            }
        }
        let actual = machine.state_hash();
        if actual != self.golden_hash {
            return Err(GymError::Drifted {
                expected: self.golden_hash,
                actual,
            });
        }
        Ok(Start {
            machine,
            bot,
            frame,
        })
    }
}

/// Where a gym begins: the machine at the end of the tape, and the model memory it
/// is handed.
#[derive(Debug)]
pub struct Start {
    pub machine: NesMachine,
    pub bot: BaseBot,
    /// The frame id the tape ended on, for a transcript that wants real frame ids.
    pub frame: u64,
}

/// What a gym's run has to achieve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objective {
    /// Get his absolute X to at least this far.
    ReachX { at_least: u16 },
    /// Finish the level, which the game says by moving on from the world and level
    /// the gym started on.
    ClearLevel,
}

impl Objective {
    /// Whether this state satisfies the objective.
    ///
    /// `started_at` is the `(world, level)` the run began in. Only [`Objective::ClearLevel`]
    /// reads it, and it needs it because "finished" is a change of place rather than a
    /// position.
    pub fn held(&self, state: &SmbState, started_at: (u8, u8)) -> bool {
        match self {
            Self::ReachX { at_least } => state.absolute_x >= *at_least,
            Self::ClearLevel => (state.world, state.level) != started_at,
        }
    }
}

impl fmt::Display for Objective {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReachX { at_least } => write!(f, "reach absolute X {at_least}"),
            Self::ClearLevel => write!(f, "finish the level"),
        }
    }
}

/// How a gym's run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The objective held.
    Met,
    /// The budget ran out with the objective unmet and nothing else decided.
    TimedOut,
    /// The game took a life.
    Died,
    /// The game took the last one and took the controller back.
    GameOver,
    /// He was still in play and absolute X had stopped moving. The camera limit
    /// reads as this, which is the whole reason the variant exists: a stop the model
    /// cannot attribute to a cell is a result, not an absence of one.
    Stuck,
}

/// What one gym run did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub outcome: Outcome,
    /// Frames run after the tape, which is the only part of the cost a gym should be
    /// measured in.
    pub frames: u64,
    pub furthest_x: u16,
    /// Refusals the game made during the gym, not counting what the tape brought.
    pub refusals: usize,
}

/// A start state, an objective, a budget and the last measured verdict.
#[derive(Clone, Debug)]
pub struct Gym {
    pub name: &'static str,
    pub objective: Objective,
    pub seed: LedgerSeed,
    pub tape: Tape,
    /// The player this gym measures. Two gyms that differ only here are the same start
    /// state asked of two players, which is the only comparison that says a library is
    /// worth anything.
    pub driver: Driver,
    /// Frames after the tape.
    pub frame_budget: u64,
    /// Frames without a new furthest absolute X before the run is called
    /// [`Outcome::Stuck`]. Short enough to catch a stop, long enough that a jump, a
    /// koomba walking into him and a ledge he reads as a hole each cost a few frames
    /// and prove nothing on their own.
    pub stall_frames: u64,
    /// The same run's result the last time it was measured. A gym without this drifts:
    /// the verdict becomes a number nobody compares, and the only way to notice a
    /// regression is to remember what it used to say.
    pub recorded: Verdict,
}

impl Gym {
    /// Replay the tape and run the base policy against the objective.
    pub fn run(&self, rom: &[u8]) -> Result<Verdict, GymError> {
        self.run_with(&self.tape, rom)
    }

    /// The same run from a tape other than the committed one, which is how
    /// [`Gym::record`] proves a freshly cut tape before anything commits it.
    pub fn run_with(&self, tape: &Tape, rom: &[u8]) -> Result<Verdict, GymError> {
        let Start {
            mut machine,
            bot,
            frame: _,
        } = tape.replay(rom, self.seed)?;
        let mut player = (self.driver.player)(bot);
        let opened = read_state(&machine);
        let started_at = (opened.world, opened.level);
        let lives = opened.lives;
        let mut furthest_x = opened.absolute_x;
        let mut no_progress_for = 0u64;
        let mut refusals = 0usize;
        let mut outcome = Outcome::TimedOut;
        let mut frames = 0u64;
        for elapsed in 1..=self.frame_budget {
            frames = elapsed;
            let at_frame = machine.ppu().frame_id();
            let state = read_state(&machine);
            if self.objective.held(&state, started_at) {
                outcome = Outcome::Met;
                break;
            }
            if state.game_mode == MODE_END_GAME {
                outcome = Outcome::GameOver;
                break;
            }
            if state.lives < lives {
                outcome = Outcome::Died;
                break;
            }
            if state.absolute_x > furthest_x {
                furthest_x = state.absolute_x;
                no_progress_for = 0;
            } else {
                no_progress_for += 1;
                if no_progress_for >= self.stall_frames {
                    outcome = Outcome::Stuck;
                    break;
                }
            }
            let buttons = player.buttons(&machine);
            machine.run_frame([buttons, ControllerButtons::NONE])?;
            let produced = read_state(&machine);
            if player.observe(&produced, &machine, at_frame).is_some() {
                refusals += 1;
            }
        }
        Ok(Verdict {
            outcome,
            frames,
            furthest_x,
            refusals,
        })
    }

    /// Cut a new tape for this gym by running the base policy from power-on to
    /// `until`, and measure the verdict that start state produces.
    ///
    /// The run-up is the setup script the profile already defines, then the reflex,
    /// because a tape that starts at the handover would be a tape whose first three
    /// hundred frames nobody recorded: replaying it from power-on would press the
    /// policy's buttons where the game was still expecting a Start press and land
    /// somewhere else entirely.
    ///
    /// The run-up is always the reflex's, whatever [`Gym::driver`] this gym measures.
    /// That is on purpose: a tape cut by the player being tested would start the gym
    /// past the frames that player is supposed to solve, and a gym whose start state
    /// already contains its own answer measures nothing.
    ///
    /// The tape is replayed once before it is handed back. That costs a second
    /// run-up and buys the only check that matters at this moment: the bytes that
    /// are about to be committed are the bytes that produced the verdict printed
    /// beside them.
    pub fn record(&self, rom: &[u8], until: u64) -> Result<Recording, GymError> {
        let (mut machine, gameplay_frame) =
            boot_to_gameplay(rom).map_err(|e| GymError::Setup(e.to_string()))?;
        let handover =
            wait_for_handover(&mut machine).map_err(|e| GymError::Setup(e.to_string()))?;
        if until <= handover {
            return Err(GymError::BeforeHandover { until, handover });
        }
        let mut bot = BaseBot::default();
        let mut runs: Vec<(u8, u32)> = Vec::new();
        for frame in 1..=handover {
            push(&mut runs, setup_buttons(frame));
        }
        let mut refusals = 0usize;
        let mut furthest_x = read_state(&machine).absolute_x;
        let mut frame = machine.ppu().frame_id();
        while frame < until {
            let buttons = bot.buttons(&machine);
            push(&mut runs, buttons);
            machine.run_frame([buttons, ControllerButtons::NONE])?;
            let produced = read_state(&machine);
            if bot.observe(&produced, &machine, frame).is_some() {
                refusals += 1;
            }
            furthest_x = furthest_x.max(produced.absolute_x);
            frame = machine.ppu().frame_id();
        }
        let tape = Tape {
            cartridge: machine.cartridge_identity(),
            runs: Cow::Owned(runs),
            frame: until,
            golden_hash: machine.state_hash(),
        };
        let verdict = self.run_with(&tape, rom)?;
        Ok(Recording {
            tape,
            verdict,
            gameplay_frame,
            handover_frame: handover,
            refusals_before: refusals,
            furthest_x_before: furthest_x,
        })
    }
}

/// What [`Gym::record`] cut: a tape, the verdict measured from it, and the two
/// frame ids that say when the run-up was actually the game's and not the bot's.
#[derive(Debug)]
pub struct Recording {
    pub tape: Tape,
    /// Measured by replaying [`Recording::tape`], not by watching the run-up, so it
    /// is the same measurement [`Gym::run`] will make once the tape is committed.
    pub verdict: Verdict,
    /// Frame the mode byte said play.
    pub gameplay_frame: u64,
    /// Frame the game handed the player over; the policy starts on the one after it.
    pub handover_frame: u64,
    /// Refusals during the run-up, which is what a [`LedgerSeed::Warm`] gym arrives
    /// already knowing.
    pub refusals_before: usize,
    pub furthest_x_before: u16,
}

/// Append one frame's latch value, extending the run it belongs to.
fn push(runs: &mut Vec<(u8, u32)>, buttons: ControllerButtons) {
    let mask = buttons.bits();
    match runs.last_mut() {
        Some((last, count)) if *last == mask => *count += 1,
        _ => runs.push((mask, 1)),
    }
}

/// A latch value as the buttons it is, so a committed tape reads as a run-up and
/// not as a column of numbers.
pub fn button_names(mask: u8) -> String {
    let buttons = ControllerButtons::from_bits(mask);
    let mut named = Vec::new();
    for (name, button) in [
        ("A", ControllerButtons::A),
        ("B", ControllerButtons::B),
        ("SELECT", ControllerButtons::SELECT),
        ("START", ControllerButtons::START),
        ("UP", ControllerButtons::UP),
        ("DOWN", ControllerButtons::DOWN),
        ("LEFT", ControllerButtons::LEFT),
        ("RIGHT", ControllerButtons::RIGHT),
    ] {
        if buttons.contains(button) {
            named.push(name);
        }
    }
    if named.is_empty() {
        "NONE".to_owned()
    } else {
        named.join("|")
    }
}

/// Why a gym could not be run.
#[derive(Debug)]
pub enum GymError {
    /// The tape was recorded on a different image than the one loaded.
    WrongCartridge {
        expected: CartridgeIdentity,
        actual: CartridgeIdentity,
    },
    /// The recording does not cover the frame it claims to start on.
    ShortTape { played: u64, frame: u64 },
    /// Replaying the tape landed on a different machine state than the one recorded.
    /// The gym is not wrong and neither is the bot: the core moved.
    Drifted {
        expected: StateHash,
        actual: StateHash,
    },
    /// The run-up never reached the state a tape can start from.
    Setup(String),
    /// Asked to cut a tape that ends before the game has handed the player over,
    /// where there is no policy run to record yet.
    BeforeHandover { until: u64, handover: u64 },
    /// The tape could not be put under the profile at all.
    Cartridge(Unusable),
    /// The core failed mid-run.
    Machine(MachineError),
}

impl fmt::Display for GymError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongCartridge { expected, actual } => write!(
                f,
                "tape was recorded on {expected:?} and the loaded cartridge is {actual:?}",
            ),
            Self::ShortTape { played, frame } => write!(
                f,
                "tape covers {played} frames but claims to start on frame {frame}",
            ),
            Self::Drifted { expected, actual } => write!(
                f,
                "tape replayed to {actual} where it was recorded at {expected}: the core moved, \
                 not the bot",
            ),
            Self::Setup(reason) => write!(f, "the run-up never produced a start state: {reason}"),
            Self::BeforeHandover { until, handover } => write!(
                f,
                "a tape ending on frame {until} ends before the handover on frame {handover}",
            ),
            Self::Cartridge(error) => write!(f, "{error}"),
            Self::Machine(error) => write!(f, "the core stopped: {error}"),
        }
    }
}

impl Error for GymError {}

impl From<Unusable> for GymError {
    fn from(error: Unusable) -> Self {
        Self::Cartridge(error)
    }
}

impl From<MachineError> for GymError {
    fn from(error: MachineError) -> Self {
        Self::Machine(error)
    }
}
