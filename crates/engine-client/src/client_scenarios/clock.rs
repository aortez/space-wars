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

pub(super) mod benchmark;

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "clock",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        headless_benchmark: true,
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
    benchmark: Option<benchmark::Driver>,
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
    mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    if let ScenarioStartMode::Benchmark(config) = mode {
        let (state, driver) = benchmark::Driver::new(config.clock, seed, viewport.aspect_ratio());
        return Ok(Box::new(ClockClientScenario {
            state,
            last_emitted_reading: Cell::new(None),
            benchmark: Some(driver),
        }));
    }
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            duck_debug_overlay: std::env::var("SPACEWARS_CLOCK_DUCK_DEBUG")
                .is_ok_and(|value| value == "1"),
            duck_jump_profile: duck_jump_profile(
                std::env::var("SPACEWARS_CLOCK_DUCK_PROFILE")
                    .ok()
                    .as_deref(),
            ),
            time_format: settings.clock.time_format,
            duck_course_pattern: duck_course_pattern(
                std::env::var("SPACEWARS_CLOCK_DUCK_COURSE").ok().as_deref(),
            ),
            event_profile: settings.clock.event_profile,
            events: settings.clock.events,
            marquee_preset: settings.clock.marquee_preset,
            marquee_message: settings.clock.marquee_message,
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
        benchmark: None,
    }))
}

fn duck_jump_profile(value: Option<&str>) -> Option<engine_common::ClockDuckJumpProfile> {
    match value {
        Some("careful") => Some(engine_common::ClockDuckJumpProfile::Careful),
        Some("flowing") => Some(engine_common::ClockDuckJumpProfile::Flowing),
        _ => None,
    }
}

fn duck_course_pattern(value: Option<&str>) -> Option<engine_common::ClockDuckCoursePattern> {
    use engine_common::ClockDuckCoursePattern;
    match value {
        Some("platforms") => Some(ClockDuckCoursePattern::Platforms),
        Some("terraces") => Some(ClockDuckCoursePattern::Terraces),
        Some("two-jump") => Some(ClockDuckCoursePattern::TwoJump),
        Some("shortcut") => Some(ClockDuckCoursePattern::Shortcut),
        _ => None,
    }
}

