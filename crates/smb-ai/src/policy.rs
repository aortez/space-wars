//! L5: the smallest policy that acts on the model, and the envelope it acts through.
//!
//! Run right, and jump whatever the model says is in the way: a hole, an enemy, a
//! wall. Nothing here knows the shape of a level, and nothing here reaches past L4
//! for a tile number or an address, which is the test Phase 0 set for this layer.
//!
//! The envelope matters as much as the decisions. SMB's jump is variable-height, so
//! a one-frame A press is a hop that clears nothing, and a hold set after a frame's
//! buttons were built lands A one frame late and changes a run by more than two
//! thousand pixels. Both of this project's failures that looked like model bugs were
//! envelope bugs, so the hold lives here rather than in whatever is driving.

use engine_nes::{ControllerButtons, NesMachine};
use smb_model::affordances::{
    ENEMY_TRIGGER_PIXELS, WALL_LOOK_COLUMNS, gap_within_reach, wall_ahead,
};
use smb_model::frame::{TILE_CELLS, player_tile, scroll_x, snapshot, tile_grid};
use smb_model::profile::{SmbState, read_state};
use smb_model::resistance::{Motion, Refusal, Resistance};

/// Frames to keep A down once a jump starts.
pub const JUMP_HOLD_FRAMES: u64 = 20;

/// One frame of the base policy: run right, and jump a hole, an enemy or a wall the
/// reader can see. `hold_a` carries a jump across frames, because releasing A on the
/// frame he leaves the ground gives a hop rather than a jump.
///
/// The caller passes the verdict rather than the ledger, because how far ahead to
/// trust a wall is the caller's business and the answer differs between a bot with a
/// run-up and a test that only wants to know what the model saw.
pub fn run_right_buttons(
    state: &SmbState,
    machine: &NesMachine,
    tiles: &[u8; TILE_CELLS],
    column: i16,
    row: i16,
    wall: bool,
    hold_a: &mut u64,
) -> ControllerButtons {
    let enemy_ahead = state
        .enemies(machine)
        .iter()
        .filter(|enemy| enemy.delta_x > 0)
        .map(|enemy| enemy.delta_x)
        .min()
        .unwrap_or(i16::MAX);
    if state.is_grounded()
        && (wall
            || gap_within_reach(tiles, column, row).is_some()
            || enemy_ahead <= ENEMY_TRIGGER_PIXELS)
    {
        *hold_a = JUMP_HOLD_FRAMES;
    }
    let mut buttons = ControllerButtons::RIGHT | ControllerButtons::B;
    if *hold_a > 0 {
        buttons |= ControllerButtons::A;
        *hold_a -= 1;
    }
    buttons
}

/// What any player of this game does on a frame, whether it is the reflex below, a
/// script of measured skills, or Phase 5's search.
///
/// Three calls and nothing else, because the harness has to be able to swap a player
/// without knowing what it knows: `buttons` reads the machine and returns one latch
/// value, `observe` is the game's answer to that value, and `learn` is the same answer
/// for a frame whose buttons came from somewhere else — a gym replays a recorded
/// latch value rather than asking a policy, and still has to tell its model what the
/// game did with it.
///
/// The gym harness drives this trait, so a skill library and a search are measured by
/// the same instrument that measures the reflex, on the same tapes.
pub trait Bot {
    /// Buttons for this frame, read from the machine as it stands.
    fn buttons(&mut self, machine: &NesMachine) -> ControllerButtons;

