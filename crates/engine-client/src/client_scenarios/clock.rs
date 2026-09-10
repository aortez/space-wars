use std::cell::Cell;
use std::time::Duration;

use chrono::{Local, Timelike};
use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "clock",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Clock follows local device time. Start or P/Esc pauses; choose Clock Controls to change 12/24-hour format, event profile and individual events without restarting. Or tap Clock Controls on the face. Calm runs occasional events, Demo runs frequent events, Off disables automatic events. Preview & Resume replaces the current animation with your chosen event, even if disabled. Settings are saved. Pause freezes animation.",
    create,
};

pub(crate) struct ClockClientScenario {
    pub(crate) state: ClockState,
    last_emitted_reading: Cell<Option<ClockReading>>,
}

impl ClockClientScenario {
    fn actions_for_reading(&self, reading: ClockReading) -> Vec<Action> {
        if self.last_emitted_reading.replace(Some(reading)) == Some(reading) {
            Vec::new()
        } else {
            vec![ClockAction::set_reading(reading)]
        }
    }
}

fn create(
    seed: u64,
    settings: &Settings,
    viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            time_format: settings.clock.time_format,
            event_profile: settings.clock.event_profile,
            events: settings.clock.events,
        },
        seed,
    );
    let reading = local_clock_reading();
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    Ok(Box::new(ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(Some(reading)),
    }))
}