impl ClientScenario for ClockClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        ClockScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        if let Some(driver) = &mut self.benchmark {
            let scripted = if dt.is_zero() {
                Vec::new()
            } else {
                driver.next_actions()
            };
            return ClockScenario::step(&mut self.state, &scripted, dt);
        }
        ClockScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, _input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        if self.benchmark.is_some() {
            return Vec::new();
        }
        self.actions_for_reading(local_clock_reading())
    }

    fn benchmark_counts(&self) -> Option<super::BenchmarkCounts> {
        Some(super::BenchmarkCounts {
            bodies: self.state.body_count(),
            colliders: self.state.collider_count(),
            clock_event_active: self.state.event_kind().is_some(),
            ..Default::default()
        })
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
                    trigger: event.trigger,
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
            marquee: self.state.marquee_state(),
            digit_slide: self.state.digit_slide_state(),
            reading: self
                .state
                .reading()
                .map(|reading| [reading.hour(), reading.minute(), reading.second()]),
            display_digits: self.state.display().digits,
            can_trigger: self.state.can_trigger_event(),
            trigger_pending: false,
            settings_pending: false,
            settings_error: None,
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
    fn digit_slide_clips_and_recovers_on_both_render_paths() {
        for viewport in [
            Viewport::new(800.0, 480.0),
            Viewport::new(480.0, 800.0),
            Viewport::new(1280.0, 720.0),
        ] {
            let from = ClockReading::new(12, 34, 59).unwrap();
            let to = ClockReading::new(12, 35, 0).unwrap();
            let config = ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                ..ClockConfig::default()
            };
            let mut state = ClockScenario::init(config, 42);
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(from)],
                Duration::ZERO,
            );
            let mut reference = ClockScenario::init(config, 42);
            ClockScenario::step(
                &mut reference,
                &[ClockAction::set_reading(to)],
                Duration::ZERO,
            );
            let normal = ClockScenario::render_frame(&reference);
            let mut scenario = ClockClientScenario {
                state,
                last_emitted_reading: Cell::new(Some(from)),
                benchmark: None,
            };
            let actions = scenario.actions_for_reading(to);
            scenario.step(&actions, Duration::from_nanos(16_666_667));
            assert_eq!(
                scenario
                    .clock_state()
                    .unwrap()
                    .digit_slide
                    .unwrap()
                    .changed_slots,
                [false, false, false, true]
            );
            let mut renderer = crate::raster::RasterRenderer::new();
            let normal_image = renderer
                .image_from_frames_with_layout(
                    std::slice::from_ref(&normal),
                    viewport,
                    scenario.frame_layout(),
                    crate::raster::RasterOptions::default(),
                )
                .to_rgb8()
                .unwrap();
            for tick in 1..=scenario_clock::DIGIT_SLIDE_TICKS {
                if tick > 1 {
                    scenario.step(&[], Duration::from_nanos(16_666_667));
                }
                if ![1, 12, 24, 36, 48].contains(&tick) {
                    continue;
                }
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Vector, viewport)
                );
                let presentation = crate::render::scene_presentation_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                );
                assert!(!presentation.main_primitives.is_empty());
                let pixels = renderer
                    .image_from_frames_with_layout(
                        &frames,
                        viewport,
                        scenario.frame_layout(),
                        crate::raster::RasterOptions::default(),
                    )
                    .to_rgb8()
                    .unwrap();
                // Only the rightmost slot moves. The colon uses the new reading
                // immediately; everything outside the last slot matches it.
                for (index, pixel) in pixels.as_slice().iter().enumerate() {
                    if index % (pixels.width() as usize) < pixels.width() as usize / 2 {
                        assert_eq!(pixel, &normal_image.as_slice()[index]);
                    }
                }
                if tick == scenario_clock::DIGIT_SLIDE_TICKS {
                    assert_eq!(frames[0], normal);
                    assert_eq!(pixels.as_bytes(), normal_image.as_bytes());
                    assert!(scenario.clock_state().unwrap().digit_slide.is_none());
                } else {
                    assert_ne!(pixels.as_bytes(), normal_image.as_bytes());
                }
                if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let file = std::fs::File::create(directory.join(format!(
                        "digit-slide-{tick}-{}x{}.png",
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
    fn marquee_recipes_render_on_both_backends_at_landscape_and_portrait_sizes() {
        for viewport in [
            Viewport::new(800.0, 480.0),
            Viewport::new(480.0, 800.0),
            Viewport::new(1280.0, 720.0),
        ] {
            for preset in engine_common::ClockMarqueePreset::ALL {
                let mut state = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: viewport.aspect_ratio(),
                        marquee_preset: preset,
                        event_profile: engine_common::ClockEventProfile::Off,
                        ..ClockConfig::default()
                    },
                    42,
                );
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::set_reading(
                        ClockReading::new(8, 24, 0).unwrap(),
                    )],
                    Duration::ZERO,
                );
                let normal = ClockScenario::render_frame(&state);
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::preview_event(
                        engine_common::ClockEventKind::Marquee,
                    )],
                    Duration::ZERO,
                );
                let mut scenario = ClockClientScenario {
                    state,
                    last_emitted_reading: Cell::new(None),
                    benchmark: None,
                };
                let mut renderer = crate::raster::RasterRenderer::new();
                let mut previous_frame = None;
                for tick in 0..=scenario_clock::MARQUEE_TICKS {
                    if tick > 0 {
                        scenario.step(&[], Duration::from_nanos(16_666_667));
                    }
                    if ![0, 180, 360, 600, scenario_clock::MARQUEE_TICKS].contains(&tick) {
                        continue;
                    }
                    let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                    assert_eq!(
                        frames,
                        scenario.render_frames(RenderBackend::Vector, viewport)
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
                    if tick == 0 || tick == scenario_clock::MARQUEE_TICKS {
                        assert_eq!(frames[0], normal);
                    } else {
                        let lit = pixels
                            .as_slice()
                            .iter()
                            .filter(|p| p.r > 140 || p.g > 140)
                            .count();
                        assert!(
                            lit > 100,
                            "missing content: {preset:?}, tick={tick}, {viewport:?}"
                        );
                        assert_ne!(frames[0], normal);
                        if let Some(previous) = &previous_frame {
                            assert_ne!(&frames[0], previous);
                        }
                        previous_frame = Some(frames[0].clone());
                    }
                    if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                        let directory = std::path::PathBuf::from(directory);
                        std::fs::create_dir_all(&directory).unwrap();
                        let file = std::fs::File::create(directory.join(format!(
                            "marquee-{}-{tick}-{}x{}.png",
                            preset as u8, viewport.width, viewport.height
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

    #[test]
    fn duck_course_reaches_both_render_paths_and_resets_to_the_normal_arena() {
        for (profile, pattern) in [
            engine_common::ClockDuckJumpProfile::Careful,
            engine_common::ClockDuckJumpProfile::Flowing,
        ]
        .into_iter()
        .flat_map(|profile| {
            [
                engine_common::ClockDuckCoursePattern::Platforms,
                engine_common::ClockDuckCoursePattern::Terraces,
                engine_common::ClockDuckCoursePattern::TwoJump,
                engine_common::ClockDuckCoursePattern::Shortcut,
            ]
            .map(|pattern| (profile, pattern))
        }) {
            for viewport in [
                Viewport::new(800.0, 480.0),
                Viewport::new(1024.0, 768.0),
                Viewport::new(480.0, 800.0),
                Viewport::new(1280.0, 720.0),
            ] {
                let mut state = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: viewport.aspect_ratio(),
                        duck_jump_profile: Some(profile),
                        duck_course_pattern: Some(pattern),
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
                    benchmark: None,
                };
                let mut renderer = crate::raster::RasterRenderer::new();
                for tick in 0..=scenario_clock::DUCK_TICKS {
                    if tick > 0 {
                        scenario.step(&[], Duration::from_nanos(16_666_667));
                    }
                    if ![
                        0,
                        18,
                        60,
                        170,
                        395,
                        600,
                        900,
                        1235,
                        1260,
                        1650,
                        1800,
                        scenario_clock::DUCK_TICKS,
                    ]
                    .contains(&tick)
                    {
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
                    // Shorter patterns may already be fading after a successful
                    // exit at this timestamp. Only inspect an active course.
                    let active_duck = scenario
                        .state
                        .duck_state()
                        .is_some_and(|duck| duck.outcome.is_none());
                    if [1235, 1260].contains(&tick) && active_duck {
                        // Inspect the exit frame itself, not just telemetry or its
                        // closed panel: it must be absent before the spawn timer.
                        let duck = scenario.state.duck_state().unwrap();
                        let world_width = 480.0 * viewport.aspect_ratio();
                        let radius = duck.navigation.unwrap().body_radius_milli as f32 / 1000.0;
                        let side = if duck.left_to_right { 1.0 } else { -1.0 };
                        let scale = viewport.height / 480.0;
                        let x = (viewport.width * 0.5
                            + (world_width * 0.5 - 2.0 * radius) * side * scale)
                            as usize;
                        let y = (viewport.height * 0.5 - (-163.2 + 4.1 * radius) * scale) as usize;
                        let frame_pixel = pixels.as_slice()[y * pixels.width() as usize + x];
                        let visible =
                            frame_pixel.r > 40 && frame_pixel.g > 80 && frame_pixel.b > 100;
                        assert_eq!(
                            visible,
                            tick == 1260,
                            "exit frame at {tick} in {viewport:?} {pattern:?} {profile:?}"
                        );
                    }
                    let yellow = pixels
                        .as_slice()
                        .iter()
                        .filter(|p| p.r > 240 && p.g > 200 && p.b < 50)
                        .count();
                    if [60, 170, 395, 600, 900, 1260].contains(&tick) && active_duck {
                        assert!(yellow > 25, "duck missing at {tick} in {viewport:?}");
                    }
                    if tick == 0 || tick == scenario_clock::DUCK_TICKS {
                        assert_eq!(yellow, 0);
                        assert_eq!(frames[0], normal);
                    }
                    if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                        let directory = std::path::PathBuf::from(directory);
                        std::fs::create_dir_all(&directory).unwrap();
                        let file = std::fs::File::create(directory.join(format!(
                            "duck-{pattern:?}-{profile:?}-{tick}-{}x{}.png",
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

    #[test]
    fn duck_planning_overlay_is_visible_without_changing_the_physics() {
        for profile in [
            engine_common::ClockDuckJumpProfile::Careful,
            engine_common::ClockDuckJumpProfile::Flowing,
        ] {
            let viewport = Viewport::new(800.0, 480.0);
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: viewport.aspect_ratio(),
                    duck_debug_overlay: true,
                    duck_jump_profile: Some(profile),
                    event_profile: engine_common::ClockEventProfile::Off,
                    ..ClockConfig::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                    ClockAction::preview_event(engine_common::ClockEventKind::Duck),
                ],
                Duration::ZERO,
            );
            for _ in 0..240 {
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            if profile == engine_common::ClockDuckJumpProfile::Flowing {
                // Capture the running arc, not an earlier careful fallback.
                for _ in 0..1000 {
                    let duck = state.duck_state().unwrap();
                    if !duck.grounded
                        && duck.navigation.unwrap().behavior
                            == engine_common::ClockDuckBehavior::Jumping
                        && duck
                            .navigation
                            .unwrap()
                            .planning
                            .unwrap()
                            .plan
                            .is_some_and(|plan| plan.running_takeoff)
                    {
                        break;
                    }
                    ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                }
                assert!(
                    state
                        .duck_state()
                        .unwrap()
                        .navigation
                        .unwrap()
                        .planning
                        .unwrap()
                        .plan
                        .unwrap()
                        .running_takeoff
                );
            }
            assert!(
                state
                    .duck_state()
                    .unwrap()
                    .navigation
                    .unwrap()
                    .planning
                    .unwrap()
                    .plan
                    .is_some()
            );
            let scenario = ClockClientScenario {
                state,
                last_emitted_reading: Cell::new(None),
                benchmark: None,
            };
            let frames = scenario.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(
                frames,
                scenario.render_frames(RenderBackend::Vector, viewport)
            );
            let mut renderer = crate::raster::RasterRenderer::new();
            let image = renderer.image_from_frames_with_layout(
                &frames,
                viewport,
                scenario.frame_layout(),
                crate::raster::RasterOptions::default(),
            );
            let pixels = image.to_rgb8().unwrap();
            let purple = pixels
                .as_slice()
                .iter()
                .filter(|pixel| {
                    pixel.r > 180 && pixel.r < 240 && pixel.g > 70 && pixel.g < 140 && pixel.b > 230
                })
                .count();
            assert!(purple > 20, "planned arc should be visible");
            if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                let file = std::fs::File::create(
                    directory.join(format!("duck-planner-{profile:?}-overlay.png")),
                )
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

    #[test]
    fn duck_profile_override_preserves_both_choices_and_defaults_to_a_seeded_mix() {
        use engine_common::ClockDuckJumpProfile;
        assert_eq!(
            duck_jump_profile(Some("flowing")),
            Some(ClockDuckJumpProfile::Flowing)
        );
        assert_eq!(
            duck_jump_profile(Some("careful")),
            Some(ClockDuckJumpProfile::Careful)
        );
        for value in [None, Some("mixed"), Some(""), Some("unknown")] {
            assert_eq!(duck_jump_profile(value), None);
        }
    }

    #[test]
    fn duck_course_override_preserves_authored_choices_and_defaults_to_a_seeded_mix() {
        use engine_common::ClockDuckCoursePattern;
        for (value, pattern) in [
            ("platforms", ClockDuckCoursePattern::Platforms),
            ("terraces", ClockDuckCoursePattern::Terraces),
            ("two-jump", ClockDuckCoursePattern::TwoJump),
            ("shortcut", ClockDuckCoursePattern::Shortcut),
        ] {
            assert_eq!(duck_course_pattern(Some(value)), Some(pattern));
        }
        for value in [None, Some("mixed"), Some("unknown")] {
            assert_eq!(duck_course_pattern(value), None);
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
                benchmark: None,
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
