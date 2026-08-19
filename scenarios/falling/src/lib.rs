//! A deterministic scenario adapter for the bundled Falling NES game.

use std::fmt;
use std::time::Duration;

use engine_common::{
    Action, NativePixelFormat, NativeVideoCrop, NativeVideoFrame, NativeVideoTiming, Observation,
    RenderFrame, Scenario, StepResult, TickModel,
};
use engine_nes::{
    AudioOutput, CartridgeError, CartridgeIdentity, CartridgeImage, ControllerButtons,
    FRAME_HEIGHT, FRAME_WIDTH, FrameInput, FrameResult, MachineConfig, MachineError,
    NES_PALETTE_RGB565, NesMachine, VideoOutput,
};

pub const FALLING_ROM: &[u8] = include_bytes!("../assets/falling.nes");
pub const FALLING_ROM_SHA256: &str =
    "e22b947542c2d7e595bf84725b333be7af8189c5965b9c53e356a249c7d79943";
pub const FALLING_ROM_IDENTITY: CartridgeIdentity = CartridgeIdentity {
    byte_len: 40_976,
    fnv1a64: 0x16a4_d7ee_be1a_fc30,
};

pub const FALLING_ACTION_VERSION: u16 = 1;
pub const FALLING_ACTION_SET_CONTROLLER: u32 = 0;
pub const FALLING_OBSERVATION_VERSION: u16 = 1;

const GAME_STATE_ADDRESS: usize = 0x00;
const GAME_MODE_ADDRESS: usize = 0x0b;
const PLAYER_X_ADDRESS: usize = 0x12;
const PLAYER_Y_ADDRESS: usize = 0x13;
const PLAYER_LIVES_ADDRESS: usize = 0x19;
const PLAYER_SCORE_ADDRESS: usize = 0x1b;

/// Falling draws useful pixels across the full hardware frame, while the
/// original host reference presents a conventional 8-pixel top/bottom crop.
pub const FALLING_VISIBLE_CROP: NativeVideoCrop = NativeVideoCrop {
    x: 0,
    y: 8,
    width: FRAME_WIDTH as u32,
    height: (FRAME_HEIGHT - 16) as u32,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FallingConfig {
    pub audio: AudioOutput,
}

impl Default for FallingConfig {
    fn default() -> Self {
        Self {
            audio: AudioOutput::Disabled,
        }
    }
}

#[derive(Debug)]
pub enum FallingError {
    Cartridge(CartridgeError),
    InvalidEmbeddedRom {
        expected: CartridgeIdentity,
        actual: CartridgeIdentity,
    },
    Runtime(MachineError),
}

impl fmt::Display for FallingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cartridge(error) => error.fmt(formatter),
            Self::InvalidEmbeddedRom { expected, actual } => write!(
                formatter,
                "embedded Falling ROM identity mismatch: expected {expected:?}, found {actual:?}"
            ),
            Self::Runtime(error) => write!(formatter, "Falling machine startup failed: {error}"),
        }
    }
}

impl std::error::Error for FallingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cartridge(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::InvalidEmbeddedRom { .. } => None,
        }
    }
}

impl From<CartridgeError> for FallingError {
    fn from(error: CartridgeError) -> Self {
        Self::Cartridge(error)
    }
}

#[derive(Debug)]
pub struct FallingState {
    machine: NesMachine,
    controller: ControllerButtons,
    next_input_sequence: u64,
    runtime_error: Option<String>,
}

impl FallingState {
    pub fn try_new(config: FallingConfig) -> Result<Self, FallingError> {
        let image = CartridgeImage::parse(FALLING_ROM)?;
        let actual = image.identity();
        if actual != FALLING_ROM_IDENTITY {
            return Err(FallingError::InvalidEmbeddedRom {
                expected: FALLING_ROM_IDENTITY,
                actual,
            });
        }

        let mut machine = NesMachine::power_on(
            image,
            MachineConfig {
                video: VideoOutput::Enabled,
                audio: config.audio,
                ..MachineConfig::default()
            },
        );
        // Finish the first hardware frame before publication so a successful
        // factory always exposes a complete, initialized native image.
        machine
            .run_frame_with_input(FrameInput::new(
                0,
                [ControllerButtons::NONE, ControllerButtons::NONE],
            ))
            .map_err(FallingError::Runtime)?;

        Ok(Self {
            machine,
            controller: ControllerButtons::NONE,
            next_input_sequence: 1,
            runtime_error: None,
        })
    }

    pub fn machine(&self) -> &NesMachine {
        &self.machine
    }

    pub fn controller(&self) -> ControllerButtons {
        self.controller
    }

    pub fn runtime_error(&self) -> Option<&str> {
        self.runtime_error.as_deref()
    }

