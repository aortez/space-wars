//! L1 profile: one cartridge's address map, its decoders, and the scripted
//! setup that gets a cartridge into play.
//!
//! Nothing here knows what a tile is. The map is DirtSim's
//! `NesSuperMarioBrosRamExtractor.cpp`, itself from the Data Crystal Super Mario
//! Bros. RAM map, retargeted at the `engine-nes` read surface. The address
//! constants stay raw because the map *is* the artifact: a reader that hides them
//! cannot be checked against the game.
//!
//! The profile binds to the core's own `CartridgeIdentity`, which is what a
//! running machine can answer cheaply; `cartridge` holds that check and the
//! pinned revision. A different cartridge is refused rather than misread.

use crate::cartridge;
use engine_nes::{ControllerButtons, NesMachine};

// Player and progression state. DirtSim NesSuperMarioBrosRamExtractor.cpp,
// from the Data Crystal Super Mario Bros. RAM map.
pub const GAME_MODE: u16 = 0x0770;

pub const WORLD: u16 = 0x075F;

pub const LEVEL: u16 = 0x0760;

pub const LIVES: u16 = 0x075A;

pub const POWERUP: u16 = 0x0756;

pub const PLAYER_STATE: u16 = 0x000E;

pub const FLOAT_STATE: u16 = 0x001D;

pub const H_SPEED: u16 = 0x0057;

pub const V_SPEED: u16 = 0x009F;

pub const FACING: u16 = 0x0033;

pub const X_PAGE: u16 = 0x006D;

pub const X_SCREEN: u16 = 0x0086;

pub const Y_SCREEN: u16 = 0x00CE;

// The five enemy slots the game keeps live at once.
pub const ENEMY_ACTIVE: [u16; 5] = [0x000F, 0x0010, 0x0011, 0x0012, 0x0013];

pub const ENEMY_TYPE: [u16; 5] = [0x0016, 0x0017, 0x0018, 0x0019, 0x001A];

pub const ENEMY_X_PAGE: [u16; 5] = [0x006E, 0x006F, 0x0070, 0x0071, 0x0072];

pub const ENEMY_X_SCREEN: [u16; 5] = [0x0087, 0x0088, 0x0089, 0x008A, 0x008B];

pub const ENEMY_Y_SCREEN: [u16; 5] = [0x00CF, 0x00D0, 0x00D1, 0x00D2, 0x00D3];

// Player modes at `PLAYER_STATE`.
pub const STATE_LEFTMOST: u8 = 0x00;

pub const STATE_PLAYER_DIES: u8 = 0x06;

pub const STATE_NORMAL: u8 = 0x08;

pub const STATE_GROWING: u8 = 0x09;

pub const STATE_SHRINKING: u8 = 0x0A;

pub const STATE_DYING: u8 = 0x0B;

// Game modes at `GAME_MODE`.
pub const MODE_ATTRACT: u8 = 0x00;

pub const MODE_PLAYING: u8 = 0x01;

pub const MODE_END_WORLD: u8 = 0x02;

pub const MODE_END_GAME: u8 = 0x03;

// Float state at `FLOAT_STATE`.
pub const FLOAT_GROUNDED: u8 = 0x00;

pub const FLOAT_JUMPING: u8 = 0x01;

pub const FLOAT_WALKED_OFF: u8 = 0x02;

pub const FLOAT_FLAGPOLE: u8 = 0x03;

// DirtSim's setup policy: one Start press, then watch for normal play.
pub const SETUP_START_FRAME: u64 = 120;

pub const SETUP_SCRIPT_END_FRAME: u64 = 300;

pub const SETUP_FAILURE_FRAME: u64 = 500;

// The evaluator's terminal test: below the visible screen means lost.
pub const BELOW_SCREEN_Y: u8 = 224;

/// A decoded enemy slot. `delta_x` is signed distance from the player inside
/// the 512-pixel nametable period, which is what the PPU and the game both see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enemy {
    pub slot: usize,
    pub kind: u8,
    pub absolute_x: u16,
    pub screen_y: u8,
    pub delta_x: i16,
}

/// Semantic game state, decoded from CPU RAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmbState {
    pub game_mode: u8,
    pub world: u8,
    pub level: u8,
    pub lives: u8,
    pub powerup: u8,
    pub player_state: u8,
    pub float_state: u8,
    pub h_speed: i8,
    pub v_speed: i8,
    pub facing: u8,
    pub absolute_x: u16,
    pub screen_x: u8,
    pub screen_y: u8,
}

impl SmbState {
    pub fn is_playing(&self) -> bool {
        self.game_mode == MODE_PLAYING
    }

