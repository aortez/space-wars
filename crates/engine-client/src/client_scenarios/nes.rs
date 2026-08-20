use std::time::{Duration, Instant};

use engine_common::{
    Action, NativePixelFormat, NativeVideoFrame, NativeVideoTiming, RenderFrame, Settings,
    StepResult, TickModel,
};
use engine_nes::{AudioOutput, FrameInput, NES_PALETTE_RGB565};
use scenario_nes::{NesAction, NesScenarioConfig, NesScenarioError, NesScenarioState};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::nes_audio::CpalAudioOutput;
use crate::nes_realtime::{
    NesRealtimeRuntime, RealtimeNesCore, RealtimeNesFrame, RealtimeStartError, RealtimeTelemetry,
    RealtimeVideoConsumer,
};
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "nes",
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
    controls_help: "NES Library: each assigned gamepad is a standard NES controller. D-pad, A, B, Select, and Start are passed to the cartridge. Press Start+Select together for the host controls menu. Keyboard P1: arrows, Z/Space = A, X = B, Tab = Select, Enter = Start, Esc = host pause.",
    create,
};

pub(crate) struct NativeNesClientScenario {
    registration: &'static ScenarioRegistration,
    pub(crate) runtime: NesRealtimeRuntime,
}

struct CartridgeRealtimeCore {
    state: NesScenarioState,
}

fn create(
    _seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let ScenarioAsset::NesRom(asset) = asset else {
        return Err(ScenarioCreateError::MissingAsset {
            name: REGISTRATION.id,
            asset: "a selected .nes cartridge in the ROM library".into(),
        });
    };

    let audio_device = create_audio_output();
    let mut config = NesScenarioConfig::new(asset.image.clone());
    config.audio = if audio_device.is_some() {
        AudioOutput::Enabled
    } else {
        AudioOutput::Disabled
    };
    let state = NesScenarioState::try_new(config).map_err(|error| map_error(asset, error))?;
    tracing::info!(
        cartridge = %asset.display_name,
        path = %asset.source_path.display(),
        sha256 = %asset.digest,
        "loaded user NES cartridge."
    );
    spawn_native_nes_client(
        &REGISTRATION,
        CartridgeRealtimeCore { state },
        audio_device,
        settings,
    )
}

pub(super) fn spawn_native_nes_client(
    registration: &'static ScenarioRegistration,
    core: impl RealtimeNesCore,
    audio_device: Option<CpalAudioOutput>,
    settings: &Settings,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let runtime = match audio_device {
        Some(audio) => NesRealtimeRuntime::spawn_with_audio(core, audio),
        None => NesRealtimeRuntime::spawn(core),
    }
    .map_err(|error| map_realtime_start_error(registration.id, error))?;
    runtime.set_audio_volume(settings.audio.master_volume);
    runtime.set_audio_muted(settings.audio.muted);
    Ok(Box::new(NativeNesClientScenario {
        registration,
        runtime,
    }))
}

#[cfg(test)]
pub(super) fn create_audio_output() -> Option<CpalAudioOutput> {
    None
}

#[cfg(not(test))]
pub(super) fn create_audio_output() -> Option<CpalAudioOutput> {
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

fn map_realtime_start_error(name: &'static str, error: RealtimeStartError) -> ScenarioCreateError {
    ScenarioCreateError::RuntimeInitialization {
        name,
        detail: error.to_string(),
    }
}

fn map_error(asset: &crate::nes_roms::NesRomAsset, error: NesScenarioError) -> ScenarioCreateError {
    match error {
        NesScenarioError::Cartridge(error) => ScenarioCreateError::UnsupportedCartridge {
            name: REGISTRATION.id,
            detail: error.to_string(),
        },
        NesScenarioError::InvalidVisibleCrop(_) | NesScenarioError::Runtime(_) => {
            ScenarioCreateError::RuntimeInitialization {
                name: REGISTRATION.id,
                detail: format!("{}: {error}", asset.display_name),
            }
        }
    }
}

impl ClientScenario for NativeNesClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        self.registration
    }

    fn tick_model(&self) -> TickModel {
        TickModel::EmulatorClock
    }

    fn step(&mut self, _actions: &[Action], _dt: Duration) -> StepResult {
        StepResult::default()
    }

    fn map_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        vec![NesAction::set_controllers([
            input.nes_controller_buttons(0),
            input.nes_controller_buttons(1),
        ])]
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
        let controllers = actions
            .iter()
            .filter_map(NesAction::decode)
            .next_back()
            .unwrap_or_default();
        self.runtime.publish_input(controllers, observed_at);
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

impl RealtimeNesCore for CartridgeRealtimeCore {
    fn current_frame(&self) -> RealtimeNesFrame<'_> {
        RealtimeNesFrame {
            video: self.state.native_video_frame(),
            audio_samples: &[],
            frame_ppu_clocks: 0,
        }
    }

    fn advance_frame(&mut self, input: FrameInput) -> Result<RealtimeNesFrame<'_>, String> {
        let start_ppu_clocks = self.state.machine().ppu().timing().clocks;
        let visible_crop = self.state.native_video_frame().visible_crop;
        let result = self.state.advance_frame(input)?;
        let frame_ppu_clocks = result.timing.ppu_clocks;
        let emulated_ticks = start_ppu_clocks.wrapping_add(frame_ppu_clocks);
        let frame_id = result.frame_id;
        let input_sequence_id = result.input.sequence_id;
        let pixels = result
            .video
            .ok_or_else(|| "NES realtime worker lost native video output".to_string())?;
        Ok(RealtimeNesFrame {
            video: NativeVideoFrame {
                width: engine_nes::FRAME_WIDTH as u32,
                height: engine_nes::FRAME_HEIGHT as u32,
                visible_crop,
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

    use engine_nes::{ControllerButtons, test_rom::NromBuilder};

    use super::*;
    use crate::input::{GamepadInput, GamepadSeatInput};
    fn test_asset() -> ScenarioAsset {
        let bytes = NromBuilder::new_16k().build();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.nes");
        std::fs::write(&path, &bytes).unwrap();
        ScenarioAsset::NesRom(crate::nes_roms::load_path(&path).unwrap())
    }

    #[test]
    fn generic_client_maps_both_gamepad_seats() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_left: true,
                south: true,
                ..GamepadSeatInput::default()
            },
        );
        gamepads.borrow_mut().set_seat(
            1,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                east: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        let asset = test_asset();
        let scenario = create(
            0,
            &Settings::default(),
            Viewport::new(1280.0, 720.0),
            ScenarioStartMode::Normal,
            &asset,
        )
        .unwrap();

        let actions = scenario.map_input(&mut input, false);
        assert_eq!(
            NesAction::decode(&actions[0]),
            Some([
                ControllerButtons::LEFT | ControllerButtons::A,
                ControllerButtons::RIGHT | ControllerButtons::B,
            ])
        );
    }

    #[test]
    fn generic_client_requires_an_explicit_cartridge_asset() {
        let error = match create(
            0,
            &Settings::default(),
            Viewport::new(1280.0, 720.0),
            ScenarioStartMode::Normal,
            &ScenarioAsset::None,
        ) {
            Ok(_) => panic!("NES library must not start without a cartridge"),
            Err(error) => error,
        };
        assert!(matches!(error, ScenarioCreateError::MissingAsset { .. }));
    }
}