    pub fn native_video_frame(&self) -> NativeVideoFrame<'_> {
        let pixels = self
            .machine
            .ppu()
            .framebuffer()
            .expect("Falling always enables native video output");
        NativeVideoFrame {
            width: FRAME_WIDTH as u32,
            height: FRAME_HEIGHT as u32,
            visible_crop: FALLING_VISIBLE_CROP,
            pixel_format: NativePixelFormat::Indexed8Rgb565,
            frame_id: self.machine.ppu().frame_id(),
            pixels,
            palette_rgb565: &NES_PALETTE_RGB565,
            timing: Some(NativeVideoTiming {
                emulated_ticks: self.machine.ppu().timing().clocks,
                input_sequence_id: self.machine.last_applied_input().sequence_id,
            }),
        }
    }

    /// Advances one authoritative frame with a caller-owned input sequence.
    /// Realtime and synchronous hosts therefore share the exact same machine
    /// boundary; only the caller's pacing policy differs.
    pub fn advance_frame(&mut self, input: FrameInput) -> Result<FrameResult<'_>, String> {
        if let Some(error) = &self.runtime_error {
            return Err(error.clone());
        }

        self.controller = input.controllers[0];
        self.next_input_sequence = input.sequence_id.wrapping_add(1);
        match self.machine.run_frame_with_input(input) {
            Ok(result) => Ok(result),
            Err(error) => {
                let detail = error.to_string();
                self.runtime_error = Some(detail.clone());
                Err(detail)
            }
        }
    }

    fn apply_actions(&mut self, actions: &[Action]) {
        for action in actions {
            if let Some(buttons) = FallingAction::decode(action) {
                self.controller = buttons;
            }
        }
    }

    fn run_frame(&mut self) -> StepResult {
        if self.runtime_error.is_some() {
            return StepResult { terminated: true };
        }

        let input = FrameInput {
            sequence_id: self.next_input_sequence,
            controllers: [self.controller, ControllerButtons::NONE],
        };
        self.next_input_sequence = self.next_input_sequence.wrapping_add(1);
        match self.advance_frame(input) {
            Ok(_) => StepResult::default(),
            Err(_) => StepResult { terminated: true },
        }
    }
}

pub struct FallingScenario;

impl FallingScenario {
    pub fn try_init(config: FallingConfig, _seed: u64) -> Result<FallingState, FallingError> {
        FallingState::try_new(config)
    }
}

impl Scenario for FallingScenario {
    type State = FallingState;
    type Config = FallingConfig;

    fn init(config: Self::Config, seed: u64) -> Self::State {
        Self::try_init(config, seed).expect("the pinned embedded Falling ROM must initialize")
    }

    fn step(state: &mut Self::State, actions: &[Action], _dt: Duration) -> StepResult {
        state.apply_actions(actions);
        state.run_frame()
    }

    fn observe(state: &Self::State) -> Observation {
        FallingObservation::from_state(state).encode()
    }

    fn render_frame(_state: &Self::State) -> RenderFrame {
        // Native video is exposed separately; this empty frame keeps the
        // generic Scenario trait usable by headless callers.
        RenderFrame::default()
    }

    fn tick_model() -> TickModel {
        TickModel::EmulatorClock
    }
}

pub struct FallingAction;

impl FallingAction {
    pub fn set_controller(buttons: ControllerButtons) -> Action {
        let mut payload = Vec::with_capacity(3);
        payload.extend_from_slice(&FALLING_ACTION_VERSION.to_le_bytes());
        payload.push(buttons.bits());
        Action::scenario(FALLING_ACTION_SET_CONTROLLER, payload)
    }

