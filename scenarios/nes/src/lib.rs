//! A deterministic, cartridge-agnostic NES scenario adapter.

use std::fmt;
use std::time::Duration;

use engine_common::{
    Action, NativePixelFormat, NativeVideoCrop, NativeVideoFrame, NativeVideoTiming, Observation,
    RenderFrame, Scenario, StepResult, TickModel,
};
use engine_nes::{
    AudioOutput, CartridgeError, CartridgeImage, ControllerButtons, FRAME_HEIGHT, FRAME_WIDTH,
    FrameInput, FrameResult, MachineConfig, MachineError, NES_PALETTE_RGB565, NesMachine,
    VideoOutput,
};

pub const NES_ACTION_VERSION: u16 = 1;
pub const NES_ACTION_SET_CONTROLLERS: u32 = 0;
pub const NES_OBSERVATION_VERSION: u16 = 1;

pub const STANDARD_VISIBLE_CROP: NativeVideoCrop = NativeVideoCrop {
    x: 0,
    y: 8,
    width: FRAME_WIDTH as u32,
    height: (FRAME_HEIGHT - 16) as u32,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NesScenarioConfig {
    pub cartridge: CartridgeImage,
    pub audio: AudioOutput,
    pub visible_crop: NativeVideoCrop,
}

impl NesScenarioConfig {
    pub fn new(cartridge: CartridgeImage) -> Self {
        Self {
            cartridge,
            audio: AudioOutput::Disabled,
            visible_crop: STANDARD_VISIBLE_CROP,
        }
    }
}

#[derive(Debug)]
pub enum NesScenarioError {
    Cartridge(CartridgeError),
    InvalidVisibleCrop(NativeVideoCrop),
    Runtime(MachineError),
}

impl fmt::Display for NesScenarioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cartridge(error) => error.fmt(formatter),
            Self::InvalidVisibleCrop(crop) => write!(
                formatter,
                "NES visible crop {crop:?} does not fit the {FRAME_WIDTH}x{FRAME_HEIGHT} frame"
            ),
            Self::Runtime(error) => write!(formatter, "NES machine startup failed: {error}"),
        }
    }
}

impl std::error::Error for NesScenarioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cartridge(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::InvalidVisibleCrop(_) => None,
        }
    }
}

impl From<CartridgeError> for NesScenarioError {
    fn from(error: CartridgeError) -> Self {
        Self::Cartridge(error)
    }
}

#[derive(Debug)]
pub struct NesScenarioState {
    machine: NesMachine,
    controllers: [ControllerButtons; 2],
    next_input_sequence: u64,
    runtime_error: Option<String>,
    visible_crop: NativeVideoCrop,
}

impl NesScenarioState {
    pub fn try_new(config: NesScenarioConfig) -> Result<Self, NesScenarioError> {
        if !crop_fits_frame(config.visible_crop) {
            return Err(NesScenarioError::InvalidVisibleCrop(config.visible_crop));
        }
        let mut machine = NesMachine::power_on(
            config.cartridge,
            MachineConfig {
                video: VideoOutput::Enabled,
                audio: config.audio,
                ..MachineConfig::default()
            },
        );
        machine
            .run_frame_with_input(FrameInput::new(
                0,
                [ControllerButtons::NONE, ControllerButtons::NONE],
            ))
            .map_err(NesScenarioError::Runtime)?;

        Ok(Self {
            machine,
            controllers: [ControllerButtons::NONE, ControllerButtons::NONE],
            next_input_sequence: 1,
            runtime_error: None,
            visible_crop: config.visible_crop,
        })
    }

    pub fn try_from_ines(bytes: &[u8], audio: AudioOutput) -> Result<Self, NesScenarioError> {
        let mut config = NesScenarioConfig::new(CartridgeImage::parse(bytes)?);
        config.audio = audio;
        Self::try_new(config)
    }

    pub fn machine(&self) -> &NesMachine {
        &self.machine
    }

    pub fn controllers(&self) -> [ControllerButtons; 2] {
        self.controllers
    }

