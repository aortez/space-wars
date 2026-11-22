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
use engine_nes::{ControllerButtons, CpuBus, NesMachine};

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

/// The buttons the game itself believes player 1 is holding.
///
/// Every other address here is the game's opinion about the world. This one is its
/// opinion about the controller, and it is the only place the frame a button
/// actually arrived on can be read: the buttons handed to
/// [`engine_nes::NesMachine::run_frame`] are a host-side argument that vanishes when
/// the call returns.
pub const BUTTONS_P1: u16 = 0x074A;

// Player modes at `PLAYER_STATE`.
pub const STATE_LEFTMOST: u8 = 0x00;

pub const STATE_PLAYER_DIES: u8 = 0x06;

/// The game is moving him between areas, and his buttons do not reach him.
///
/// DirtSim's extractor names three of the thirteen modes this byte takes — `$00`,
/// `$06` and `$0b` — and leaves the rest to whoever reads them, which is how this one
/// went unnamed here while being the longest thing that happens to a level start. The
/// name is the reader's, from what the byte does. Measured on a start patched into
/// world 1's pipe slot, frame by frame from the Start press: 157 frames of load, then
/// `$07` for 334 frames while he walks right over the *previous* level's screen under his
/// own power, the level byte advancing to `$02` two frames before that walk ends, 24 more
/// frames of load, 17 more of `$07`, and his own area from there. Buttons during it are
/// ignored, so `is_in_control` has to exclude it and a handover wait sized for a plain
/// level load gives up inside it — which is the only reason four of the eight worlds ever
/// looked like levels that never started.
///
/// It is not a marker for travel alone. The same byte appears for one frame in the middle
/// of a plain level load and for seventeen in the load of a pipe world's second level,
/// always between the load and the handover, so the length is what separates a walk from
/// a flicker. What a caller actually needs is the shorter question — is he in control —
/// which [`SmbState::is_in_control`] answers for every one of these frames, and how long
/// the wait for it could take, which is [`LEVEL_LOAD_FRAMES`].
pub const STATE_TRAVEL: u8 = 0x07;

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
    ///
    /// The states it excludes are the load (`$00`), the two deaths (`$06`, `$0b`), the
    /// changing-size frames (`$09`, `$0a`) and the game's own travel between areas
    /// (`$07`, [`STATE_TRAVEL`]). The last is the expensive one to get wrong: it is not
    /// a short pause but a several-hundred-frame walk during which the level byte moves
    /// under the player, so a caller that treats it as play starts a policy two screens
    /// early and then has to explain a bot that ran right through a level it never saw.
    ///
    /// This is also how a fall into a pit reports itself, and it is slow: measured on
    /// the pit at world pixel 1379 he leaves the ground on frame 1100 and the state
    /// only leaves `$08` on frame 1379, so a caller that wants a verdict has to wait
    /// for the game to give one rather than infer it from a Y byte. There is no
    /// `has_fallen_off_screen` beside this on purpose: `screen_y` wraps every 256
    /// pixels of a fall, so DirtSim's terminal test against 224 is true for about
    /// eight frames in every sixty-four while he is falling and false for the rest.
    pub fn is_in_control(&self) -> bool {
        self.is_playing() && self.player_state == STATE_NORMAL
    }

    pub fn is_grounded(&self) -> bool {
        self.float_state == FLOAT_GROUNDED && self.player_state == STATE_NORMAL
    }

    /// The level the game is playing, as its own status bar numbers it.
    ///
    /// `level + 1` is not this, and the difference is not cosmetic. In the four worlds
    /// entered by pipe the byte runs one ahead of the label from the pipe level onward,
    /// so a bot that reads `level + 1` after clearing 1-1 believes it is in world 1-3
    /// while the status bar the game drew during the travel says 1-2 — measured, byte
    /// `$02` against a status-bar field of `01 28 02`. `None` is an honest answer too:
    /// a byte one past the world's last level still hands the player over, on an area
    /// that is not one of the world's four, and inventing a label for it is how a
    /// measurement quietly becomes a name.
    pub fn level_number(&self) -> Option<u8> {
        level_at_slot(self.world, self.level)
    }

    /// Whether this frame has cost him a life.
    ///
    /// Not the same question as the state byte read on its own, and DirtSim decodes
    /// it as its own value for the same reason. Two different bytes mean dying, and
    /// the life counter lags both: measured here, falling down the pit at world pixel
    /// 1379 puts `$06` in the byte a frame before the counter moves, and a Goomba at
    /// world pixel 1236 puts `$0b` there and never moves the counter at all on that
    /// frame. A caller that waits for the counter is late, and one that compares
    /// against `$06` alone misses a death outright.
    pub fn is_dying(&self) -> bool {
        matches!(self.player_state, STATE_PLAYER_DIES | STATE_DYING)
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

/// What the game says it was asked to do, as opposed to what a caller asked.
///
/// The byte is in the order the controller's serial register shifts buttons out — A
/// in the high bit down to Right in the low one — which is the reverse of
/// [`ControllerButtons`], so the decode reverses it. Measured against a press
/// pattern that touches all eight: asking A and Right reads `$81`, asking Left reads
/// `$02`, asking Select and Right reads `$21`, and reversing reproduces all three.
///
/// The reason to carry it is that every envelope in this project is a claim about
/// what a button program did, and such a claim is only worth the assumption under it:
/// that the buttons handed to a frame are the buttons the game acted on *that* frame.
/// Nothing in the tree could see that assumption. The execution plan records a bug
/// where a hold set after the frame's buttons had been built landed A one frame late
/// and moved a run by more than two thousand pixels, and every decoded state along the
/// way looked reasonable, because the state is the game's and the buttons were the
/// caller's. This byte is where the two are compared, which is DirtSim's response
/// probe doing exactly that: it refuses to time a jump response until the game's own
/// input mask has moved to what it asked for (`gameInputCopiedFrameId` in
/// `NesSuperMarioBrosResponseProbe.cpp`).
///
/// Read from RAM rather than from the port, because reading `$4016` shifts the
/// register and would disturb the game.
///
/// `tests/reader.rs` is the consumer today, and it is the check: the byte equals the
/// buttons the frame was handed, on that frame, once the game has handed the player
/// over.
pub fn buttons_the_game_read(machine: &NesMachine) -> ControllerButtons {
    ControllerButtons::from_bits(machine.bus().peek(BUTTONS_P1).reverse_bits())
}

/// The frame a level start has to write its two bytes on: one before the Start press.
///
/// The game reads them when it builds the level, and it builds the level as part of
/// starting the game, so a write that lands on the Start frame itself arrives after the
/// loader looked. Swept both ways against all thirty-two level starts, and the late one
/// is the instructive failure: it gives eight distinct screens in total, one per world,
/// each of them that world's own first level, while the byte reads back exactly what was
/// asked for. A patch can therefore be in place, verifiable, and completely unread by the
/// game — which is the same reason the byte is not evidence about a start and the screen
/// is. Writing here instead gives a start whose screen differs for every slot the mapping
/// asks for.
pub const SETUP_PATCH_FRAME: u64 = SETUP_START_FRAME - 1;

/// The worlds whose second level is entered through a pipe rather than at its own start.
///
/// World *bytes*, so `0` is world 1, the way [`WORLD`] and [`SmbState::world`] read it.
/// Measured rather than recalled: a start patched at level byte `$01` in these four
/// spends 334 frames in [`STATE_TRAVEL`] walking over the previous level's screen before
/// any area is drawn, and the other four draw an area after 167. They are the four
/// worlds whose second level is reached by taking the pipe at the end of the first —
/// 1-2, 2-2, 4-2 and 7-2 — and the walk is the game's own and not something the patch
/// invented: clearing 1-1 with the scripted driver runs through the same frames.
pub const WORLDS_ENTERED_BY_PIPE: [u8; 4] = [0, 1, 3, 6];

/// The [`LEVEL`] byte that puts the game at a world's level.
///
/// `world` is the byte — 0 for world 1 — and `level` is the number the game's own status
/// bar prints, 1 for the first level of a world. That asymmetry is the whole point of
/// the function: it is the seam between the label a person or a bot uses and the slot
/// the game indexes, and the two are not the same arithmetic in every world.
///
/// In the four worlds that are not entered by pipe the slot is the level minus one,
/// which is what the map looks like from a boot: `$0760` reads 0 while the status bar
/// reads 1-1. In the four that are, the game spends a slot on the pipe travel between
/// the first and second levels, so every level from the pipe level onward sits one
/// higher than its number: `$00` is 1-1, `$01` is the travel, `$02` is 1-2, `$03` is
/// 1-3 and `$04` is 1-4.
///
/// Three readings of the screen, not of the byte, hold that up. The area the travel
/// delivers is the area a start patched at `$02` draws, tile for tile, and the status
/// bar the game draws during the travel reads "1-2". The castle — black backdrop, a
/// ceiling across the whole screen, the alternating brick pattern — is at `$04` in the
/// four pipe worlds and at `$03` in the other four, and nowhere else in the forty slots
/// swept. And in world 1 slot `$03` is not a castle, so it cannot be the world's last
/// level, which is what rules out the simpler reading where the byte is the level minus
/// one everywhere.
pub fn level_slot(world: u8, level: u8) -> Option<u8> {
    if world >= 8 || level == 0 || level > 4 {
        return None;
    }
    Some(if WORLDS_ENTERED_BY_PIPE.contains(&world) && level > 1 {
        level
    } else {
        level - 1
    })
}

/// The level a [`LEVEL`] byte value is playing, as the status bar numbers it.
///
/// The inverse of [`level_slot`], with the one thing an inverse cannot undo: slot `$01`
/// in a pipe world is the travel, which is not a level and reports the level it is
/// travelling to. Bytes past a world's last slot answer `None` rather than a label,
/// because the game really does accept them — a start at `$04` in world 3 hands the
/// player over on an area that is not one of world 3's four.
pub fn level_at_slot(world: u8, slot: u8) -> Option<u8> {
    if world >= 8 {
        return None;
    }
    if WORLDS_ENTERED_BY_PIPE.contains(&world) {
        match slot {
            0 => Some(1),
            1 => Some(2),
            2..=4 => Some(slot),
            _ => None,
        }
    } else if slot <= 3 {
        Some(slot + 1)
    } else {
        None
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
    boot_with_start(rom, None)
}

/// Boot the cartridge and start it at one particular level, then wait for the game to
/// hand the player over.
///
/// The two bytes go in on [`SETUP_PATCH_FRAME`], and the wait is
/// [`wait_for_handover`] rather than the mode byte, because the mode byte says play from
/// the first frame of a load and the load then takes another 167 to 231 frames before he
/// is anyone's to control; a policy started at the mode byte spends all of them pushing
/// at a world that has not let go of him. `world` is the byte and `level` the status-bar
/// number, as in [`level_slot`]; the slot the byte is written with is that function's
/// answer, so a caller asks for 4-2 and does not need to know that is `$04`.
///
/// Returns the machine and the frame the handover was confirmed on.
pub fn boot_to_level(
    rom: &[u8],
    world: u8,
    level: u8,
) -> Result<(NesMachine, u64), Box<dyn std::error::Error>> {
    let (mut machine, _) = boot_to_level_load(rom, world, level)?;
    let handed_over = wait_for_handover(&mut machine)?;
    Ok((machine, handed_over))
}

/// [`boot_to_level`] stopped one step earlier: the patch is in and the mode byte says
/// play, but the level has not been placed and the player has not been handed over.
///
/// The instrument that watches a load needs this and the one that plays a level needs
/// the other, and the frames in between are not idle time to a reader: the level byte
/// moves during them in the pipe worlds, and the screen under the camera still belongs
/// to the level before this one.
pub fn boot_to_level_load(
    rom: &[u8],
    world: u8,
    level: u8,
) -> Result<(NesMachine, u64), Box<dyn std::error::Error>> {
    let slot = level_slot(world, level).ok_or("no such world or level")?;
    boot_with_start(rom, Some((world, slot)))
}

/// The setup script, with an optional level start written one frame before the press.
fn boot_with_start(
    rom: &[u8],
    start: Option<(u8, u8)>,
) -> Result<(NesMachine, u64), Box<dyn std::error::Error>> {
    let mut machine = cartridge::machine(rom)?;
    for frame in 1..=SETUP_FAILURE_FRAME {
        if frame == SETUP_PATCH_FRAME
            && let Some((world, slot)) = start
        {
            machine.bus_mut().write(WORLD, world);
            machine.bus_mut().write(LEVEL, slot);
        }
        machine.run_frame([setup_buttons(frame), ControllerButtons::NONE])?;
        if machine.bus().peek(GAME_MODE) == MODE_PLAYING {
            return Ok((machine, frame));
        }
    }
    Err("setup script never reached normal play".into())
}

/// Frames to wait for a level to place the player, from the mode byte saying play.
///
/// Counted the way [`wait_for_handover`] counts: from the frame the mode byte is first
/// seen on to the frame the handover is confirmed. That anchor is the Start press itself
/// — the mode byte flips to play on the frame the press runs on, so "from the mode byte"
/// and "from the press" are the same number here — and the count includes the
/// [`HANDOVER_FRAMES`] of standing still the confirmation needs, which is why a plain
/// level's 167 is more than the 157 frames the game spends loading it.
///
/// Measured against every start this reader can ask for, and there are three kinds. A
/// plain level load takes 167 of them, in all eight worlds. A start at the second level
/// of one of the four pipe worlds takes 217 or 231, because the game still has to place
/// an area the world was entered past its own beginning. A start written at the travel
/// slot itself — `$01` in those worlds, which [`level_slot`] never returns but a caller
/// holding a byte can easily write — takes 577 or 591, because the game walks him over
/// the *previous* level's screen before it draws his own. The mode byte says play for all
/// three, so the wait cannot know which it is behind and the constant is the largest of
/// the three. The margin costs anything only on a start that never hands over at all, and
/// the number that used to be here, 300, is exactly why four worlds once looked like
/// levels that never started: the obvious mapping writes the travel slot in them.
pub const LEVEL_LOAD_FRAMES: u64 = 720;

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