    /// What the game did with the last frame's buttons.
    fn observe(
        &mut self,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal>;

    /// The same, for a frame decided by something other than [`Bot::buttons`].
    fn learn(
        &mut self,
        asked: Motion,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal>;
}

impl Bot for BaseBot {
    fn buttons(&mut self, machine: &NesMachine) -> ControllerButtons {
        Self::buttons(self, machine)
    }

    fn observe(
        &mut self,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal> {
        Self::observe(self, produced, machine, at_frame)
    }

    fn learn(
        &mut self,
        asked: Motion,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal> {
        Self::learn(self, asked, produced, machine, at_frame)
    }
}

/// The base policy with the model's memory of solidity under it.
///
/// A bot is `buttons` then `observe` and nothing else: it reads the machine, returns
/// one frame's latch value, and is then told what the game did with it. That envelope
/// is why a gym, an example and a test can all say "the bot" and mean the same
/// thing, and it is the seam Phase 3's skills will be added to.
///
/// The ledger is a field rather than an argument because it is what the bot *is*:
/// handing a bot someone else's refusals is the difference between a model that has
/// been taught and one that has not, and Phase 1 measured that difference as the
/// whole content of the claim.
#[derive(Debug)]
pub struct BaseBot {
    pub ledger: Resistance,
    look_columns: i16,
    hold_a: u64,
    asked: Option<Motion>,
    wall: bool,
    ledger_named_wall: bool,
}

impl Default for BaseBot {
    fn default() -> Self {
        Self::new(Resistance::default())
    }
}

impl BaseBot {
    /// A bot that starts out having been refused nothing.
    pub fn new(ledger: Resistance) -> Self {
        Self::with_look_columns(ledger, WALL_LOOK_COLUMNS)
    }

    /// The same bot, told how far ahead of its own body to believe a wall.
    ///
    /// Zero is the strictest reading: only the cell a refusal actually named is a
    /// report, and everything past it is an induction. A caller that wants to watch
    /// a model with no memory meet an obstacle for the first time asks for that, and
    /// gets a bot that is otherwise identical to this one.
    pub fn with_look_columns(ledger: Resistance, look_columns: i16) -> Self {
        Self {
            ledger,
            look_columns,
            hold_a: 0,
            asked: None,
            wall: false,
            ledger_named_wall: false,
        }
    }

    /// What the model sees, and what this frame's buttons will do about it.
    ///
    /// Call this immediately before `run_frame`: the buttons it returns are only
    /// valid for the frame whose state it just read.
    pub fn buttons(&mut self, machine: &NesMachine) -> ControllerButtons {
        let state = read_state(machine);
        let frame = snapshot(machine);
        let origin_x = scroll_x(&frame);
        let tiles = tile_grid(&frame, origin_x, 0);
        let (column, row) = player_tile(&state, origin_x);
        let columns = self.look_columns;
        self.wall = wall_ahead(&frame, origin_x, &state, &self.ledger, columns);
        // The same question with the ledger wiped, so a yes here is something this
        // model was taught and not something the tile ids already carry.
        self.ledger_named_wall =
            self.wall && !wall_ahead(&frame, origin_x, &state, &Resistance::default(), columns);
        let buttons = run_right_buttons(
            &state,
            machine,
            &tiles,
            column,
            row,
            self.wall,
            &mut self.hold_a,
        );
        self.asked = Some(Motion::asked(&state, buttons));
        buttons
    }

    /// Tell the bot what the game did with the last frame's buttons.
    ///
    /// `at_frame` is the frame id those buttons were issued on, which is what a
    /// refusal is recorded against, so that a claim about one can be replayed.
    pub fn observe(
        &mut self,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal> {
        let asked = self.asked.take()?;
        self.learn(asked, produced, machine, at_frame)
    }

    /// The same, for a frame whose buttons were decided by something other than
    /// [`BaseBot::buttons`]: a gym replays a recorded latch value rather than asking
    /// a policy, and still has to tell its model what the game did with it.
    pub fn learn(
        &mut self,
        asked: Motion,
        produced: &SmbState,
        machine: &NesMachine,
        at_frame: u64,
    ) -> Option<Refusal> {
        let frame = snapshot(machine);
        self.ledger
            .observe(asked, produced, &frame, scroll_x(&frame), at_frame)
    }

    /// Whether the model saw something to stand against on the frame just acted.
    pub fn saw_wall(&self) -> bool {
        self.wall
    }

    /// Whether the model saw a wall *because of what it remembers*, on the frame
    /// just acted: the ledger said yes and the tile ids said no.
    ///
    /// This is the only observation that measures the ledger rather than the level,
    /// which is why Phase 1's test and example both watch for its first frame.
    pub fn ledger_named_wall(&self) -> bool {
        self.ledger_named_wall
    }
}