    /// Whether the game has placed him in the level and is taking his input.
    ///
    /// `is_playing` alone does not answer that. `$0770` reads play from the first
    /// frame of a level load, and for the ~150 frames after it he is pinned to
    /// absolute X 0 in player state 0x00 while the level is placed and the scroll
    /// set. A policy started on `is_playing` spends its first two seconds pushing
    /// at a world that has not handed him over, and any stuck detector measured in
    /// frames reads that as the bot giving up.
    pub fn is_in_control(&self) -> bool {
        self.is_playing() && self.player_state == STATE_NORMAL
    }

    pub fn is_grounded(&self) -> bool {
        self.float_state == FLOAT_GROUNDED && self.player_state == STATE_NORMAL
    }

    pub fn has_fallen_off_screen(&self) -> bool {
        self.screen_y > BELOW_SCREEN_Y
    }
}

/// Absolute horizontal position: SMB splits it across a page byte and an
/// on-screen byte because a level is wider than one nametable.
pub fn absolute_x(machine: &NesMachine, page: u16, screen: u16) -> u16 {
    (u16::from(machine.bus().peek(page)) << 8) | u16::from(machine.bus().peek(screen))
}

/// Decode the whole map in one pass. Reads are side-effect free, so this can
/// run every frame without disturbing the game.
pub fn read_state(machine: &NesMachine) -> SmbState {
    let peek = |address| machine.bus().peek(address);
    SmbState {
        game_mode: peek(GAME_MODE),
        world: peek(WORLD),
        level: peek(LEVEL),
        lives: peek(LIVES),
        powerup: peek(POWERUP),
        player_state: peek(PLAYER_STATE),
        float_state: peek(FLOAT_STATE),
        h_speed: peek(H_SPEED) as i8,
        v_speed: peek(V_SPEED) as i8,
        facing: peek(FACING),
        absolute_x: absolute_x(machine, X_PAGE, X_SCREEN),
        screen_x: peek(X_SCREEN),
        screen_y: peek(Y_SCREEN),
    }
}

/// The DirtSim setup script: nothing until frame 120, one Start press there.
pub fn setup_buttons(frame: u64) -> ControllerButtons {
    if frame == SETUP_START_FRAME {
        ControllerButtons::START
    } else {
        ControllerButtons::NONE
    }
}

/// Boot the cartridge this profile decodes and run the setup script until normal
/// play begins.
///
/// Returns the frame gameplay appeared on. A cartridge that is not the pinned
/// revision fails in [`cartridge::machine`] before a single frame runs, so the
/// only way to get the error this returns is a core that boots the right
/// cartridge and still never reaches play.
pub fn boot_to_gameplay(rom: &[u8]) -> Result<(NesMachine, u64), Box<dyn std::error::Error>> {
    let mut machine = cartridge::machine(rom)?;
    for frame in 1..=SETUP_FAILURE_FRAME {
        machine.run_frame([setup_buttons(frame), ControllerButtons::NONE])?;
        if machine.bus().peek(GAME_MODE) == MODE_PLAYING {
            return Ok((machine, frame));
        }
    }
    Err("setup script never reached normal play".into())
}

/// Frames the game takes to place a level, counted from the mode byte saying play.
pub const LEVEL_LOAD_FRAMES: u64 = 300;

/// Consecutive frames of "in control and grounded" that count as a handover.
pub const HANDOVER_FRAMES: u64 = 8;

/// Run idle frames until the game has put the player in the level and let go.
///
/// `boot_to_gameplay` stops at the mode byte, and the mode byte says play from the
/// very first frame of the level load. The load then takes about 150 more frames to
/// place the level, and through the first two of those the player bytes still hold
/// the *previous* screen's values, so a single frame of "normal, grounded, at X 40"
/// is the ghost of a level that is already gone. Wait for the state to hold rather
/// than for it to flicker: a policy started on the mode byte spends its first two
/// seconds pushing at a world that is not listening, and a stall detector counted in
/// frames then reads the run as giving up before it has moved.
///
/// Returns the frame id the handover was confirmed on.
pub fn wait_for_handover(machine: &mut NesMachine) -> Result<u64, Box<dyn std::error::Error>> {
    let mut in_control_for = 0;
    for _ in 0..LEVEL_LOAD_FRAMES {
        let state = read_state(machine);
        in_control_for = if state.is_in_control() && state.is_grounded() {
            in_control_for + 1
        } else {
            0
        };
        if in_control_for >= HANDOVER_FRAMES {
            return Ok(machine.ppu().frame_id());
        }
        machine.run_frame([ControllerButtons::NONE, ControllerButtons::NONE])?;
    }
    Err("the game never handed the player over".into())
}