    pub fn runtime_error(&self) -> Option<&str> {
        self.runtime_error.as_deref()
    }

    pub fn native_video_frame(&self) -> NativeVideoFrame<'_> {
        let pixels = self
            .machine
            .ppu()
            .framebuffer()
            .expect("NES scenarios always enable native video output");
        NativeVideoFrame {
            width: FRAME_WIDTH as u32,
            height: FRAME_HEIGHT as u32,
            visible_crop: self.visible_crop,
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

    pub fn advance_frame(&mut self, input: FrameInput) -> Result<FrameResult<'_>, String> {
        if let Some(error) = &self.runtime_error {
            return Err(error.clone());
        }

        self.controllers = input.controllers;
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
            if let Some(controllers) = NesAction::decode(action) {
                self.controllers = controllers;
            }
        }
    }

    fn run_frame(&mut self) -> StepResult {
        if self.runtime_error.is_some() {
            return StepResult { terminated: true };
        }
        let input = FrameInput::new(self.next_input_sequence, self.controllers);
        self.next_input_sequence = self.next_input_sequence.wrapping_add(1);
        match self.advance_frame(input) {
            Ok(_) => StepResult::default(),
            Err(_) => StepResult { terminated: true },
        }
    }
}

pub struct NesScenario;

impl NesScenario {
    pub fn try_init(
        config: NesScenarioConfig,
        _seed: u64,
    ) -> Result<NesScenarioState, NesScenarioError> {
        NesScenarioState::try_new(config)
    }
}

impl Scenario for NesScenario {
    type State = NesScenarioState;
    type Config = NesScenarioConfig;

    fn init(config: Self::Config, seed: u64) -> Self::State {
        Self::try_init(config, seed).expect("validated NES cartridge must initialize")
    }

    fn step(state: &mut Self::State, actions: &[Action], _dt: Duration) -> StepResult {
        state.apply_actions(actions);
        state.run_frame()
    }

    fn observe(state: &Self::State) -> Observation {
        NesObservation::from_state(state).encode()
    }

    fn render_frame(_state: &Self::State) -> RenderFrame {
        RenderFrame::default()
    }

    fn tick_model() -> TickModel {
        TickModel::EmulatorClock
    }
}

pub struct NesAction;

impl NesAction {
    pub fn set_controllers(controllers: [ControllerButtons; 2]) -> Action {
        let mut payload = Vec::with_capacity(4);
        payload.extend_from_slice(&NES_ACTION_VERSION.to_le_bytes());
        payload.push(controllers[0].bits());
        payload.push(controllers[1].bits());
        Action::scenario(NES_ACTION_SET_CONTROLLERS, payload)
    }