    pub fn decode(action: &Action) -> Option<ControllerButtons> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        if *kind != FALLING_ACTION_SET_CONTROLLER || payload.len() != 3 {
            return None;
        }
        let version = u16::from_le_bytes([payload[0], payload[1]]);
        (version == FALLING_ACTION_VERSION).then(|| ControllerButtons::from_bits(payload[2]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FallingObservation {
    pub frame_id: u64,
    pub state_hash_version: u16,
    pub state_hash: u64,
    pub controller: ControllerButtons,
    pub game_state: u8,
    pub game_mode: u8,
    pub player_x: u8,
    pub player_y: u8,
    pub player_lives: u8,
    pub player_score: u16,
}

impl FallingObservation {
    const PAYLOAD_BYTES: usize = 2 + 8 + 2 + 8 + 1 + 1 + 1 + 1 + 1 + 1 + 2;

    fn from_state(state: &FallingState) -> Self {
        let ram = state.machine.bus().ram();
        let hash = state.machine.state_hash();
        Self {
            frame_id: state.machine.ppu().frame_id(),
            state_hash_version: hash.version,
            state_hash: hash.value,
            controller: state.controller,
            game_state: ram[GAME_STATE_ADDRESS],
            game_mode: ram[GAME_MODE_ADDRESS],
            player_x: ram[PLAYER_X_ADDRESS],
            player_y: ram[PLAYER_Y_ADDRESS],
            player_lives: ram[PLAYER_LIVES_ADDRESS],
            player_score: u16::from_le_bytes([
                ram[PLAYER_SCORE_ADDRESS],
                ram[PLAYER_SCORE_ADDRESS + 1],
            ]),
        }
    }

    fn encode(self) -> Observation {
        let mut payload = Vec::with_capacity(Self::PAYLOAD_BYTES);
        payload.extend_from_slice(&FALLING_OBSERVATION_VERSION.to_le_bytes());
        payload.extend_from_slice(&self.frame_id.to_le_bytes());
        payload.extend_from_slice(&self.state_hash_version.to_le_bytes());
        payload.extend_from_slice(&self.state_hash.to_le_bytes());
        payload.push(self.controller.bits());
        payload.push(self.game_state);
        payload.push(self.game_mode);
        payload.push(self.player_x);
        payload.push(self.player_y);
        payload.push(self.player_lives);
        payload.extend_from_slice(&self.player_score.to_le_bytes());
        Observation { payload }
    }

    pub fn decode(observation: &Observation) -> Option<Self> {
        let bytes = observation.payload.as_slice();
        if bytes.len() != Self::PAYLOAD_BYTES
            || u16::from_le_bytes([bytes[0], bytes[1]]) != FALLING_OBSERVATION_VERSION
        {
            return None;
        }
        Some(Self {
            frame_id: u64::from_le_bytes(bytes[2..10].try_into().ok()?),
            state_hash_version: u16::from_le_bytes(bytes[10..12].try_into().ok()?),
            state_hash: u64::from_le_bytes(bytes[12..20].try_into().ok()?),
            controller: ControllerButtons::from_bits(bytes[20]),
            game_state: bytes[21],
            game_mode: bytes[22],
            player_x: bytes[23],
            player_y: bytes[24],
            player_lives: bytes[25],
            player_score: u16::from_le_bytes(bytes[26..28].try_into().ok()?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_rom_identity_and_native_frame_are_pinned() {
        let state = FallingState::try_new(FallingConfig::default()).unwrap();
        assert_eq!(state.machine().cartridge_identity(), FALLING_ROM_IDENTITY);

        let frame = state.native_video_frame();
        assert!(frame.has_valid_layout());
        assert_eq!(frame.width, 256);
        assert_eq!(frame.height, 240);
        assert_eq!(frame.visible_crop, FALLING_VISIBLE_CROP);
        assert_eq!(frame.palette_rgb565.len(), 64);
        assert_eq!(frame.timing.unwrap().input_sequence_id, 0);
    }

    #[test]
    fn full_controller_action_is_versioned_and_applied_at_frame_boundary() {
        let mut state = FallingState::try_new(FallingConfig::default()).unwrap();
        let buttons = ControllerButtons::START | ControllerButtons::RIGHT;
        let action = FallingAction::set_controller(buttons);
        assert_eq!(FallingAction::decode(&action), Some(buttons));

        FallingScenario::step(&mut state, &[action], Duration::ZERO);
        assert_eq!(state.controller(), buttons);
        assert_eq!(state.machine().last_applied_input().controllers[0], buttons);
        assert_eq!(state.machine().last_applied_input().sequence_id, 1);
    }

    #[test]
    fn realtime_callers_preserve_their_input_sequence_at_the_same_boundary() {
        let mut state = FallingState::try_new(FallingConfig::default()).unwrap();
        let input = FrameInput::new(
            41,
            [
                ControllerButtons::LEFT | ControllerButtons::B,
                ControllerButtons::NONE,
            ],
        );

        let result = state.advance_frame(input).unwrap();
        assert_eq!(result.input.sequence_id, 41);
        assert_eq!(result.input.controllers, input.controllers);
        assert_eq!(result.input.frame_id, result.frame_id);
    }

    #[test]
    fn observation_has_a_stable_decodable_v1_shape() {
        let mut state = FallingState::try_new(FallingConfig::default()).unwrap();
        for _ in 0..8 {
            FallingScenario::step(
                &mut state,
                &[FallingAction::set_controller(ControllerButtons::NONE)],
                Duration::ZERO,
            );
        }
        FallingScenario::step(
            &mut state,
            &[FallingAction::set_controller(ControllerButtons::START)],
            Duration::ZERO,
        );
        let observation = FallingScenario::observe(&state);
        let decoded = FallingObservation::decode(&observation).unwrap();

        assert_eq!(observation.payload.len(), FallingObservation::PAYLOAD_BYTES);
        assert_eq!(decoded.frame_id, state.machine().ppu().frame_id());
        assert_eq!(decoded.controller, ControllerButtons::START);
        assert_eq!(decoded.state_hash, state.machine().state_hash().value);
        assert_eq!(decoded.game_state, 1);
    }

    #[test]
    fn wrong_action_versions_do_not_mutate_controller_state() {
        let mut state = FallingState::try_new(FallingConfig::default()).unwrap();
        let action = Action::scenario(FALLING_ACTION_SET_CONTROLLER, vec![2, 0, 0xff]);
        FallingScenario::step(&mut state, &[action], Duration::ZERO);
        assert_eq!(state.controller(), ControllerButtons::NONE);
    }
}