impl ClientScenario for ClockClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        ClockScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        ClockScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, _input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        self.actions_for_reading(local_clock_reading())
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![ClockScenario::render_frame(&self.state)]
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        self.state.set_aspect_ratio(viewport.aspect_ratio());
    }

    fn clock_state(&self) -> Option<spacewars_control::ClockState> {
        Some(spacewars_control::ClockState {
            schema_version: spacewars_control::CLOCK_STATE_SCHEMA_VERSION,
            scenario_revision: 0, // Stamped by the host, not the scenario.
            paused: false,
            settings: self.state.settings(),
            profile: match self.state.event_profile() {
                engine_common::ClockEventProfile::Off => "off",
                engine_common::ClockEventProfile::Calm => "calm",
                engine_common::ClockEventProfile::Demo => "demo",
            }
            .into(),
            lifecycle: self.state.lifecycle().as_str().into(),
            event_kind: self.state.event_kind(),
            phase: self.state.event_phase().map(|phase| phase.as_str().into()),
            event_id: self.state.event_id(),
            phase_tick: self.state.phase_tick(),
            simulation_tick: self.state.simulation_tick(),
            next_event_tick: self.state.next_event_tick(),
            events: scenario_clock::EVENT_CATALOG
                .iter()
                .map(|event| spacewars_control::ClockEventInfo {
                    kind: event.kind,
                    label: event.kind.label().into(),
                    effect: event.effect.as_str().into(),
                    duration_ticks: event.duration_ticks,
                    cooldown_ticks: event.cooldown_ticks,
                    enabled: self.state.event_enabled(event.kind),
                    automatic_ready_at_tick: self.state.event_ready_at_tick(event.kind),
                })
                .collect(),
            palette_rgb: {
                let fill = self.state.palette().fill;
                [fill.r, fill.g, fill.b].map(|channel| (channel * 255.0).round() as u8)
            },
            body_count: self.state.body_count(),
            collider_count: self.state.collider_count(),
            meltdown: self.state.meltdown_state(),
            duck: self.state.duck_state(),
            reading: self
                .state
                .reading()
                .map(|reading| [reading.hour(), reading.minute(), reading.second()]),
            display_digits: self.state.display().digits,
            can_trigger: self.state.can_trigger_event(),
            trigger_pending: false,
        })
    }

    fn trigger_clock_event(&mut self, event: engine_common::ClockEventKind) {
        // Synchronize wall time at the client edge, including immediately after
        // a pause, before choosing which lit bars to release.
        let mut actions = self.actions_for_reading(local_clock_reading());
        actions.push(ClockAction::trigger_event(event));
        ClockScenario::step(&mut self.state, &actions, Duration::ZERO);
    }

    fn configure_clock(&mut self, settings: engine_common::ClockSettings) {
        let mut actions = self.actions_for_reading(local_clock_reading());
        actions.push(ClockAction::configure(settings));
        ClockScenario::step(&mut self.state, &actions, Duration::ZERO);
    }

    fn preview_clock_event(&mut self, event: engine_common::ClockEventKind) {
        let mut actions = self.actions_for_reading(local_clock_reading());
        actions.push(ClockAction::preview_event(event));
        ClockScenario::step(&mut self.state, &actions, Duration::ZERO);
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

fn local_clock_reading() -> ClockReading {
    let now = Local::now();
    ClockReading::new(now.hour() as u8, now.minute() as u8, now.second() as u8)
        .expect("chrono always returns a valid local clock reading")
}

#[cfg(test)]
mod tests {
    use engine_common::ClockTimeFormat;

    use super::*;

    #[test]
    fn factory_applies_a_valid_initial_local_reading_and_settings() {
        let mut settings = Settings::default();
        settings.clock.time_format = ClockTimeFormat::TwelveHour;
        let scenario = create(
            17,
            &settings,
            Viewport::new(800.0, 480.0),
            ScenarioStartMode::Normal,
            &ScenarioAsset::None,
        )
        .unwrap();
        let scenario = scenario
            .as_any()
            .downcast_ref::<ClockClientScenario>()
            .unwrap();

        assert!(scenario.state.reading().is_some());
        assert_eq!(scenario.state.time_format(), ClockTimeFormat::TwelveHour);
        assert_eq!(scenario.state.aspect_ratio(), 800.0 / 480.0);
    }

    #[test]
    fn adapter_suppresses_duplicate_readings() {
        let scenario = create(
            0,
            &Settings::default(),
            Viewport::new(800.0, 480.0),
            ScenarioStartMode::Normal,
            &ScenarioAsset::None,
        )
        .unwrap();
        let scenario = scenario
            .as_any()
            .downcast_ref::<ClockClientScenario>()
            .unwrap();
        let first = ClockReading::new(10, 20, 30).unwrap();
        let second = ClockReading::new(10, 20, 31).unwrap();
        scenario.last_emitted_reading.set(Some(first));
        assert!(scenario.actions_for_reading(first).is_empty());

        let actions = scenario.actions_for_reading(second);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            ClockAction::decode(&actions[0]),
            Some(ClockAction::SetReading(second))
        );
    }

    #[test]
    fn duck_course_reaches_both_render_paths_and_resets_to_the_normal_arena() {
        for viewport in [
            Viewport::new(800.0, 480.0),
            Viewport::new(480.0, 800.0),
            Viewport::new(1280.0, 720.0),
        ] {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: viewport.aspect_ratio(),
                    event_profile: engine_common::ClockEventProfile::Off,
                    ..ClockConfig::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(8, 8, 0).unwrap(),
                )],
                Duration::ZERO,
            );
            let normal = ClockScenario::render_frame(&state);
            ClockScenario::step(
                &mut state,
                &[ClockAction::trigger_event(
                    engine_common::ClockEventKind::Duck,
                )],
                Duration::ZERO,
            );
            let mut scenario = ClockClientScenario {
                state,
                last_emitted_reading: Cell::new(None),
            };
            let mut renderer = crate::raster::RasterRenderer::new();
            for tick in 0..=scenario_clock::DUCK_TICKS {
                if tick > 0 {
                    scenario.step(&[], Duration::from_nanos(16_666_667));
                }
                if ![0, 18, 60, 170, 260, 395, 460, 590, 600].contains(&tick) {
                    continue;
                }
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Vector, viewport)
                );
                assert!(
                    frames[0]
                        .layers
                        .iter()
                        .map(|l| l.primitives.len())
                        .sum::<usize>()
                        < 300
                );
                let presentation = crate::render::scene_presentation_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                );
                assert!(!presentation.main_primitives.is_empty());
                let image = renderer.image_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                    crate::raster::RasterOptions::default(),
                );
                let pixels = image.to_rgb8().unwrap();
                let yellow = pixels
                    .as_slice()
                    .iter()
                    .filter(|p| p.r > 240 && p.g > 200 && p.b < 50)
                    .count();
                if [60, 170, 260, 395].contains(&tick) {
                    assert!(yellow > 25, "duck missing at {tick} in {viewport:?}");
                }
                if tick == 0 || tick == 600 {
                    assert_eq!(yellow, 0);
                    assert_eq!(frames[0], normal);
                }
                if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let file = std::fs::File::create(directory.join(format!(
                        "duck-{tick}-{}x{}.png",
                        viewport.width, viewport.height
                    )))
                    .unwrap();
                    let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
                    encoder.set_color(png::ColorType::Rgb);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(pixels.as_bytes())
                        .unwrap();
                }
            }
        }
    }

    #[test]
    fn meltdown_reaches_both_render_paths_and_raster_water_is_visible() {
        for viewport in [
            Viewport::new(800.0, 480.0),
            Viewport::new(480.0, 800.0),
            Viewport::new(1280.0, 720.0),
        ] {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: viewport.aspect_ratio(),
                    event_profile: engine_common::ClockEventProfile::Off,
                    ..ClockConfig::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                    ClockAction::trigger_event(engine_common::ClockEventKind::Meltdown),
                ],
                Duration::ZERO,
            );
            let mut scenario = ClockClientScenario {
                state,
                last_emitted_reading: Cell::new(None),
            };
            let mut renderer = crate::raster::RasterRenderer::new();
            for tick in 0..=510 {
                if tick > 0 {
                    scenario.step(&[], Duration::from_nanos(16_666_667));
                }
                if ![0, 75, 125, 210, 450, 510].contains(&tick) {
                    continue;
                }
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Vector, viewport)
                );
                assert!(
                    frames[0]
                        .layers
                        .iter()
                        .map(|l| l.primitives.len())
                        .sum::<usize>()
                        <= 500
                );
                let presentation = crate::render::scene_presentation_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                );
                assert!(!presentation.main_primitives.is_empty());
                let image = renderer.image_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                    crate::raster::RasterOptions::default(),
                );
                let pixels = image.to_rgb8().unwrap();
                let blue = pixels
                    .as_slice()
                    .iter()
                    .filter(|p| p.r < 50 && p.g > 100 && p.b > 180)
                    .count();
                if tick == 125 || tick == 210 {
                    assert!(blue > 100, "missing water at tick {tick} {viewport:?}");
                }
                if tick == 0 || tick == 510 {
                    assert_eq!(blue, 0);
                }
                if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let file = std::fs::File::create(directory.join(format!(
                        "meltdown-{tick}-{}x{}.png",
                        viewport.width, viewport.height
                    )))
                    .unwrap();
                    let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
                    encoder.set_color(png::ColorType::Rgb);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(pixels.as_bytes())
                        .unwrap();
                }
            }
        }
    }
}