    pub fn decode(action: &Action) -> Option<[ControllerButtons; 2]> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        if *kind != NES_ACTION_SET_CONTROLLERS || payload.len() != 4 {
            return None;
        }
        let version = u16::from_le_bytes([payload[0], payload[1]]);
        (version == NES_ACTION_VERSION).then(|| {
            [
                ControllerButtons::from_bits(payload[2]),
                ControllerButtons::from_bits(payload[3]),
            ]
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NesObservation {
    pub frame_id: u64,
    pub state_hash_version: u16,
    pub state_hash: u64,
    pub controllers: [ControllerButtons; 2],
}

impl NesObservation {
    const PAYLOAD_BYTES: usize = 2 + 8 + 2 + 8 + 2;

    fn from_state(state: &NesScenarioState) -> Self {
        let hash = state.machine.state_hash();
        Self {
            frame_id: state.machine.ppu().frame_id(),
            state_hash_version: hash.version,
            state_hash: hash.value,
            controllers: state.controllers,
        }
    }

    fn encode(self) -> Observation {
        let mut payload = Vec::with_capacity(Self::PAYLOAD_BYTES);
        payload.extend_from_slice(&NES_OBSERVATION_VERSION.to_le_bytes());
        payload.extend_from_slice(&self.frame_id.to_le_bytes());
        payload.extend_from_slice(&self.state_hash_version.to_le_bytes());
        payload.extend_from_slice(&self.state_hash.to_le_bytes());
        payload.push(self.controllers[0].bits());
        payload.push(self.controllers[1].bits());
        Observation { payload }
    }

    pub fn decode(observation: &Observation) -> Option<Self> {
        let bytes = observation.payload.as_slice();
        if bytes.len() != Self::PAYLOAD_BYTES
            || u16::from_le_bytes([bytes[0], bytes[1]]) != NES_OBSERVATION_VERSION
        {
            return None;
        }
        Some(Self {
            frame_id: u64::from_le_bytes(bytes[2..10].try_into().ok()?),
            state_hash_version: u16::from_le_bytes(bytes[10..12].try_into().ok()?),
            state_hash: u64::from_le_bytes(bytes[12..20].try_into().ok()?),
            controllers: [
                ControllerButtons::from_bits(bytes[20]),
                ControllerButtons::from_bits(bytes[21]),
            ],
        })
    }
}

fn crop_fits_frame(crop: NativeVideoCrop) -> bool {
    crop.width != 0
        && crop.height != 0
        && crop
            .x
            .checked_add(crop.width)
            .is_some_and(|right| right <= FRAME_WIDTH as u32)
        && crop
            .y
            .checked_add(crop.height)
            .is_some_and(|bottom| bottom <= FRAME_HEIGHT as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_nes::test_rom::{Mmc3Builder, NromBuilder};

    fn state() -> NesScenarioState {
        NesScenarioState::try_from_ines(&NromBuilder::new_16k().build(), AudioOutput::Disabled)
            .unwrap()
    }

    #[test]
    fn generic_state_exposes_a_complete_native_frame() {
        let state = state();
        let frame = state.native_video_frame();
        assert!(frame.has_valid_layout());
        assert_eq!(frame.visible_crop, STANDARD_VISIBLE_CROP);
        assert_eq!(frame.timing.unwrap().input_sequence_id, 0);
    }

    #[test]
    fn actions_carry_two_complete_controller_snapshots() {
        let controllers = [
            ControllerButtons::LEFT | ControllerButtons::A,
            ControllerButtons::RIGHT | ControllerButtons::START,
        ];
        let action = NesAction::set_controllers(controllers);
        assert_eq!(NesAction::decode(&action), Some(controllers));

        let mut state = state();
        NesScenario::step(&mut state, &[action], Duration::ZERO);
        assert_eq!(state.controllers(), controllers);
        assert_eq!(
            state.machine().last_applied_input().controllers,
            controllers
        );
    }

    #[test]
    fn observation_is_cartridge_agnostic_and_versioned() {
        let state = state();
        let observation = NesScenario::observe(&state);
        let decoded = NesObservation::decode(&observation).unwrap();
        assert_eq!(decoded.frame_id, state.machine().ppu().frame_id());
        assert_eq!(decoded.state_hash, state.machine().state_hash().value);
        assert_eq!(decoded.controllers, [ControllerButtons::NONE; 2]);
    }

    #[test]
    fn observation_supports_four_screen_mmc3_cartridges() {
        let mut rom = Mmc3Builder::with_chr_rom(2, 1);
        rom.set_four_screen(true);
        let state = NesScenarioState::try_from_ines(&rom.build(), AudioOutput::Disabled).unwrap();

        let observation = NesScenario::observe(&state);
        let decoded = NesObservation::decode(&observation).unwrap();
        assert_eq!(decoded.state_hash, state.machine().state_hash().value);
    }

    #[test]
    fn invalid_crop_is_recoverable() {
        let image = CartridgeImage::parse(&NromBuilder::new_16k().build()).unwrap();
        let mut config = NesScenarioConfig::new(image);
        config.visible_crop = NativeVideoCrop {
            x: 255,
            y: 0,
            width: 2,
            height: 240,
        };
        assert!(matches!(
            NesScenarioState::try_new(config),
            Err(NesScenarioError::InvalidVisibleCrop(_))
        ));
    }
}
