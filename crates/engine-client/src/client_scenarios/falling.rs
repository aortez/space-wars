use std::time::{Duration, Instant};

use engine_common::{
    Action, NativePixelFormat, NativeVideoFrame, NativeVideoTiming, RenderFrame, Scenario,
    Settings, StepResult, TickModel,
};
use engine_nes::{AudioOutput, CartridgeError, ControllerButtons, FrameInput, NES_PALETTE_RGB565};
use scenario_falling::{
    FALLING_VISIBLE_CROP, FallingAction, FallingConfig, FallingError, FallingScenario, FallingState,
};

use super::{
    ClientScenario, RenderBackend, ScenarioCapabilities, ScenarioCreateError, ScenarioRegistration,
    ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::nes_audio::CpalAudioOutput;
use crate::nes_realtime::{
    NesRealtimeRuntime, RealtimeNesCore, RealtimeNesFrame, RealtimeStartError, RealtimeTelemetry,
    RealtimeVideoConsumer,
};
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
    pub(crate) runtime: NesRealtimeRuntime,
}

struct FallingRealtimeCore {
    state: FallingState,
}

fn create(
    seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
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
    let runtime = match audio_device {
        Some(audio) => NesRealtimeRuntime::spawn_with_audio(core, audio),
        None => NesRealtimeRuntime::spawn(core),
    }
    .map_err(map_realtime_start_error)?;
    runtime.set_audio_volume(settings.audio.master_volume);
    runtime.set_audio_muted(settings.audio.muted);
    Ok(Box::new(FallingClientScenario { runtime }))
}

#[cfg(test)]
fn create_audio_output() -> Option<CpalAudioOutput> {
    // Unit tests validate the lock-free audio path with a synthetic callback;
    // they must not claim or depend on the developer's default device.
    None
}

#[cfg(not(test))]
fn create_audio_output() -> Option<CpalAudioOutput> {
    match CpalAudioOutput::try_default() {
        Ok(audio) => {
            tracing::info!(output = %audio.describe(), "initialized NES audio output.");
            Some(audio)
        }
        Err(error) => {
            tracing::warn!(%error, "NES audio is unavailable; continuing with silent emulation.");
            None
        }
    }
}

fn map_realtime_start_error(error: RealtimeStartError) -> ScenarioCreateError {
    ScenarioCreateError::RuntimeInitialization {
        name: REGISTRATION.id,
        detail: error.to_string(),
    }
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

    fn step(&mut self, _actions: &[Action], _dt: Duration) -> StepResult {
        // Realtime play advances on the worker. Deterministic callers use the
        // platform-independent FallingScenario directly.
        StepResult::default()
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

    fn realtime_video_consumer(&self) -> Option<RealtimeVideoConsumer> {
        Some(self.runtime.video_consumer())
    }

    fn publish_realtime_actions(&self, actions: &[Action], observed_at: Instant) {
        let controller = actions
            .iter()
            .filter_map(FallingAction::decode)
            .next_back()
            .unwrap_or(ControllerButtons::NONE);
        self.runtime
            .publish_input([controller, ControllerButtons::NONE], observed_at);
    }

    fn set_realtime_paused(&self, paused: bool) {
        self.runtime.set_paused(paused);
    }

    fn shutdown_realtime(&mut self) {
        self.runtime.stop_and_join();
    }

    fn record_realtime_displayed_loop_iteration(&self) {
        self.runtime.record_displayed_loop_iteration();
    }

    fn realtime_telemetry(&self) -> Option<RealtimeTelemetry> {
        Some(self.runtime.telemetry())
    }

    fn runtime_error(&self) -> Option<String> {
        self.runtime.runtime_error()
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
        let scenario = create(
            0,
            &Settings::default(),
            Viewport::new(1280.0, 720.0),
            ScenarioStartMode::Normal,
        )
        .unwrap();

        let actions = scenario.map_input(&mut input, false);
        assert_eq!(
            FallingAction::decode(&actions[0]),
            Some(ControllerButtons::RIGHT | ControllerButtons::A | ControllerButtons::START)
        );
    }
}
