use std::time::Duration;

use engine_common::{
    Action, NativeVideoFrame, RenderFrame, Scenario, Settings, StepResult, TickModel,
};
use engine_nes::CartridgeError;
use scenario_falling::{FallingAction, FallingConfig, FallingError, FallingScenario, FallingState};

use super::{
    ClientScenario, RenderBackend, ScenarioCapabilities, ScenarioCreateError, ScenarioRegistration,
    ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "falling",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
        native_video: true,
        captures_gamepad_start: true,
    },
    controls_help: "Falling: d-pad left/right moves during play; up/down chooses a title mode; Start begins or pauses the game. A/B are passed through for standard NES compatibility. Gamepad Select opens the host pause/controls menu. Keyboard: arrows, Z/Space = A, X = B, Tab = NES Select, Enter = Start, Esc = host pause.",
    create,
};

pub(crate) struct FallingClientScenario {
    pub(crate) state: FallingState,
}

fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let state = FallingScenario::try_init(FallingConfig::default(), seed).map_err(map_error)?;
    Ok(Box::new(FallingClientScenario { state }))
}

fn map_error(error: FallingError) -> ScenarioCreateError {
    match error {
        FallingError::Cartridge(error) if is_unsupported_cartridge(&error) => {
            ScenarioCreateError::UnsupportedCartridge {
                name: REGISTRATION.id,
                detail: error.to_string(),
            }
        }
        FallingError::Cartridge(error) => ScenarioCreateError::InvalidAsset {
            name: REGISTRATION.id,
            asset: "embedded falling.nes".into(),
            detail: error.to_string(),
        },
        FallingError::InvalidEmbeddedRom { expected, actual } => {
            ScenarioCreateError::InvalidAsset {
                name: REGISTRATION.id,
                asset: "embedded falling.nes".into(),
                detail: format!("expected {expected:?}, found {actual:?}"),
            }
        }
        FallingError::Runtime(error) => ScenarioCreateError::RuntimeInitialization {
            name: REGISTRATION.id,
            detail: error.to_string(),
        },
    }
}

fn is_unsupported_cartridge(error: &CartridgeError) -> bool {
    matches!(
        error,
        CartridgeError::UnsupportedNes2
            | CartridgeError::UnsupportedConsoleType(_)
            | CartridgeError::UnsupportedMapper(_)
            | CartridgeError::UnsupportedPrgRomBanks(_)
            | CartridgeError::UnsupportedChrRomBanks(_)
    )
}

impl ClientScenario for FallingClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        FallingScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        FallingScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        vec![FallingAction::set_controller(
            input.nes_controller_buttons(0),
        )]
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        Vec::new()
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }

    fn native_video_frame(&self) -> Option<NativeVideoFrame<'_>> {
        Some(self.state.native_video_frame())
    }

    fn runtime_error(&self) -> Option<&str> {
        self.state.runtime_error()
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use engine_nes::ControllerButtons;

    use super::*;
    use crate::input::{GameKey, GamepadInput, GamepadSeatInput};

    #[test]
    fn client_maps_nes_keyboard_and_gamepad_to_one_complete_action() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                south: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        input.press(GameKey::NesStart);
        let scenario = FallingClientScenario {
            state: FallingState::try_new(FallingConfig::default()).unwrap(),
        };

        let actions = scenario.map_input(&mut input, false);
        assert_eq!(
            FallingAction::decode(&actions[0]),
            Some(ControllerButtons::RIGHT | ControllerButtons::A | ControllerButtons::START)
        );
    }
}
