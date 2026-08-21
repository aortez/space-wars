use engine_common::{NativePixelFormat, NativeVideoFrame, NativeVideoTiming, Settings};
use engine_nes::{AudioOutput, CartridgeError, FrameInput, NES_PALETTE_RGB565};
use scenario_falling::{
    FALLING_VISIBLE_CROP, FallingConfig, FallingError, FallingScenario, FallingState,
};
#[cfg(test)]
use scenario_nes::NesAction;

use super::{
    ClientScenario, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError, ScenarioRegistration,
    ScenarioStartMode,
};
use crate::nes_realtime::{RealtimeNesCore, RealtimeNesFrame};
use crate::render::Viewport;

use super::nes::{create_audio_output, spawn_native_nes_client};

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
        captures_gamepad_select: true,
    },
    controls_help: "Falling: d-pad left/right moves during play; up/down chooses a title mode; Start begins or pauses the game. A/B and Select are standard NES inputs. Press Start+Select together for the host controls menu. Keyboard: arrows, Z/Space = A, X = B, Tab = Select, Enter = Start, Esc = host pause.",
    create,
};

struct FallingRealtimeCore {
    state: FallingState,
}

fn create(
    seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let audio_device = create_audio_output();
    let state = FallingScenario::try_init(
        FallingConfig {
            audio: if audio_device.is_some() {
                AudioOutput::Enabled
            } else {
                AudioOutput::Disabled
            },
        },
        seed,
    )
    .map_err(map_error)?;
    let core = FallingRealtimeCore { state };
    spawn_native_nes_client(&REGISTRATION, core, audio_device, settings)
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
            | CartridgeError::UnsupportedPalTiming
            | CartridgeError::UnsupportedMapper(_)
            | CartridgeError::UnsupportedFourScreenMirroring(_)
            | CartridgeError::UnsupportedPrgRomBanks { .. }
            | CartridgeError::UnsupportedPrgRamBanks { .. }
            | CartridgeError::UnsupportedChrRomBanks { .. }
    )
}

impl RealtimeNesCore for FallingRealtimeCore {
    fn current_frame(&self) -> RealtimeNesFrame<'_> {
        RealtimeNesFrame {
            video: self.state.native_video_frame(),
            audio_samples: &[],
            frame_ppu_clocks: 0,
        }
    }

    fn advance_frame(&mut self, input: FrameInput) -> Result<RealtimeNesFrame<'_>, String> {
        let start_ppu_clocks = self.state.machine().ppu().timing().clocks;
        let result = self.state.advance_frame(input)?;
        let frame_ppu_clocks = result.timing.ppu_clocks;
        let emulated_ticks = start_ppu_clocks.wrapping_add(frame_ppu_clocks);
        let frame_id = result.frame_id;
        let input_sequence_id = result.input.sequence_id;
        let pixels = result
            .video
            .ok_or_else(|| "Falling realtime worker lost native video output".to_string())?;
        Ok(RealtimeNesFrame {
            video: NativeVideoFrame {
                width: engine_nes::FRAME_WIDTH as u32,
                height: engine_nes::FRAME_HEIGHT as u32,
                visible_crop: FALLING_VISIBLE_CROP,
                pixel_format: NativePixelFormat::Indexed8Rgb565,
                frame_id,
                pixels,
                palette_rgb565: &NES_PALETTE_RGB565,
                timing: Some(NativeVideoTiming {
                    emulated_ticks,
                    input_sequence_id,
                }),
            },
            audio_samples: result.audio_samples,
            frame_ppu_clocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use engine_nes::ControllerButtons;

    use super::*;
    use crate::input::{ClientInput, GameKey, GamepadInput, GamepadSeatInput};

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
        let scenario = create(
            0,
            &Settings::default(),
            Viewport::new(1280.0, 720.0),
            ScenarioStartMode::Normal,
            &ScenarioAsset::None,
        )
        .unwrap();

        let actions = scenario.map_input(&mut input, false);
        assert_eq!(
            NesAction::decode(&actions[0]),
            Some([
                ControllerButtons::RIGHT | ControllerButtons::A | ControllerButtons::START,
                ControllerButtons::NONE,
            ])
        );
    }
}
