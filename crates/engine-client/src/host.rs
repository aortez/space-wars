//! Scenario hosting loop for the Slint client.

use std::cell::RefCell;
use std::fmt;
use std::fs::File;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use engine_common::{
    Action, NativeVideoFrame, NativeVideoTiming, RenderFrame, Settings, StepResult, TickModel,
};
use engine_core::Color as CoreColor;
use slint::{
    Brush, Color as SlintColor, ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel,
};

use crate::MainWindow;
use crate::client_scenarios::{
    self, BenchmarkCounts, BenchmarkStepMetrics, CenterPanelState, ClientScenario, ScenarioAsset,
    ScenarioCreateError, ScenarioRegistration, ScenarioStartMode,
};
pub use crate::client_scenarios::{BenchmarkConfiguration, RenderBackend};
#[cfg(test)]
use crate::input;
use crate::input::{ClientInput, SharedInput};
use crate::native_video::{self, NativeVideoRenderer};
use crate::nes_realtime::{
    FrameWaker, RealtimeTelemetry, RealtimeVideoConsumer, RealtimeVideoMetadata,
};
use crate::raster;
use crate::render::{self, Viewport};

const TIMER_INTERVAL: Duration = Duration::from_millis(16);
const MAX_FIXED_STEPS_PER_TICK: usize = 5;
const BENCHMARK_VIEWPORT: Viewport = Viewport::new(1280.0, 720.0);
static NEXT_SCENARIO_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct ScenarioLoopOptions {
    pub start_benchmark: bool,
    pub benchmark_configuration: BenchmarkConfiguration,
    pub renderer: RenderBackend,
    pub raster_scale: f32,
    pub controls: Option<SharedScenarioControls>,
    pub input: Option<SharedInput>,
    pub settings: Settings,
    pub asset: ScenarioAsset,
}

impl Default for ScenarioLoopOptions {
    fn default() -> Self {
        Self {
            start_benchmark: false,
            benchmark_configuration: BenchmarkConfiguration::default(),
            renderer: RenderBackend::default(),
            raster_scale: 1.0,
            controls: None,
            input: None,
            settings: Settings::default(),
            asset: ScenarioAsset::None,
        }
    }
}

pub type SharedScenarioControls = Rc<RefCell<ScenarioControls>>;

#[derive(Debug, Default)]
pub struct ScenarioControls {
    request: Option<ScenarioControlRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenarioControlRequest {
    Pause,
    Resume,
    Restart,
    Benchmark,
    ZoomIn { player: usize },
    ZoomOut { player: usize },
}

pub fn new_scenario_controls() -> SharedScenarioControls {
    Rc::new(RefCell::new(ScenarioControls::default()))
}

impl ScenarioControls {
    pub fn request_pause(&mut self) {
        self.request = Some(ScenarioControlRequest::Pause);
    }

    pub fn request_resume(&mut self) {
        self.request = Some(ScenarioControlRequest::Resume);
    }

    pub fn request_restart(&mut self) {
        self.request = Some(ScenarioControlRequest::Restart);
    }

    pub fn request_benchmark(&mut self) {
        self.request = Some(ScenarioControlRequest::Benchmark);
    }

    pub fn request_zoom_in(&mut self, player: usize) {
        self.request = Some(ScenarioControlRequest::ZoomIn { player });
    }

    pub fn request_zoom_out(&mut self, player: usize) {
        self.request = Some(ScenarioControlRequest::ZoomOut { player });
    }

    pub fn clear(&mut self) {
        self.request = None;
    }

    fn take_request(&mut self) -> Option<ScenarioControlRequest> {
        self.request.take()
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkOptions {
    pub scenario: String,
    pub seed: u64,
    pub seconds: u64,
    pub report_path: Option<PathBuf>,
    pub renderer: RenderBackend,
    pub raster_scale: f32,
    pub configuration: BenchmarkConfiguration,
    pub settings: Settings,
}

pub enum HostError {
    UnknownScenario {
        name: String,
    },
    BenchmarkUnsupported {
        name: String,
    },
    ScenarioCreation {
        name: String,
        source: ScenarioCreateError,
    },
    Presentation {
        name: String,
        detail: String,
    },
}

impl fmt::Debug for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::UnknownScenario { name } => {
                write!(
                    f,
                    "unknown scenario {name:?}; available scenarios: {}",
                    scenario_names().join(", ")
                )
            }
            HostError::BenchmarkUnsupported { name } => {
                write!(f, "scenario {name:?} does not support benchmark mode")
            }
            HostError::ScenarioCreation { name, source } => {
                write!(f, "could not start scenario {name:?}: {source}")
            }
            HostError::Presentation { name, detail } => {
                write!(f, "could not present scenario {name:?}: {detail}")
            }
        }
    }
}

impl std::error::Error for HostError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ScenarioCreation { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn validate_scenario(name: &str) -> Result<(), HostError> {
    if is_known_scenario(name) {
        Ok(())
    } else {
        Err(HostError::UnknownScenario { name: name.into() })
    }
}

pub fn scenario_names() -> Vec<&'static str> {
    client_scenarios::registrations()
        .iter()
        .map(|registration| registration.id)
        .collect()
}

pub fn launcher_scenario_names() -> Vec<&'static str> {
    client_scenarios::launcher_registrations()
        .map(|registration| registration.id)
        .collect()
}

pub fn is_known_scenario(name: &str) -> bool {
    client_scenarios::registration(name).is_some()
}

pub fn scenario_registration(name: &str) -> Option<&'static ScenarioRegistration> {
    client_scenarios::registration(name)
}

pub fn start_debug_render_loop(
    window: &MainWindow,
    stress_triangles: usize,
    renderer: RenderBackend,
) -> Timer {
    set_center_panel(window, None);
    window.set_scenario_pointer_enabled(false);

    let timer = Timer::default();
    let weak_window = window.as_weak();
    let start = Instant::now();
    let mut frame_count = 0_u64;
    let mut raster_renderer = raster::RasterRenderer::new();

    timer.start(TimerMode::Repeated, TIMER_INTERVAL, move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };

        let convert_start = Instant::now();
        let frame = render::debug_frame(start.elapsed(), stress_triangles);
        let scene_item_count = present_frame(&window, frame, renderer, &mut raster_renderer);

        frame_count += 1;
        if frame_count % 120 == 0 {
            tracing::info!(
                stress_triangles,
                scene_item_count,
                convert_ms = convert_start.elapsed().as_secs_f64() * 1000.0,
                renderer = renderer.label(),
                "debug render frame presented."
            );
        }
    });

    timer
}

struct RealtimeNativeVideoPresenter {
    consumer: RealtimeVideoConsumer,
    renderer: NativeVideoRenderer,
    pixels: Vec<u8>,
    pending_error: Option<String>,
}

impl RealtimeNativeVideoPresenter {
    fn new(consumer: RealtimeVideoConsumer) -> Self {
        let pixel_count = consumer.descriptor().pixel_count();
        Self {
            consumer,
            renderer: NativeVideoRenderer::new(),
            pixels: vec![0; pixel_count],
            pending_error: None,
        }
    }

    fn present_latest(&mut self, window: &MainWindow) -> Result<Option<u64>, String> {
        let Some(metadata) = self
            .consumer
            .try_copy_latest(&mut self.pixels)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let descriptor = self.consumer.descriptor();
        let frame = NativeVideoFrame {
            width: descriptor.width,
            height: descriptor.height,
            visible_crop: descriptor.visible_crop,
            pixel_format: descriptor.pixel_format,
            frame_id: metadata.frame_id,
            pixels: &self.pixels,
            palette_rgb565: descriptor.palette_rgb565.as_ref(),
            timing: Some(NativeVideoTiming {
                emulated_ticks: metadata.emulated_ticks,
                input_sequence_id: metadata.input_sequence_id,
            }),
        };
        let frame_id = present_native_video(window, frame, &mut self.renderer)
            .map_err(|error| error.to_string())?;
        self.consumer.mark_submitted(frame_id);
        trace_realtime_frame(metadata);
        Ok(Some(frame_id))
    }

    fn present_or_record_error(&mut self, window: &MainWindow) {
        if let Err(error) = self.present_latest(window) {
            self.pending_error = Some(error);
        }
    }

    fn take_error(&mut self) -> Option<String> {
        self.pending_error.take()
    }
}

fn trace_realtime_frame(metadata: RealtimeVideoMetadata) {
    tracing::trace!(
        generation = metadata.generation,
        frame_id = metadata.frame_id,
        input_sequence_id = metadata.input_sequence_id,
        input_observed_us = metadata.input_observed_at.as_micros(),
        worker_sampled_us = metadata.worker_sampled_at.as_micros(),
        frame_completed_us = metadata.frame_completed_at.as_micros(),
        frame_published_us = metadata.frame_published_at.as_micros(),
        "consumed realtime native video frame."
    );
}

fn trace_realtime_telemetry(telemetry: RealtimeTelemetry) {
    if telemetry.displayed_loop_iterations == 0
        || !telemetry.displayed_loop_iterations.is_multiple_of(120)
    {
        return;
    }
    tracing::debug!(
        emulated_frames = telemetry.emulated_frames,
        produced_video_frames = telemetry.produced_video_frames,
        consumed_video_frames = telemetry.consumed_video_frames,
        submitted_video_frames = telemetry.submitted_video_frames,
        displayed_loop_iterations = telemetry.displayed_loop_iterations,
        coalesced_video_frames = telemetry.coalesced_video_frames,
        duplicate_video_polls = telemetry.duplicate_video_polls,
        wake_requests = telemetry.wake_requests,
        coalesced_wake_requests = telemetry.coalesced_wake_requests,
        catch_up_rebases = telemetry.catch_up_rebases,
        produced_audio_samples = telemetry.produced_audio_samples,
        audio_available = telemetry.audio_available,
        audio_active = telemetry.audio_active,
        audio_primed = telemetry.audio_primed,
        audio_muted = telemetry.audio_muted,
        audio_queue_depth_samples = telemetry.audio_queue_depth_samples,
        audio_target_depth_samples = telemetry.audio_target_depth_samples,
        audio_capacity_samples = telemetry.audio_capacity_samples,
        audio_high_water_samples = telemetry.audio_high_water_samples,
        consumed_audio_samples = telemetry.consumed_audio_samples,
        audio_underrun_samples = telemetry.audio_underrun_samples,
        audio_overflow_samples = telemetry.audio_overflow_samples,
        audio_callback_count = telemetry.audio_callback_count,
        audio_device_errors = telemetry.audio_device_errors,
        last_produced_frame_id = telemetry.last_produced_frame_id,
        last_submitted_frame_id = telemetry.last_submitted_frame_id,
        "NES realtime telemetry."
    );
}

fn native_video_waker(window: &MainWindow) -> FrameWaker {
    let weak_window = window.as_weak();
    std::sync::Arc::new(move || {
        if let Err(error) =
            weak_window.upgrade_in_event_loop(|window| window.invoke_native_video_ready())
        {
            tracing::debug!(%error, "could not queue native-video wakeup.");
        }
    })
}

fn replace_realtime_presenter(
    window: &MainWindow,
    presenter: &Rc<RefCell<Option<RealtimeNativeVideoPresenter>>>,
    consumer: Option<RealtimeVideoConsumer>,
) -> Result<bool, String> {
    *presenter.borrow_mut() = None;
    let Some(consumer) = consumer else {
        return Ok(false);
    };

    let mut next = RealtimeNativeVideoPresenter::new(consumer.clone());
    if next.present_latest(window)?.is_none() {
        return Err("realtime scenario did not publish its initial native frame".into());
    }
    *presenter.borrow_mut() = Some(next);
    consumer.set_waker(native_video_waker(window));
    Ok(true)
}

pub fn start_scenario_loop(
    window: &MainWindow,
    scenario: &str,
    seed: u64,
    options: ScenarioLoopOptions,
) -> Result<Timer, HostError> {
    let ScenarioLoopOptions {
        start_benchmark,
        benchmark_configuration,
        renderer,
        raster_scale,
        controls,
        input,
        settings,
        asset,
    } = options;
    let scenario_name = scenario.to_string();
    let initial_viewport = Viewport::from_window(window.window());
    let start_mode = if start_benchmark {
        ScenarioStartMode::Benchmark(benchmark_configuration)
    } else {
        ScenarioStartMode::Normal
    };
    let mut scenario = HostedScenario::new_with_asset(
        &scenario_name,
        seed,
        &settings,
        initial_viewport,
        start_mode,
        &asset,
    )?;
    scenario.set_viewport(initial_viewport);
    let tick_model = scenario.tick_model();
    let fixed_dt = fixed_step_duration(tick_model);
    let input = input.unwrap_or_else(|| Rc::new(RefCell::new(ClientInput::default())));
    {
        let mut input = input.borrow_mut();
        input.clear();
        input.reset_spacewars_controls();
    }
    let controls = controls.unwrap_or_else(new_scenario_controls);

    let timer = Timer::default();
    let weak_window = window.as_weak();
    let mut last_tick = Instant::now();
    let mut accumulator = Duration::ZERO;
    let mut paused = false;
    let mut benchmark_active = start_benchmark;
    let mut scenario_revision = next_scenario_revision();
    let mut performance = PerformanceStats::new(tick_model, last_tick);
    let input_diagnostics = input.borrow().runtime_diagnostics_text();
    window.set_runtime_diagnostics(SharedString::from(runtime_diagnostics_text(
        &scenario_name,
        scenario_revision,
        paused,
        benchmark_active,
        renderer,
        raster_scale,
        &performance,
        &input_diagnostics,
    )));
    let mut raster_renderer = raster::RasterRenderer::new();
    let mut native_video_renderer = NativeVideoRenderer::new();
    let mut presented_native_frame_id = None;
    let realtime_presenter: Rc<RefCell<Option<RealtimeNativeVideoPresenter>>> =
        Rc::new(RefCell::new(None));
    {
        let weak_window = window.as_weak();
        let callback_presenter = Rc::clone(&realtime_presenter);
        window.on_native_video_ready(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            if let Some(presenter) = callback_presenter.borrow_mut().as_mut() {
                presenter.present_or_record_error(&window);
            }
        });
    }
    let initial_frames = scenario.render_frames(renderer, initial_viewport);
    let mut input_projections =
        render::frame_projections(&initial_frames, initial_viewport, scenario.frame_layout());
    let mut projection_viewport = initial_viewport;
    window.set_scenario_pointer_enabled(scenario.registration().capabilities.pointer_input);
    window.set_scenario_error_text(SharedString::from(""));

    let has_realtime_presentation = replace_realtime_presenter(
        window,
        &realtime_presenter,
        scenario.realtime_video_consumer(),
    )
    .map_err(|detail| HostError::Presentation {
        name: scenario_name.clone(),
        detail,
    })?;
    if has_realtime_presentation {
        input_projections.clear();
        scenario.set_realtime_paused(false);
    } else if let Some(frame) = scenario.native_video_frame() {
        let frame_id =
            present_native_video(window, frame, &mut native_video_renderer).map_err(|error| {
                HostError::Presentation {
                    name: scenario_name.clone(),
                    detail: error.to_string(),
                }
            })?;
        presented_native_frame_id = Some(frame_id);
    } else {
        present_frames(
            window,
            initial_frames,
            scenario.frame_layout(),
            renderer,
            raster_scale,
            &mut raster_renderer,
        );
    }

    if benchmark_active {
        tracing::info!(scenario = scenario_name, seed, "started visual benchmark.");
    }

    let mut last_realtime_emulated_frames = 0;
    let mut last_diagnostics_revision = input.borrow().runtime_diagnostics_revision();
    let mut last_diagnostics_scenario_revision = scenario_revision;
    let mut last_diagnostics_paused = paused;
    let mut last_diagnostics_benchmark_active = benchmark_active;
    timer.start(TimerMode::Repeated, TIMER_INTERVAL, move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        if window.get_launcher_visible() {
            return;
        }

        let now = Instant::now();
        let elapsed = now.saturating_duration_since(last_tick);
        last_tick = now;
        if let Some(error) = realtime_presenter
            .borrow_mut()
            .as_mut()
            .and_then(RealtimeNativeVideoPresenter::take_error)
        {
            paused = true;
            accumulator = Duration::ZERO;
            input.borrow_mut().clear();
            scenario.set_realtime_paused(true);
            window.set_scenario_error_text(SharedString::from(format!(
                "Video presentation stopped: {error}. Restart or return to the launcher."
            )));
        }
        let viewport = Viewport::from_window(window.window());
        scenario.set_viewport(viewport);
        if viewport != projection_viewport {
            let projection_frames = scenario.render_frames(renderer, viewport);
            input_projections =
                render::frame_projections(&projection_frames, viewport, scenario.frame_layout());
            projection_viewport = viewport;
        }

        let mut input = input.borrow_mut();
        let step_result = step_scenario(
            &mut scenario,
            &scenario_name,
            seed,
            tick_model,
            fixed_dt,
            elapsed,
            &mut accumulator,
            &mut input,
            &mut controls.borrow_mut(),
            &mut paused,
            &mut benchmark_active,
            window.get_ingame_controls_visible(),
            &settings,
            viewport,
            &input_projections,
        );
        if step_result.return_to_launcher {
            window.invoke_ingame_return_launcher();
            return;
        }
        if let Some(visible) = step_result.ingame_controls_visible {
            window.set_ingame_controls_visible(visible);
        }
        if let Some(error_text) = step_result.scenario_error_text {
            window.set_scenario_error_text(SharedString::from(error_text));
        }
        if step_result.scenario_replaced {
            scenario_revision = next_scenario_revision();
            performance = PerformanceStats::new(tick_model, now);
            last_realtime_emulated_frames = 0;
            match replace_realtime_presenter(
                &window,
                &realtime_presenter,
                scenario.realtime_video_consumer(),
            ) {
                Ok(_) => {}
                Err(error) => {
                    paused = true;
                    accumulator = Duration::ZERO;
                    scenario.set_realtime_paused(true);
                    window.set_scenario_error_text(SharedString::from(format!(
                        "Video presentation stopped: {error}. Restart or return to the launcher."
                    )));
                }
            }
        }
        if window.get_launcher_visible() {
            return;
        }

        if let Some(error) = scenario.runtime_error() {
            paused = true;
            accumulator = Duration::ZERO;
            scenario.set_realtime_paused(true);
            window.set_scenario_error_text(SharedString::from(format!(
                "Scenario stopped: {error}. Restart or return to the launcher."
            )));
        }

        scenario.record_realtime_displayed_loop_iteration();
        let updates = if let Some(telemetry) = scenario.realtime_telemetry() {
            let updates = telemetry
                .emulated_frames
                .saturating_sub(last_realtime_emulated_frames) as usize;
            last_realtime_emulated_frames = telemetry.emulated_frames;
            trace_realtime_telemetry(telemetry);
            updates
        } else {
            step_result.updates
        };
        let performance_sample_completed = performance.record_frame(now, updates);
        let diagnostics_revision = input.runtime_diagnostics_revision();
        if performance_sample_completed
            || diagnostics_revision != last_diagnostics_revision
            || scenario_revision != last_diagnostics_scenario_revision
            || paused != last_diagnostics_paused
            || benchmark_active != last_diagnostics_benchmark_active
        {
            let input_diagnostics = input.runtime_diagnostics_text();
            window.set_runtime_diagnostics(SharedString::from(runtime_diagnostics_text(
                &scenario_name,
                scenario_revision,
                paused,
                benchmark_active,
                renderer,
                raster_scale,
                &performance,
                &input_diagnostics,
            )));
            last_diagnostics_revision = diagnostics_revision;
            last_diagnostics_scenario_revision = scenario_revision;
            last_diagnostics_paused = paused;
            last_diagnostics_benchmark_active = benchmark_active;
        }
        let performance_text = performance.display_text();
        set_center_panel(
            &window,
            scenario.center_panel_state(paused, benchmark_active, &performance_text),
        );
        let game_over = scenario.is_game_over();
        window.set_game_over_visible(game_over);
        set_ingame_menu(&window, paused && !game_over);
        window.set_scenario_pointer_enabled(scenario.registration().capabilities.pointer_input);
        if scenario.has_realtime_runtime() {
            input_projections.clear();
            projection_viewport = viewport;
        } else if let Some(frame) = scenario.native_video_frame() {
            input_projections.clear();
            projection_viewport = viewport;
            if presented_native_frame_id != Some(frame.frame_id) {
                match present_native_video(&window, frame, &mut native_video_renderer) {
                    Ok(frame_id) => presented_native_frame_id = Some(frame_id),
                    Err(error) => {
                        paused = true;
                        accumulator = Duration::ZERO;
                        window.set_scenario_error_text(SharedString::from(format!(
                            "Video presentation stopped: {error}. Restart or return to the launcher."
                        )));
                    }
                }
            }
        } else {
            let frames = scenario.render_frames(renderer, viewport);
            input_projections =
                render::frame_projections(&frames, viewport, scenario.frame_layout());
            projection_viewport = viewport;
            present_frames(
                &window,
                frames,
                scenario.frame_layout(),
                renderer,
                raster_scale,
                &mut raster_renderer,
            );
        }
    });

    Ok(timer)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct HostStepResult {
    updates: usize,
    return_to_launcher: bool,
    scenario_replaced: bool,
    ingame_controls_visible: Option<bool>,
    scenario_error_text: Option<String>,
}

impl HostStepResult {
    fn updates(updates: usize) -> Self {
        Self {
            updates,
            return_to_launcher: false,
            scenario_replaced: false,
            ingame_controls_visible: None,
            scenario_error_text: None,
        }
    }

    fn return_to_launcher() -> Self {
        Self {
            updates: 0,
            return_to_launcher: true,
            scenario_replaced: false,
            ingame_controls_visible: None,
            scenario_error_text: None,
        }
    }

    fn set_ingame_controls_visible(visible: bool) -> Self {
        Self {
            updates: 0,
            return_to_launcher: false,
            scenario_replaced: false,
            ingame_controls_visible: Some(visible),
            scenario_error_text: None,
        }
    }

    fn scenario_error(error: impl Into<String>) -> Self {
        Self {
            scenario_error_text: Some(error.into()),
            ..Self::default()
        }
    }

    fn scenario_restarted() -> Self {
        Self {
            scenario_replaced: true,
            scenario_error_text: Some(String::new()),
            ..Self::default()
        }
    }
}

fn step_scenario(
    scenario: &mut HostedScenario,
    scenario_name: &str,
    seed: u64,
    tick_model: TickModel,
    fixed_dt: Option<Duration>,
    elapsed: Duration,
    accumulator: &mut Duration,
    input: &mut ClientInput,
    controls: &mut ScenarioControls,
    paused: &mut bool,
    benchmark_active: &mut bool,
    ingame_controls_visible: bool,
    settings: &Settings,
    viewport: Viewport,
    input_projections: &[render::FrameProjection],
) -> HostStepResult {
    let result = step_scenario_inner(
        scenario,
        scenario_name,
        seed,
        tick_model,
        fixed_dt,
        elapsed,
        accumulator,
        input,
        controls,
        paused,
        benchmark_active,
        ingame_controls_visible,
        settings,
        viewport,
        input_projections,
    );
    scenario.set_realtime_paused(*paused || result.return_to_launcher);
    if result.return_to_launcher {
        scenario.shutdown_realtime();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn step_scenario_inner(
    scenario: &mut HostedScenario,
    scenario_name: &str,
    seed: u64,
    tick_model: TickModel,
    fixed_dt: Option<Duration>,
    elapsed: Duration,
    accumulator: &mut Duration,
    input: &mut ClientInput,
    controls: &mut ScenarioControls,
    paused: &mut bool,
    benchmark_active: &mut bool,
    ingame_controls_visible: bool,
    settings: &Settings,
    viewport: Viewport,
    input_projections: &[render::FrameProjection],
) -> HostStepResult {
    if input.has_pointer_cancellation() {
        deliver_pointer_cancellation(scenario, input, input_projections);
    }

    if let Some(request) = controls.take_request() {
        match request {
            ScenarioControlRequest::Pause => {
                if !scenario.is_game_over() {
                    *paused = true;
                    *accumulator = Duration::ZERO;
                    deliver_pointer_cancellation(scenario, input, input_projections);
                    input.clear();
                    tracing::info!(benchmark = *benchmark_active, "paused by host control.");
                }
                return HostStepResult::default();
            }
            ScenarioControlRequest::Resume => {
                *paused = false;
                *accumulator = Duration::ZERO;
                deliver_pointer_cancellation(scenario, input, input_projections);
                input.clear();
                tracing::info!(benchmark = *benchmark_active, "resumed from in-game menu.");
                return HostStepResult::default();
            }
            ScenarioControlRequest::Restart => {
                return match restart_scenario(
                    scenario,
                    scenario_name,
                    seed,
                    accumulator,
                    input,
                    paused,
                    benchmark_active,
                    settings,
                    viewport,
                ) {
                    Ok(()) => HostStepResult::scenario_restarted(),
                    Err(error) => {
                        *paused = true;
                        *accumulator = Duration::ZERO;
                        HostStepResult::scenario_error(format!(
                            "Restart failed: {error}. The previous game is still available."
                        ))
                    }
                };
            }
            ScenarioControlRequest::Benchmark => {
                if scenario.registration().capabilities.benchmark {
                    return match start_benchmark_scenario(
                        scenario,
                        scenario_name,
                        seed,
                        accumulator,
                        input,
                        paused,
                        benchmark_active,
                        settings,
                        viewport,
                    ) {
                        Ok(()) => HostStepResult::scenario_restarted(),
                        Err(error) => {
                            *paused = true;
                            *accumulator = Duration::ZERO;
                            HostStepResult::scenario_error(format!(
                                "Benchmark start failed: {error}. The previous game is still available."
                            ))
                        }
                    };
                }
                return HostStepResult::default();
            }
            ScenarioControlRequest::ZoomIn { player } => {
                scenario.zoom_player_in(player);
                return HostStepResult::default();
            }
            ScenarioControlRequest::ZoomOut { player } => {
                scenario.zoom_player_out(player);
                return HostStepResult::default();
            }
        }
    }

    if input.take_benchmark_requested() {
        if scenario.registration().capabilities.benchmark {
            return match start_benchmark_scenario(
                scenario,
                scenario_name,
                seed,
                accumulator,
                input,
                paused,
                benchmark_active,
                settings,
                viewport,
            ) {
                Ok(()) => HostStepResult::scenario_restarted(),
                Err(error) => {
                    *paused = true;
                    *accumulator = Duration::ZERO;
                    HostStepResult::scenario_error(format!(
                        "Benchmark start failed: {error}. The previous game is still available."
                    ))
                }
            };
        }
        return HostStepResult::default();
    }

    if input.take_reset_requested() {
        return match restart_scenario(
            scenario,
            scenario_name,
            seed,
            accumulator,
            input,
            paused,
            benchmark_active,
            settings,
            viewport,
        ) {
            Ok(()) => HostStepResult::scenario_restarted(),
            Err(error) => {
                *paused = true;
                *accumulator = Duration::ZERO;
                HostStepResult::scenario_error(format!(
                    "Restart failed: {error}. The previous game is still available."
                ))
            }
        };
    }

    if input.take_back_requested() {
        if scenario.is_game_over() {
            *paused = false;
            *accumulator = Duration::ZERO;
            input.clear();
            return HostStepResult::return_to_launcher();
        }
        if *paused {
            *accumulator = Duration::ZERO;
            input.clear();
            if ingame_controls_visible {
                return HostStepResult::set_ingame_controls_visible(false);
            }
            *paused = false;
            return HostStepResult::default();
        }

        *paused = true;
        *accumulator = Duration::ZERO;
        deliver_pointer_cancellation(scenario, input, input_projections);
        input.clear();
        tracing::info!(
            paused = *paused,
            benchmark = *benchmark_active,
            "toggled pause."
        );
        return HostStepResult::default();
    }

    if input.take_controls_requested() && !scenario.is_game_over() {
        *accumulator = Duration::ZERO;
        deliver_pointer_cancellation(scenario, input, input_projections);
        input.clear();
        if !*paused {
            *paused = true;
            return HostStepResult::set_ingame_controls_visible(true);
        }
        return HostStepResult::set_ingame_controls_visible(!ingame_controls_visible);
    }

    if input.take_return_launcher_requested() && (*paused || scenario.is_game_over()) {
        *paused = false;
        *accumulator = Duration::ZERO;
        input.clear();
        return HostStepResult::return_to_launcher();
    }

    if input.take_force_pause_requested() && !scenario.is_game_over() {
        *paused = true;
        *accumulator = Duration::ZERO;
        deliver_pointer_cancellation(scenario, input, input_projections);
        input.clear();
        tracing::info!(
            benchmark = *benchmark_active,
            "paused after controller disconnect."
        );
        return HostStepResult::default();
    }

    if input.take_pause_requested() && !scenario.is_game_over() {
        *paused = !*paused;
        *accumulator = Duration::ZERO;
        deliver_pointer_cancellation(scenario, input, input_projections);
        input.clear();
        tracing::info!(
            paused = *paused,
            benchmark = *benchmark_active,
            "toggled pause."
        );
        return HostStepResult::default();
    }

    if *paused || scenario.is_game_over() {
        return HostStepResult::default();
    }

    if scenario.has_realtime_runtime() {
        let actions = scenario.actions(input, *benchmark_active, input_projections);
        scenario.publish_realtime_actions(&actions, Instant::now());
        return HostStepResult::default();
    }

    match (tick_model, fixed_dt) {
        (TickModel::FixedTimestep { .. } | TickModel::EmulatorClock, Some(dt)) => {
            *accumulator += elapsed;
            let mut steps = 0;
            while *accumulator >= dt && steps < MAX_FIXED_STEPS_PER_TICK {
                let actions = scenario.actions(input, *benchmark_active, input_projections);
                scenario.step(&actions, dt);
                *accumulator -= dt;
                steps += 1;
            }
            if steps == MAX_FIXED_STEPS_PER_TICK {
                *accumulator = Duration::ZERO;
            }
            HostStepResult::updates(steps)
        }
        (TickModel::Variable, _) => {
            let actions = scenario.actions(input, *benchmark_active, input_projections);
            scenario.step(&actions, elapsed);
            HostStepResult::updates(1)
        }
        (TickModel::FixedTimestep { .. } | TickModel::EmulatorClock, None) => {
            HostStepResult::default()
        }
    }
}

fn deliver_pointer_cancellation(
    scenario: &mut HostedScenario,
    input: &mut ClientInput,
    input_projections: &[render::FrameProjection],
) {
    input.cancel_pointer();
    let actions = scenario.pointer_actions(input, input_projections);
    if !actions.is_empty() {
        scenario.step(&actions, Duration::ZERO);
    }
}

fn restart_scenario(
    scenario: &mut HostedScenario,
    scenario_name: &str,
    seed: u64,
    accumulator: &mut Duration,
    input: &mut ClientInput,
    paused: &mut bool,
    benchmark_active: &mut bool,
    settings: &Settings,
    viewport: Viewport,
) -> Result<(), HostError> {
    let asset = scenario.asset.clone();
    replace_scenario(scenario, || {
        HostedScenario::new_with_asset(
            scenario_name,
            seed,
            settings,
            viewport,
            ScenarioStartMode::Normal,
            &asset,
        )
    })?;
    *accumulator = Duration::ZERO;
    *paused = false;
    *benchmark_active = false;
    input.clear();
    input.reset_spacewars_controls();
    tracing::info!(scenario = scenario_name, seed, "started new game.");
    Ok(())
}

fn replace_scenario(
    current: &mut HostedScenario,
    create: impl FnOnce() -> Result<HostedScenario, HostError>,
) -> Result<(), HostError> {
    // Construction completes before assignment so failure leaves the current
    // live scenario available for resume, another restart, or launcher return.
    let replacement = create()?;
    current.shutdown_realtime();
    *current = replacement;
    Ok(())
}

fn start_benchmark_scenario(
    scenario: &mut HostedScenario,
    scenario_name: &str,
    seed: u64,
    accumulator: &mut Duration,
    input: &mut ClientInput,
    paused: &mut bool,
    benchmark_active: &mut bool,
    settings: &Settings,
    viewport: Viewport,
) -> Result<(), HostError> {
    let asset = scenario.asset.clone();
    replace_scenario(scenario, || {
        HostedScenario::new_with_asset(
            scenario_name,
            seed,
            settings,
            viewport,
            ScenarioStartMode::Benchmark(BenchmarkConfiguration::default()),
            &asset,
        )
    })?;
    *accumulator = Duration::ZERO;
    *paused = false;
    *benchmark_active = true;
    input.clear();
    input.reset_spacewars_controls();
    tracing::info!(scenario = scenario_name, seed, "started visual benchmark.");
    Ok(())
}

fn set_ingame_menu(window: &MainWindow, paused: bool) {
    let visible = paused && !window.get_launcher_visible();
    if visible && !window.get_ingame_menu_visible() {
        // Reset focus before exposing the menu so external state observers do
        // not see a transient selection carried over from the previous pause.
        window.set_ingame_menu_focus_index(0);
    }
    window.set_ingame_menu_visible(visible);
    if !visible {
        window.set_ingame_controls_visible(false);
    }
}

#[derive(Debug, Clone)]
struct PerformanceStats {
    target_label: String,
    sample_started: Instant,
    frames_in_sample: u32,
    updates_in_sample: u32,
    frames_total: u64,
    updates_total: u64,
    measured_fps: Option<f32>,
    measured_ups: Option<f32>,
}

impl PerformanceStats {
    fn new(tick_model: TickModel, now: Instant) -> Self {
        Self {
            target_label: performance_target_label(tick_model),
            sample_started: now,
            frames_in_sample: 0,
            updates_in_sample: 0,
            frames_total: 0,
            updates_total: 0,
            measured_fps: None,
            measured_ups: None,
        }
    }

    fn record_frame(&mut self, now: Instant, updates: usize) -> bool {
        self.frames_in_sample += 1;
        self.updates_in_sample += updates as u32;
        self.frames_total = self.frames_total.saturating_add(1);
        self.updates_total = self.updates_total.saturating_add(updates as u64);

        let elapsed = now.saturating_duration_since(self.sample_started);
        let elapsed_secs = elapsed.as_secs_f32();
        if elapsed_secs < 1.0 {
            return false;
        }

        self.measured_fps = Some(self.frames_in_sample as f32 / elapsed_secs);
        self.measured_ups = Some(self.updates_in_sample as f32 / elapsed_secs);
        self.sample_started = now;
        self.frames_in_sample = 0;
        self.updates_in_sample = 0;
        true
    }

    fn display_text(&self) -> String {
        format!(
            "Target {} | FPS {} | UPS {}",
            self.target_label,
            measured_label(self.measured_fps),
            measured_label(self.measured_ups)
        )
    }

    fn diagnostics_text(&self) -> String {
        format!(
            "performance_target={}\nfps={}\nups={}\nframes_total={}\nupdates_total={}",
            self.target_label,
            measured_diagnostics_label(self.measured_fps),
            measured_diagnostics_label(self.measured_ups),
            self.frames_total,
            self.updates_total,
        )
    }
}

fn runtime_diagnostics_text(
    scenario_name: &str,
    scenario_revision: u64,
    paused: bool,
    benchmark_active: bool,
    renderer: RenderBackend,
    raster_scale: f32,
    performance: &PerformanceStats,
    input_diagnostics: &str,
) -> String {
    format!(
        "scenario={scenario_name}\nscenario_revision={scenario_revision}\npaused={paused}\nbenchmark_active={benchmark_active}\nrenderer={}\nraster_scale={raster_scale:.2}\n{}\n{input_diagnostics}",
        renderer.label(),
        performance.diagnostics_text(),
    )
}

fn next_scenario_revision() -> u64 {
    NEXT_SCENARIO_REVISION.fetch_add(1, Ordering::Relaxed)
}

fn performance_target_label(tick_model: TickModel) -> String {
    match tick_model {
        TickModel::FixedTimestep { hz } => format!("{hz} Hz"),
        TickModel::Variable => "variable".into(),
        TickModel::EmulatorClock => "emulator 60.10 Hz".into(),
    }
}

fn measured_label(value: Option<f32>) -> String {
    value
        .map(|value| format!("{:.0}", value.max(0.0)))
        .unwrap_or_else(|| "--".into())
}

fn measured_diagnostics_label(value: Option<f32>) -> String {
    value
        .map(|value| format!("{:.1}", value.max(0.0)))
        .unwrap_or_else(|| "--".into())
}

fn fixed_step_duration(tick_model: TickModel) -> Option<Duration> {
    match tick_model {
        TickModel::FixedTimestep { hz } => Some(Duration::from_secs_f64(1.0 / hz.max(1) as f64)),
        TickModel::Variable | TickModel::EmulatorClock => None,
    }
}

fn present_frame(
    window: &MainWindow,
    frame: RenderFrame,
    renderer: RenderBackend,
    raster_renderer: &mut raster::RasterRenderer,
) -> usize {
    present_frames(
        window,
        vec![frame],
        render::FrameLayout::EqualHorizontal,
        renderer,
        1.0,
        raster_renderer,
    )
}

fn present_frames(
    window: &MainWindow,
    frames: Vec<RenderFrame>,
    layout: render::FrameLayout,
    renderer: RenderBackend,
    raster_scale: f32,
    raster_renderer: &mut raster::RasterRenderer,
) -> usize {
    let viewport = Viewport::from_window(window.window());
    window.set_native_video_visible(false);
    let scene_item_count = match renderer {
        RenderBackend::Vector => {
            let presentation =
                render::scene_presentation_from_frames_with_layout(&frames, viewport, layout);
            let scene_item_count = presentation.scene_item_count();
            window.set_raster_visible(false);
            set_vector_presentation(window, presentation);
            scene_item_count
        }
        RenderBackend::Raster => {
            let primitive_count = raster::primitive_count(&frames);
            let image = raster_renderer.image_from_frames_with_layout(
                &frames,
                scaled_viewport(viewport, raster_scale),
                layout,
                raster::RasterOptions::for_scale(raster_scale),
            );
            window.set_primitives(ModelRc::new(VecModel::from(Vec::new())));
            window.set_vector_minimaps_visible(false);
            window.set_raster_frame(image);
            window.set_raster_visible(true);
            primitive_count
        }
    };
    window.window().request_redraw();
    scene_item_count
}

fn present_native_video(
    window: &MainWindow,
    frame: NativeVideoFrame<'_>,
    renderer: &mut NativeVideoRenderer,
) -> Result<u64, native_video::NativeVideoError> {
    let presentation = renderer.present(frame)?;
    let crop = presentation.source_crop;
    window.set_primitives(ModelRc::new(VecModel::from(Vec::new())));
    window.set_vector_minimaps_visible(false);
    window.set_raster_visible(false);
    window.set_native_video_frame(presentation.image);
    window.set_native_video_crop_x(native_video::slint_dimension(crop.x)?);
    window.set_native_video_crop_y(native_video::slint_dimension(crop.y)?);
    window.set_native_video_crop_width(native_video::slint_dimension(crop.width)?);
    window.set_native_video_crop_height(native_video::slint_dimension(crop.height)?);
    window.set_native_video_visible(true);
    window.window().request_redraw();
    if let Some(timing) = presentation.timing {
        tracing::trace!(
            frame_id = presentation.frame_id,
            emulated_ticks = timing.emulated_ticks,
            input_sequence_id = timing.input_sequence_id,
            "submitted native video frame."
        );
    }
    Ok(presentation.frame_id)
}

fn set_vector_presentation(window: &MainWindow, presentation: render::VectorPresentation) {
    window.set_primitives(ModelRc::new(VecModel::from(presentation.main_primitives)));
    window.set_minimap_opacity(render::SPACEWARS_MINIMAP_OPACITY);

    let mut minimaps = presentation.minimaps.into_iter();
    let Some(player_1) = minimaps.next() else {
        window.set_vector_minimaps_visible(false);
        return;
    };
    let Some(player_2) = minimaps.next() else {
        window.set_vector_minimaps_visible(false);
        return;
    };

    window.set_p1_minimap_x(player_1.viewport.x);
    window.set_p1_minimap_y(player_1.viewport.y);
    window.set_p1_minimap_size(player_1.viewport.width);
    window.set_p1_minimap_primitives(ModelRc::new(VecModel::from(player_1.primitives)));
    window.set_p2_minimap_x(player_2.viewport.x);
    window.set_p2_minimap_y(player_2.viewport.y);
    window.set_p2_minimap_size(player_2.viewport.width);
    window.set_p2_minimap_primitives(ModelRc::new(VecModel::from(player_2.primitives)));
    window.set_vector_minimaps_visible(true);
}

pub fn run_benchmark(options: BenchmarkOptions) -> Result<(), Box<dyn std::error::Error>> {
    let seconds = options.seconds.max(1);
    let mut scenario = HostedScenario::new(
        &options.scenario,
        options.seed,
        &options.settings,
        BENCHMARK_VIEWPORT,
        ScenarioStartMode::Benchmark(options.configuration),
    )
    .expect("validated benchmark scenario should construct");
    let tick_model = scenario.tick_model();
    let fixed_dt = fixed_step_duration(tick_model).unwrap_or(Duration::from_secs_f64(1.0 / 60.0));
    let mut input = ClientInput::default();
    let mut raster_renderer = raster::RasterRenderer::new();
    let mut report_file = match options.report_path {
        Some(path) => Some(File::create(path)?),
        None => None,
    };

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    write_benchmark_header(&mut stdout)?;
    if let Some(file) = &mut report_file {
        write_benchmark_header(file)?;
    }

    for second in 1..=seconds {
        let mut sample = BenchmarkSample::default();
        let sample_started = Instant::now();

        for _ in 0..60 {
            let frame_started = Instant::now();

            let step_started = Instant::now();
            let actions = scenario.actions(&mut input, true, &[]);
            scenario.step(&actions, fixed_dt);
            let step_time = step_started.elapsed();
            sample.step_time += step_time;
            sample.step_samples.push(step_time);
            let scenario_metrics = scenario.benchmark_step_metrics();
            sample.max_lifecycle_time = sample
                .max_lifecycle_time
                .max(scenario_metrics.lifecycle_time);
            sample.scenario_metrics += scenario_metrics;
            let counts = scenario.benchmark_counts();
            sample.max_candidate_pairs = sample.max_candidate_pairs.max(counts.candidate_pairs);
            sample.max_contact_pairs = sample.max_contact_pairs.max(counts.contact_pairs);
            sample.max_contacts = sample.max_contacts.max(counts.contacts);

            let render_started = Instant::now();
            let frames = scenario.render_frames(options.renderer, BENCHMARK_VIEWPORT);
            sample.render_time += render_started.elapsed();

            let present_started = Instant::now();
            let presentation = present_frames_for_benchmark(
                &frames,
                scenario.frame_layout(),
                options.renderer,
                options.raster_scale,
                &mut raster_renderer,
            );
            sample.scene_items = presentation.scene_items;
            sample.raster_timings += presentation.raster_timings;
            sample.present_time += present_started.elapsed();

            sample.frames += 1;
            sample.updates += 1;
            let total_time = frame_started.elapsed();
            sample.max_total_time = sample.max_total_time.max(total_time);
            sample.total_samples.push(total_time);
        }

        sample.wall_time = sample_started.elapsed();
        let row = BenchmarkRow::from_sample(
            &options.scenario,
            second,
            options.renderer,
            options.raster_scale,
            sample,
            scenario.benchmark_counts(),
        );
        write_benchmark_row(&mut stdout, &row)?;
        if let Some(file) = &mut report_file {
            write_benchmark_row(file, &row)?;
        }
    }

    Ok(())
}

fn present_frames_for_benchmark(
    frames: &[RenderFrame],
    layout: render::FrameLayout,
    renderer: RenderBackend,
    raster_scale: f32,
    raster_renderer: &mut raster::RasterRenderer,
) -> PresentationStats {
    match renderer {
        RenderBackend::Vector => {
            let presentation = render::scene_presentation_from_frames_with_layout(
                frames,
                BENCHMARK_VIEWPORT,
                layout,
            );
            let scene_item_count = presentation.scene_item_count();
            black_box(presentation);
            PresentationStats {
                scene_items: scene_item_count,
                raster_timings: raster::RasterTimings::default(),
            }
        }
        RenderBackend::Raster => {
            let primitive_count = raster::primitive_count(frames);
            let result = raster_renderer.image_from_frames_with_layout_timed(
                frames,
                scaled_viewport(BENCHMARK_VIEWPORT, raster_scale),
                layout,
                raster::RasterOptions::for_scale(raster_scale),
            );
            black_box(result.image);
            PresentationStats {
                scene_items: primitive_count,
                raster_timings: result.timings,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PresentationStats {
    scene_items: usize,
    raster_timings: raster::RasterTimings,
}

fn scaled_viewport(viewport: Viewport, raster_scale: f32) -> Viewport {
    let scale = raster_scale.clamp(0.1, 3.0);
    Viewport::new(
        (viewport.width * scale).max(1.0),
        (viewport.height * scale).max(1.0),
    )
}

#[derive(Debug, Default)]
struct BenchmarkSample {
    frames: u32,
    updates: u32,
    scene_items: usize,
    step_time: Duration,
    render_time: Duration,
    present_time: Duration,
    raster_timings: raster::RasterTimings,
    scenario_metrics: BenchmarkStepMetrics,
    step_samples: Vec<Duration>,
    total_samples: Vec<Duration>,
    max_lifecycle_time: Duration,
    max_candidate_pairs: usize,
    max_contact_pairs: usize,
    max_contacts: usize,
    max_total_time: Duration,
    wall_time: Duration,
}

#[derive(Debug)]
struct BenchmarkRow {
    scenario: String,
    renderer: &'static str,
    raster_scale: f32,
    second: u64,
    frames: u32,
    updates: u32,
    throughput_fps: f64,
    scene_items: usize,
    asteroids: usize,
    fragments: usize,
    shells: usize,
    particles: usize,
    balls: usize,
    gravity_sources: usize,
    gravity_targets: usize,
    gravity_nodes: usize,
    gravity_exact_interactions: u64,
    gravity_approximations: u64,
    gravity_applied_sources: u64,
    active_bodies: usize,
    sleeping_bodies: usize,
    candidate_pairs: usize,
    max_candidate_pairs: usize,
    contact_pairs: usize,
    max_contact_pairs: usize,
    contacts: usize,
    max_contacts: usize,
    added: usize,
    removed: usize,
    avg_step_ms: f64,
    p50_step_ms: f64,
    p95_step_ms: f64,
    p99_step_ms: f64,
    max_step_ms: f64,
    avg_workload_ms: f64,
    avg_lifecycle_ms: f64,
    max_lifecycle_ms: f64,
    avg_gravity_ms: f64,
    avg_gravity_validation_ms: f64,
    avg_gravity_build_ms: f64,
    avg_gravity_aggregation_ms: f64,
    avg_gravity_traversal_ms: f64,
    avg_collision_ms: f64,
    avg_physics_ms: f64,
    avg_snapshot_ms: f64,
    avg_rapier_step_ms: f64,
    avg_rapier_update_ms: f64,
    avg_rapier_user_changes_ms: f64,
    avg_rapier_kinematic_interpolation_ms: f64,
    avg_rapier_collision_detection_ms: f64,
    avg_rapier_broad_phase_ms: f64,
    avg_rapier_final_broad_phase_ms: f64,
    avg_rapier_narrow_phase_ms: f64,
    avg_rapier_island_ms: f64,
    avg_rapier_island_constraints_ms: f64,
    avg_rapier_solver_ms: f64,
    avg_rapier_ccd_ms: f64,
    avg_render_ms: f64,
    avg_present_ms: f64,
    avg_raster_clear_ms: f64,
    avg_raster_player_ms: f64,
    avg_raster_player_starfield_ms: f64,
    avg_raster_player_bodies_ms: f64,
    avg_raster_player_world_ms: f64,
    avg_raster_player_sun_planets_ms: f64,
    avg_raster_player_spaceports_ms: f64,
    avg_raster_player_effects_ms: f64,
    avg_raster_player_ships_ms: f64,
    avg_raster_player_debris_ms: f64,
    avg_raster_player_particles_ms: f64,
    avg_raster_player_other_ms: f64,
    avg_raster_other_frames_ms: f64,
    avg_raster_overview_refresh_ms: f64,
    avg_raster_overview_blit_ms: f64,
    avg_raster_overview_live_ms: f64,
    avg_raster_image_ms: f64,
    avg_total_ms: f64,
    p50_total_ms: f64,
    p95_total_ms: f64,
    p99_total_ms: f64,
    max_total_ms: f64,
}

impl BenchmarkRow {
    fn from_sample(
        scenario: &str,
        second: u64,
        renderer: RenderBackend,
        raster_scale: f32,
        sample: BenchmarkSample,
        counts: BenchmarkCounts,
    ) -> Self {
        let frames = sample.frames.max(1);
        let measured_time = sample.step_time + sample.render_time + sample.present_time;
        Self {
            scenario: scenario.into(),
            renderer: renderer.label(),
            raster_scale,
            second,
            frames: sample.frames,
            updates: sample.updates,
            throughput_fps: sample.frames as f64 / sample.wall_time.as_secs_f64().max(0.000_001),
            scene_items: sample.scene_items,
            asteroids: counts.asteroids,
            fragments: counts.fragments,
            shells: counts.shells,
            particles: counts.particles,
            balls: counts.balls,
            gravity_sources: counts.gravity_sources,
            gravity_targets: counts.gravity_targets,
            gravity_nodes: counts.gravity_nodes,
            gravity_exact_interactions: counts.gravity_exact_interactions,
            gravity_approximations: counts.gravity_approximations,
            gravity_applied_sources: counts.gravity_applied_sources,
            active_bodies: counts.active_bodies,
            sleeping_bodies: counts.sleeping_bodies,
            candidate_pairs: counts.candidate_pairs,
            max_candidate_pairs: sample.max_candidate_pairs,
            contact_pairs: counts.contact_pairs,
            max_contact_pairs: sample.max_contact_pairs,
            contacts: counts.contacts,
            max_contacts: sample.max_contacts,
            added: sample.scenario_metrics.added,
            removed: sample.scenario_metrics.removed,
            avg_step_ms: avg_ms(sample.step_time, frames),
            p50_step_ms: percentile_ms(&sample.step_samples, 0.50),
            p95_step_ms: percentile_ms(&sample.step_samples, 0.95),
            p99_step_ms: percentile_ms(&sample.step_samples, 0.99),
            max_step_ms: sample
                .step_samples
                .iter()
                .copied()
                .max()
                .map(duration_ms)
                .unwrap_or_default(),
            avg_workload_ms: avg_ms(sample.scenario_metrics.workload_time, frames),
            avg_lifecycle_ms: avg_ms(sample.scenario_metrics.lifecycle_time, frames),
            max_lifecycle_ms: duration_ms(sample.max_lifecycle_time),
            avg_gravity_ms: avg_ms(sample.scenario_metrics.gravity_time, frames),
            avg_gravity_validation_ms: avg_ms(
                sample.scenario_metrics.gravity_validation_time,
                frames,
            ),
            avg_gravity_build_ms: avg_ms(sample.scenario_metrics.gravity_build_time, frames),
            avg_gravity_aggregation_ms: avg_ms(
                sample.scenario_metrics.gravity_aggregation_time,
                frames,
            ),
            avg_gravity_traversal_ms: avg_ms(
                sample.scenario_metrics.gravity_traversal_time,
                frames,
            ),
            avg_collision_ms: avg_ms(sample.scenario_metrics.collision_time, frames),
            avg_physics_ms: avg_ms(sample.scenario_metrics.physics_time, frames),
            avg_snapshot_ms: avg_ms(sample.scenario_metrics.snapshot_time, frames),
            avg_rapier_step_ms: avg_ms(sample.scenario_metrics.rapier_step_time, frames),
            avg_rapier_update_ms: avg_ms(sample.scenario_metrics.rapier_update_time, frames),
            avg_rapier_user_changes_ms: avg_ms(
                sample.scenario_metrics.rapier_user_changes_time,
                frames,
            ),
            avg_rapier_kinematic_interpolation_ms: avg_ms(
                sample.scenario_metrics.rapier_kinematic_interpolation_time,
                frames,
            ),
            avg_rapier_collision_detection_ms: avg_ms(
                sample.scenario_metrics.rapier_collision_detection_time,
                frames,
            ),
            avg_rapier_broad_phase_ms: avg_ms(
                sample.scenario_metrics.rapier_broad_phase_time,
                frames,
            ),
            avg_rapier_final_broad_phase_ms: avg_ms(
                sample.scenario_metrics.rapier_final_broad_phase_time,
                frames,
            ),
            avg_rapier_narrow_phase_ms: avg_ms(
                sample.scenario_metrics.rapier_narrow_phase_time,
                frames,
            ),
            avg_rapier_island_ms: avg_ms(sample.scenario_metrics.rapier_island_time, frames),
            avg_rapier_island_constraints_ms: avg_ms(
                sample.scenario_metrics.rapier_island_constraints_time,
                frames,
            ),
            avg_rapier_solver_ms: avg_ms(sample.scenario_metrics.rapier_solver_time, frames),
            avg_rapier_ccd_ms: avg_ms(sample.scenario_metrics.rapier_ccd_time, frames),
            avg_render_ms: avg_ms(sample.render_time, frames),
            avg_present_ms: avg_ms(sample.present_time, frames),
            avg_raster_clear_ms: avg_ms(sample.raster_timings.clear, frames),
            avg_raster_player_ms: avg_ms(sample.raster_timings.player_views, frames),
            avg_raster_player_starfield_ms: avg_ms(sample.raster_timings.player_starfield, frames),
            avg_raster_player_bodies_ms: avg_ms(sample.raster_timings.player_bodies, frames),
            avg_raster_player_world_ms: avg_ms(sample.raster_timings.player_world, frames),
            avg_raster_player_sun_planets_ms: avg_ms(
                sample.raster_timings.player_sun_planets,
                frames,
            ),
            avg_raster_player_spaceports_ms: avg_ms(
                sample.raster_timings.player_spaceports,
                frames,
            ),
            avg_raster_player_effects_ms: avg_ms(sample.raster_timings.player_effects, frames),
            avg_raster_player_ships_ms: avg_ms(sample.raster_timings.player_ships, frames),
            avg_raster_player_debris_ms: avg_ms(sample.raster_timings.player_debris, frames),
            avg_raster_player_particles_ms: avg_ms(sample.raster_timings.player_particles, frames),
            avg_raster_player_other_ms: avg_ms(sample.raster_timings.player_other, frames),
            avg_raster_other_frames_ms: avg_ms(sample.raster_timings.other_frames, frames),
            avg_raster_overview_refresh_ms: avg_ms(sample.raster_timings.overview_refresh, frames),
            avg_raster_overview_blit_ms: avg_ms(sample.raster_timings.overview_blit, frames),
            avg_raster_overview_live_ms: avg_ms(sample.raster_timings.overview_live, frames),
            avg_raster_image_ms: avg_ms(sample.raster_timings.image, frames),
            avg_total_ms: avg_ms(measured_time, frames),
            p50_total_ms: percentile_ms(&sample.total_samples, 0.50),
            p95_total_ms: percentile_ms(&sample.total_samples, 0.95),
            p99_total_ms: percentile_ms(&sample.total_samples, 0.99),
            max_total_ms: duration_ms(sample.max_total_time),
        }
    }
}

fn write_benchmark_header(mut writer: impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "scenario,renderer,raster_scale,second,frames,updates,throughput_fps,scene_items,asteroids,fragments,shells,particles,balls,gravity_sources,gravity_targets,gravity_nodes,gravity_exact_interactions,gravity_approximations,gravity_applied_sources,active_bodies,sleeping_bodies,candidate_pairs,max_candidate_pairs,contact_pairs,max_contact_pairs,solver_contacts,max_solver_contacts,added,removed,avg_step_ms,p50_step_ms,p95_step_ms,p99_step_ms,max_step_ms,avg_workload_ms,avg_lifecycle_ms,max_lifecycle_ms,avg_gravity_ms,avg_gravity_validation_ms,avg_gravity_build_ms,avg_gravity_aggregation_ms,avg_gravity_traversal_ms,avg_collision_ms,avg_physics_ms,avg_snapshot_ms,avg_rapier_step_ms,avg_rapier_update_ms,avg_rapier_user_changes_ms,avg_rapier_kinematic_interpolation_ms,avg_rapier_collision_detection_ms,avg_rapier_broad_phase_ms,avg_rapier_final_broad_phase_ms,avg_rapier_narrow_phase_ms,avg_rapier_island_ms,avg_rapier_island_constraints_ms,avg_rapier_solver_ms,avg_rapier_ccd_ms,avg_render_ms,avg_present_ms,avg_raster_clear_ms,avg_raster_player_ms,avg_raster_player_starfield_ms,avg_raster_player_bodies_ms,avg_raster_player_world_ms,avg_raster_player_sun_planets_ms,avg_raster_player_spaceports_ms,avg_raster_player_effects_ms,avg_raster_player_ships_ms,avg_raster_player_debris_ms,avg_raster_player_particles_ms,avg_raster_player_other_ms,avg_raster_other_frames_ms,avg_raster_overview_refresh_ms,avg_raster_overview_blit_ms,avg_raster_overview_live_ms,avg_raster_image_ms,avg_total_ms,p50_total_ms,p95_total_ms,p99_total_ms,max_total_ms"
    )
}

fn write_benchmark_row(mut writer: impl Write, row: &BenchmarkRow) -> io::Result<()> {
    let fields = [
        row.scenario.clone(),
        row.renderer.into(),
        format!("{:.2}", row.raster_scale),
        row.second.to_string(),
        row.frames.to_string(),
        row.updates.to_string(),
        format!("{:.2}", row.throughput_fps),
        row.scene_items.to_string(),
        row.asteroids.to_string(),
        row.fragments.to_string(),
        row.shells.to_string(),
        row.particles.to_string(),
        row.balls.to_string(),
        row.gravity_sources.to_string(),
        row.gravity_targets.to_string(),
        row.gravity_nodes.to_string(),
        row.gravity_exact_interactions.to_string(),
        row.gravity_approximations.to_string(),
        row.gravity_applied_sources.to_string(),
        row.active_bodies.to_string(),
        row.sleeping_bodies.to_string(),
        row.candidate_pairs.to_string(),
        row.max_candidate_pairs.to_string(),
        row.contact_pairs.to_string(),
        row.max_contact_pairs.to_string(),
        row.contacts.to_string(),
        row.max_contacts.to_string(),
        row.added.to_string(),
        row.removed.to_string(),
        format!("{:.3}", row.avg_step_ms),
        format!("{:.3}", row.p50_step_ms),
        format!("{:.3}", row.p95_step_ms),
        format!("{:.3}", row.p99_step_ms),
        format!("{:.3}", row.max_step_ms),
        format!("{:.3}", row.avg_workload_ms),
        format!("{:.3}", row.avg_lifecycle_ms),
        format!("{:.3}", row.max_lifecycle_ms),
        format!("{:.3}", row.avg_gravity_ms),
        format!("{:.3}", row.avg_gravity_validation_ms),
        format!("{:.3}", row.avg_gravity_build_ms),
        format!("{:.3}", row.avg_gravity_aggregation_ms),
        format!("{:.3}", row.avg_gravity_traversal_ms),
        format!("{:.3}", row.avg_collision_ms),
        format!("{:.3}", row.avg_physics_ms),
        format!("{:.3}", row.avg_snapshot_ms),
        format!("{:.3}", row.avg_rapier_step_ms),
        format!("{:.3}", row.avg_rapier_update_ms),
        format!("{:.3}", row.avg_rapier_user_changes_ms),
        format!("{:.3}", row.avg_rapier_kinematic_interpolation_ms),
        format!("{:.3}", row.avg_rapier_collision_detection_ms),
        format!("{:.3}", row.avg_rapier_broad_phase_ms),
        format!("{:.3}", row.avg_rapier_final_broad_phase_ms),
        format!("{:.3}", row.avg_rapier_narrow_phase_ms),
        format!("{:.3}", row.avg_rapier_island_ms),
        format!("{:.3}", row.avg_rapier_island_constraints_ms),
        format!("{:.3}", row.avg_rapier_solver_ms),
        format!("{:.3}", row.avg_rapier_ccd_ms),
        format!("{:.3}", row.avg_render_ms),
        format!("{:.3}", row.avg_present_ms),
        format!("{:.3}", row.avg_raster_clear_ms),
        format!("{:.3}", row.avg_raster_player_ms),
        format!("{:.3}", row.avg_raster_player_starfield_ms),
        format!("{:.3}", row.avg_raster_player_bodies_ms),
        format!("{:.3}", row.avg_raster_player_world_ms),
        format!("{:.3}", row.avg_raster_player_sun_planets_ms),
        format!("{:.3}", row.avg_raster_player_spaceports_ms),
        format!("{:.3}", row.avg_raster_player_effects_ms),
        format!("{:.3}", row.avg_raster_player_ships_ms),
        format!("{:.3}", row.avg_raster_player_debris_ms),
        format!("{:.3}", row.avg_raster_player_particles_ms),
        format!("{:.3}", row.avg_raster_player_other_ms),
        format!("{:.3}", row.avg_raster_other_frames_ms),
        format!("{:.3}", row.avg_raster_overview_refresh_ms),
        format!("{:.3}", row.avg_raster_overview_blit_ms),
        format!("{:.3}", row.avg_raster_overview_live_ms),
        format!("{:.3}", row.avg_raster_image_ms),
        format!("{:.3}", row.avg_total_ms),
        format!("{:.3}", row.p50_total_ms),
        format!("{:.3}", row.p95_total_ms),
        format!("{:.3}", row.p99_total_ms),
        format!("{:.3}", row.max_total_ms),
    ];
    writeln!(writer, "{}", fields.join(","))
}

fn avg_ms(duration: Duration, samples: u32) -> f64 {
    duration_ms(duration) / samples.max(1) as f64
}

fn percentile_ms(samples: &[Duration], percentile: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut samples = samples.to_vec();
    samples.sort_unstable();
    let percentile = percentile.clamp(0.0, 1.0);
    let index = ((samples.len() - 1) as f64 * percentile).ceil() as usize;
    duration_ms(samples[index])
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(crate) struct HostedScenario {
    inner: Box<dyn ClientScenario>,
    asset: ScenarioAsset,
}

impl HostedScenario {
    pub(crate) fn new(
        name: &str,
        seed: u64,
        settings: &Settings,
        viewport: Viewport,
        mode: ScenarioStartMode,
    ) -> Result<Self, HostError> {
        Self::new_with_asset(name, seed, settings, viewport, mode, &ScenarioAsset::None)
    }

    pub(crate) fn new_with_asset(
        name: &str,
        seed: u64,
        settings: &Settings,
        viewport: Viewport,
        mode: ScenarioStartMode,
        asset: &ScenarioAsset,
    ) -> Result<Self, HostError> {
        let registration = client_scenarios::registration(name)
            .ok_or_else(|| HostError::UnknownScenario { name: name.into() })?;
        let inner = registration
            .create_with_asset(seed, settings, viewport, mode, asset)
            .map_err(|error| match error {
                ScenarioCreateError::BenchmarkUnsupported { .. } => {
                    HostError::BenchmarkUnsupported { name: name.into() }
                }
                source => HostError::ScenarioCreation {
                    name: name.into(),
                    source,
                },
            })?;
        Ok(Self {
            inner,
            asset: asset.clone(),
        })
    }

    pub(crate) fn registration(&self) -> &'static ScenarioRegistration {
        self.inner.registration()
    }

    pub(crate) fn tick_model(&self) -> TickModel {
        self.inner.tick_model()
    }

    pub(crate) fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        self.inner.step(actions, dt)
    }

    pub(crate) fn zoom_player_in(&mut self, player: usize) {
        self.inner.zoom_player_in(player);
    }

    pub(crate) fn zoom_player_out(&mut self, player: usize) {
        self.inner.zoom_player_out(player);
    }

    fn is_game_over(&self) -> bool {
        self.inner.is_game_over()
    }

    pub(crate) fn actions(
        &self,
        input: &mut ClientInput,
        benchmark_active: bool,
        input_projections: &[render::FrameProjection],
    ) -> Vec<Action> {
        let mut actions = self.inner.map_input(input, benchmark_active);
        actions.extend(self.pointer_actions(input, input_projections));
        actions
    }

    fn pointer_actions(
        &self,
        input: &mut ClientInput,
        input_projections: &[render::FrameProjection],
    ) -> Vec<Action> {
        if self.registration().capabilities.pointer_input {
            input
                .take_pointer_events()
                .into_iter()
                .filter_map(|event| {
                    render::unproject_pointer(input_projections, event.position, event.phase)
                })
                .map(Action::Pointer)
                .collect()
        } else {
            input.discard_pointer_events();
            Vec::new()
        }
    }

    pub(crate) fn benchmark_counts(&self) -> BenchmarkCounts {
        self.inner.benchmark_counts().unwrap_or_default()
    }

    pub(crate) fn benchmark_step_metrics(&self) -> BenchmarkStepMetrics {
        self.inner.benchmark_step_metrics().unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn render_frame(&self) -> RenderFrame {
        self.render_frames(RenderBackend::Vector, BENCHMARK_VIEWPORT)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    pub(crate) fn render_frames(
        &self,
        renderer: RenderBackend,
        viewport: Viewport,
    ) -> Vec<RenderFrame> {
        self.inner.render_frames(renderer, viewport)
    }

    pub(crate) fn native_video_frame(&self) -> Option<NativeVideoFrame<'_>> {
        self.inner.native_video_frame()
    }

    fn realtime_video_consumer(&self) -> Option<RealtimeVideoConsumer> {
        self.inner.realtime_video_consumer()
    }

    fn has_realtime_runtime(&self) -> bool {
        self.realtime_video_consumer().is_some()
    }

    fn publish_realtime_actions(&self, actions: &[Action], observed_at: Instant) {
        self.inner.publish_realtime_actions(actions, observed_at);
    }

    fn set_realtime_paused(&self, paused: bool) {
        self.inner.set_realtime_paused(paused);
    }

    fn shutdown_realtime(&mut self) {
        self.inner.shutdown_realtime();
    }

    fn record_realtime_displayed_loop_iteration(&self) {
        self.inner.record_realtime_displayed_loop_iteration();
    }

    fn realtime_telemetry(&self) -> Option<RealtimeTelemetry> {
        self.inner.realtime_telemetry()
    }

    fn runtime_error(&self) -> Option<String> {
        self.inner.runtime_error()
    }

    pub(crate) fn frame_layout(&self) -> render::FrameLayout {
        self.inner.frame_layout()
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        self.inner.set_viewport(viewport);
    }

    fn center_panel_state(
        &self,
        paused: bool,
        benchmark_active: bool,
        performance_text: &str,
    ) -> Option<CenterPanelState> {
        self.inner
            .center_panel_state(paused, benchmark_active, performance_text)
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self.inner.as_any()
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self.inner.as_any_mut()
    }
}

fn set_center_panel(window: &MainWindow, state: Option<CenterPanelState>) {
    let Some(state) = state else {
        window.set_spacewars_ui_visible(false);
        return;
    };

    window.set_spacewars_ui_visible(true);
    window.set_p1_name(SharedString::from(state.player_1.name));
    window.set_p1_status(SharedString::from(state.player_1.status));
    window.set_p1_status_fraction(state.player_1.status_fraction);
    window.set_p1_color(brush_from_core_color(state.player_1.color));
    window.set_p2_name(SharedString::from(state.player_2.name));
    window.set_p2_status(SharedString::from(state.player_2.status));
    window.set_p2_status_fraction(state.player_2.status_fraction);
    window.set_p2_color(brush_from_core_color(state.player_2.color));
    window.set_planet_score_label(SharedString::from(state.planet_score_label));
    window.set_p1_planet_fraction(state.player_1_planet_fraction);
    window.set_p2_planet_fraction(state.player_2_planet_fraction);
    window.set_p1_planet_score_text(SharedString::from(state.player_1_planet_score));
    window.set_free_planet_score_text(SharedString::from(state.free_planet_score));
    window.set_p2_planet_score_text(SharedString::from(state.player_2_planet_score));
    window.set_spacewars_message_text(SharedString::from(state.message_text));
    window.set_spacewars_performance_text(SharedString::from(state.performance_text));
}

fn brush_from_core_color(color: CoreColor) -> Brush {
    Brush::SolidColor(SlintColor::from_argb_f32(
        color.a.clamp(0.0, 1.0),
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::RenderPoint;
    use engine_core::SpacewarsConfig;
    use scenario_spacewars::{ShipForm, SpacewarsState};

    use crate::client_scenarios::{PizzaClientScenario, SpacewarsClientScenario};
    use crate::input::ScreenPointerEvent;

    const TEST_VIEWPORT: Viewport = Viewport::new(1280.0, 720.0);

    fn hosted_scenario(name: &str, seed: u64) -> Result<HostedScenario, HostError> {
        HostedScenario::new(
            name,
            seed,
            &Settings::default(),
            TEST_VIEWPORT,
            ScenarioStartMode::Normal,
        )
    }

    fn wait_for(timeout: Duration, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + timeout;
        while !condition() {
            assert!(Instant::now() < deadline, "condition timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn settings_from_config(config: &SpacewarsConfig) -> Settings {
        let mut settings = Settings::default();
        settings.spacewars.universe_radius = config.universe_radius;
        settings.spacewars.use_planets = config.use_planets;
        settings.spacewars.asteroids_enabled = config.asteroid_probability_per_sec > 0.0;
        settings.spacewars.asteroid_probability_per_sec = config.asteroid_probability_per_sec;
        settings.spacewars.player_health_percent = config.players[0].health_percent;
        settings.spacewars.player_1_view_height = config.player_view_heights[0];
        settings.spacewars.player_2_view_height = config.player_view_heights[1];
        settings
    }

    fn hosted_spacewars_with_config(
        seed: u64,
        config: &SpacewarsConfig,
    ) -> Result<HostedScenario, HostError> {
        HostedScenario::new(
            "spacewars",
            seed,
            &settings_from_config(config),
            TEST_VIEWPORT,
            ScenarioStartMode::Normal,
        )
    }

    fn spacewars_state(scenario: &HostedScenario) -> &SpacewarsState {
        scenario
            .as_any()
            .downcast_ref::<SpacewarsClientScenario>()
            .expect("scenario should host Spacewars")
            .state
            .as_ref()
    }

    fn spacewars_state_mut(scenario: &mut HostedScenario) -> &mut SpacewarsState {
        scenario
            .as_any_mut()
            .downcast_mut::<SpacewarsClientScenario>()
            .expect("scenario should host Spacewars")
            .state
            .as_mut()
    }

    fn small_duel_config() -> SpacewarsConfig {
        SpacewarsConfig {
            universe_radius: 600,
            asteroid_probability_per_sec: 0.0,
            use_planets: false,
            players: [
                engine_core::PlayerConfig::new("Player 1", 250, CoreColor::RED),
                engine_core::PlayerConfig::new("Player 2", 250, CoreColor::GREEN),
            ],
            ..SpacewarsConfig::default()
        }
    }

    #[test]
    fn unknown_scenario_is_rejected() {
        let err = match hosted_scenario("bogus", 0) {
            Ok(_) => panic!("bogus scenario should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("unknown scenario"));
        assert!(err.to_string().contains("spacewars"));
    }

    #[test]
    fn failed_replacement_retains_the_current_usable_scenario() {
        let mut scenario = hosted_scenario("spacewars", 0).unwrap();
        spacewars_state_mut(&mut scenario).tick = 73;

        let error = replace_scenario(&mut scenario, || {
            Err(HostError::ScenarioCreation {
                name: "spacewars".into(),
                source: ScenarioCreateError::MissingAsset {
                    name: "spacewars",
                    asset: "missing-test-asset".into(),
                },
            })
        })
        .unwrap_err();

        assert!(error.to_string().contains("missing-test-asset"));
        assert_eq!(spacewars_state(&scenario).tick, 73);
    }

    #[test]
    fn performance_stats_reports_target_and_measured_rates() {
        let start = Instant::now();
        let mut stats = PerformanceStats::new(TickModel::FixedTimestep { hz: 60 }, start);

        assert_eq!(stats.display_text(), "Target 60 Hz | FPS -- | UPS --");
        assert_eq!(
            stats.diagnostics_text(),
            "performance_target=60 Hz\nfps=--\nups=--\nframes_total=0\nupdates_total=0"
        );

        for frame in 1..=60 {
            let sample_completed =
                stats.record_frame(start + Duration::from_secs_f64(frame as f64 / 60.0), 1);
            assert_eq!(sample_completed, frame == 60);
        }

        assert_eq!(stats.display_text(), "Target 60 Hz | FPS 60 | UPS 60");
        assert_eq!(
            stats.diagnostics_text(),
            "performance_target=60 Hz\nfps=60.0\nups=60.0\nframes_total=60\nupdates_total=60"
        );
    }

    #[test]
    fn runtime_diagnostics_include_launch_state_and_performance() {
        let start = Instant::now();
        let mut stats = PerformanceStats::new(TickModel::Variable, start);
        stats.record_frame(start + Duration::from_secs(1), 3);

        assert_eq!(
            runtime_diagnostics_text(
                "pizza",
                17,
                false,
                true,
                RenderBackend::Raster,
                2.0,
                &stats,
                "No active rule-bot diagnostics.",
            ),
            "scenario=pizza\nscenario_revision=17\npaused=false\nbenchmark_active=true\nrenderer=raster\nraster_scale=2.00\nperformance_target=variable\nfps=1.0\nups=3.0\nframes_total=1\nupdates_total=3\nNo active rule-bot diagnostics."
        );
    }

    #[test]
    fn null_scenario_renders_empty_frame() {
        let scenario = hosted_scenario("null", 0).unwrap();

        assert!(scenario.render_frame().layers.is_empty());
        assert_eq!(scenario.center_panel_state(false, false, ""), None);
        assert_eq!(scenario.registration().id, "null");
    }

    #[test]
    fn falling_hosts_a_complete_native_frame_without_vector_fallback() {
        let scenario = hosted_scenario("falling", 0).unwrap();
        let consumer = scenario.realtime_video_consumer().unwrap();
        let descriptor = consumer.descriptor();

        assert!(scenario.registration().capabilities.native_video);
        assert!(scenario.registration().capabilities.captures_gamepad_start);
        assert!(matches!(scenario.tick_model(), TickModel::EmulatorClock));
        assert!(scenario.native_video_frame().is_none());
        assert_eq!((descriptor.width, descriptor.height), (256, 240));
        assert_eq!(
            (
                descriptor.visible_crop.width,
                descriptor.visible_crop.height
            ),
            (256, 224)
        );
        let mut pixels = vec![0; descriptor.pixel_count()];
        let initial = consumer.try_copy_latest(&mut pixels).unwrap().unwrap();
        assert!(initial.frame_id > 0);
        assert!(
            scenario
                .render_frames(RenderBackend::Vector, TEST_VIEWPORT)
                .is_empty()
        );
    }

    #[test]
    fn user_cartridge_uses_the_native_nes_runtime_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("falling-copy.nes");
        std::fs::write(&path, scenario_falling::FALLING_ROM).unwrap();
        let asset = ScenarioAsset::NesRom(crate::nes_roms::load_path(&path).unwrap());
        let settings = Settings::default();
        let mut scenario = HostedScenario::new_with_asset(
            "nes",
            0,
            &settings,
            TEST_VIEWPORT,
            ScenarioStartMode::Normal,
            &asset,
        )
        .unwrap();

        assert!(scenario.registration().capabilities.native_video);
        assert!(scenario.registration().capabilities.captures_gamepad_select);
        scenario.publish_realtime_actions(
            &[scenario_nes::NesAction::set_controllers(
                [engine_nes::ControllerButtons::NONE; 2],
            )],
            Instant::now(),
        );
        scenario.set_realtime_paused(false);
        wait_for(Duration::from_secs(1), || {
            scenario.realtime_telemetry().unwrap().emulated_frames >= 1
        });

        let mut accumulator = Duration::ZERO;
        let mut input = ClientInput::default();
        let mut paused = true;
        let mut benchmark_active = false;
        restart_scenario(
            &mut scenario,
            "nes",
            0,
            &mut accumulator,
            &mut input,
            &mut paused,
            &mut benchmark_active,
            &settings,
            TEST_VIEWPORT,
        )
        .unwrap();
        assert!(!paused);
        scenario.publish_realtime_actions(
            &[scenario_nes::NesAction::set_controllers(
                [engine_nes::ControllerButtons::NONE; 2],
            )],
            Instant::now(),
        );
        scenario.set_realtime_paused(false);
        wait_for(Duration::from_secs(1), || {
            scenario.realtime_telemetry().unwrap().emulated_frames >= 1
        });
        scenario.shutdown_realtime();
    }

    #[test]
    fn realtime_worker_uses_hardware_pacing_instead_of_ui_elapsed() {
        let mut scenario = hosted_scenario("falling", 0).unwrap();
        let mut input = ClientInput::default();
        input.press(input::GameKey::NesStart);
        let mut accumulator = Duration::ZERO;
        let mut controls = ScenarioControls::default();
        let mut paused = false;
        let mut benchmark_active = false;
        assert_eq!(fixed_step_duration(TickModel::EmulatorClock), None);

        // The first host iteration resumes from a neutral boundary. Even an
        // absurd UI elapsed value must not turn into a synchronous catch-up.
        let result = step_scenario(
            &mut scenario,
            "falling",
            0,
            TickModel::EmulatorClock,
            None,
            Duration::from_secs(10),
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert_eq!(result.updates, 0);
        assert_eq!(accumulator, Duration::ZERO);

        // Re-publish after resume neutralization; the newest complete mask is
        // sampled once by the worker rather than replayed as an event queue.
        step_scenario(
            &mut scenario,
            "falling",
            0,
            TickModel::EmulatorClock,
            None,
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        wait_for(Duration::from_secs(1), || {
            scenario
                .realtime_telemetry()
                .and_then(|telemetry| telemetry.latest_input)
                .is_some_and(|input| {
                    input.controllers[0].contains(engine_nes::ControllerButtons::START)
                })
        });
        assert!(scenario.realtime_telemetry().unwrap().emulated_frames < 10);
        scenario.set_realtime_paused(true);
    }

    #[test]
    fn falling_worker_handles_pause_restart_launcher_and_relaunch() {
        let mut scenario = hosted_scenario("falling", 7).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::ZERO;
        let mut controls = ScenarioControls::default();
        let mut paused = false;
        let mut benchmark_active = false;
        assert_eq!(fixed_step_duration(TickModel::EmulatorClock), None);

        step_scenario(
            &mut scenario,
            "falling",
            7,
            TickModel::EmulatorClock,
            None,
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        wait_for(Duration::from_secs(1), || {
            scenario.realtime_telemetry().unwrap().emulated_frames >= 2
        });
        input.press(input::GameKey::Pause);
        step_scenario(
            &mut scenario,
            "falling",
            7,
            TickModel::EmulatorClock,
            None,
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(paused);

        controls.request_restart();
        let restart = step_scenario(
            &mut scenario,
            "falling",
            7,
            TickModel::EmulatorClock,
            None,
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert_eq!(restart.scenario_error_text.as_deref(), Some(""));
        assert!(restart.scenario_replaced);
        assert!(!paused);
        wait_for(Duration::from_secs(1), || {
            scenario.realtime_telemetry().unwrap().emulated_frames >= 1
        });

        input.press(input::GameKey::Pause);
        step_scenario(
            &mut scenario,
            "falling",
            7,
            TickModel::EmulatorClock,
            None,
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        input.press(input::GameKey::ReturnLauncher);
        let result = step_scenario(
            &mut scenario,
            "falling",
            7,
            TickModel::EmulatorClock,
            None,
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(result.return_to_launcher);

        let relaunched = hosted_scenario("falling", 7).unwrap();
        assert_eq!(relaunched.realtime_telemetry().unwrap().emulated_frames, 0);
    }

    #[test]
    fn pizza_scenario_receives_unprojected_pointer_actions() {
        let mut settings = Settings::default();
        settings.pizza.desired_balls = 0;
        let mut scenario = HostedScenario::new(
            "pizza",
            7,
            &settings,
            TEST_VIEWPORT,
            ScenarioStartMode::Normal,
        )
        .unwrap();
        let frames = scenario.render_frames(RenderBackend::Vector, TEST_VIEWPORT);
        let projections =
            render::frame_projections(&frames, TEST_VIEWPORT, scenario.frame_layout());
        let mut input = ClientInput::default();
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(TEST_VIEWPORT.width * 0.5, TEST_VIEWPORT.height * 0.5),
            phase: engine_common::PointerPhase::Press,
        });

        let actions = scenario.actions(&mut input, false, &projections);
        assert_eq!(
            actions,
            vec![Action::Pointer(engine_common::PointerAction {
                position: RenderPoint::new(0.5, 0.28125),
                phase: engine_common::PointerPhase::Press,
            })]
        );
        scenario.step(&actions, Duration::from_secs_f64(1.0 / 60.0));

        let pizza = scenario
            .as_any()
            .downcast_ref::<PizzaClientScenario>()
            .expect("scenario should host Pizza");
        assert_eq!(pizza.state.balls.len(), 1);
        assert!(pizza.state.held_ball_id.is_some());
    }

    #[test]
    fn pausing_cancels_pizza_pointer_interaction() {
        let mut settings = Settings::default();
        settings.pizza.desired_balls = 0;
        let mut scenario = HostedScenario::new(
            "pizza",
            7,
            &settings,
            TEST_VIEWPORT,
            ScenarioStartMode::Normal,
        )
        .unwrap();
        let frames = scenario.render_frames(RenderBackend::Vector, TEST_VIEWPORT);
        let projections =
            render::frame_projections(&frames, TEST_VIEWPORT, scenario.frame_layout());
        let mut input = ClientInput::default();
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(TEST_VIEWPORT.width * 0.5, TEST_VIEWPORT.height * 0.5),
            phase: engine_common::PointerPhase::Press,
        });
        let actions = scenario.actions(&mut input, false, &projections);
        scenario.step(&actions, Duration::from_secs_f64(1.0 / 60.0));

        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(TEST_VIEWPORT.width * 0.6, TEST_VIEWPORT.height * 0.5),
            phase: engine_common::PointerPhase::Drag,
        });
        let actions = scenario.actions(&mut input, false, &projections);
        scenario.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
        input.press(input::GameKey::Pause);

        let mut accumulator = Duration::ZERO;
        let mut controls = ScenarioControls::default();
        let mut paused = false;
        let mut benchmark_active = false;
        step_scenario(
            &mut scenario,
            "pizza",
            7,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &settings,
            TEST_VIEWPORT,
            &projections,
        );

        let pizza = scenario
            .as_any()
            .downcast_ref::<PizzaClientScenario>()
            .expect("scenario should host Pizza");
        assert!(paused);
        assert!(pizza.state.held_ball_id.is_none());
        assert!(!pizza.state.balls[0].invincible);
        assert!(pizza.state.balls[0].moving);
        assert_eq!(pizza.state.balls[0].velocity, engine_core::Vec2::ZERO);
    }

    #[test]
    fn spacewars_scenario_renders_initial_world() {
        let scenario = hosted_scenario("spacewars", 0).unwrap();
        let frame = scenario.render_frame();
        let state = spacewars_state(&scenario);

        assert_eq!(state.config, SpacewarsConfig::default());
        assert!(state.sun.is_some());
        assert!(!state.planets.is_empty());
        assert!(!frame.layers.is_empty());
        assert!(matches!(
            scenario.tick_model(),
            TickModel::FixedTimestep { hz: 60 }
        ));
    }

    #[test]
    fn spacewars_scenario_uses_supplied_config() {
        let config = small_duel_config();
        let scenario = hosted_spacewars_with_config(0, &config).unwrap();
        let state = spacewars_state(&scenario);

        assert_eq!(state.config, config);
        assert!(state.sun.is_none());
        assert!(state.planets.is_empty());
    }

    #[test]
    fn spacewars_scenario_renders_original_style_local_play_frames_for_client() {
        let scenario = hosted_scenario("spacewars", 0).unwrap();
        let viewport = Viewport::new(1000.0, 700.0);
        let frames = scenario.render_frames(RenderBackend::Vector, viewport);

        let state = spacewars_state(&scenario);

        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].camera.center.x, state.ships[0].position.x);
        assert_eq!(frames[0].camera.center.y, state.ships[0].position.y);
        assert_eq!(frames[1].camera.center.x, state.ships[1].position.x);
        assert_eq!(frames[1].camera.center.y, state.ships[1].position.y);
        assert_eq!(frames[0].camera.height, frames[1].camera.height);
        assert_eq!(frames[2].camera.center.x, 1200.0);
        assert_eq!(frames[2].camera.center.y, 1200.0);
        assert_eq!(frames[3].camera, frames[2].camera);
        let view_rectangle = frames[2]
            .layers
            .iter()
            .find(|layer| layer.z == 8)
            .and_then(|layer| layer.primitives.first())
            .expect("overview should contain the player view rectangle");
        let engine_common::RenderPrimitive::Polygon(view_rectangle) = view_rectangle else {
            panic!("player view rectangle should be a polygon");
        };
        let visible_width = view_rectangle.points[1].x - view_rectangle.points[0].x;
        let visible_height = view_rectangle.points[2].y - view_rectangle.points[1].y;
        let expected_aspect_ratio = (viewport.width * 0.5) / viewport.height;
        assert!((visible_width / visible_height - expected_aspect_ratio).abs() <= 1.0e-5);
        assert_eq!(
            scenario.frame_layout(),
            render::FrameLayout::SpacewarsLocalPlay
        );
    }

    #[test]
    fn spacewars_panel_state_reports_health_pod_and_planet_score() {
        let mut scenario = hosted_scenario("spacewars", 0).unwrap();
        let (total_planets, free_planets) = {
            let state = spacewars_state_mut(&mut scenario);
            let total_planets = state.planets.len().max(1) as f32;
            state.ships[0].life = state.ships[0].life_max * 0.5;
            state.ships[1].form = ShipForm::EscapePod;
            state.ships[1].life = state.ships[1].life_max * 0.25;
            state.players[0].planet_count = 1;
            state.players[1].planet_count = 2;
            (total_planets, state.planets.len().saturating_sub(3))
        };

        let panel = scenario
            .center_panel_state(false, false, "Target 60 Hz | FPS 60 | UPS 60")
            .unwrap();

        assert_eq!(panel.player_1.name, "Player 1: Player 1");
        assert_eq!(panel.player_1.status, "Ship Health: 50%");
        assert_eq!(panel.player_1.status_fraction, 0.5);
        assert_eq!(panel.player_2.name, "Player 2: Player 2");
        assert_eq!(panel.player_2.status, "Pod Rebuild: 25%");
        assert_eq!(panel.player_2.status_fraction, 0.25);
        assert_eq!(panel.message_text, "P/Esc Pause | R Restart | B Bench");
        assert_eq!(panel.performance_text, "Target 60 Hz | FPS 60 | UPS 60");
        assert_eq!(
            panel.planet_score_label,
            format!("Planets  P1 1 | Free {free_planets} | P2 2")
        );
        assert_eq!(panel.player_1_planet_fraction, 1.0 / total_planets);
        assert_eq!(panel.player_2_planet_fraction, 2.0 / total_planets);
        assert_eq!(panel.player_1_planet_score, "1");
        assert_eq!(panel.free_planet_score, free_planets.to_string());
        assert_eq!(panel.player_2_planet_score, "2");
    }

    #[test]
    fn spacewars_panel_state_reports_winner_and_eliminated_player() {
        let mut scenario = hosted_scenario("spacewars", 0).unwrap();
        let state = spacewars_state_mut(&mut scenario);
        state.players[0].eliminated = true;
        state.winner = Some(1);

        let panel = scenario
            .center_panel_state(false, false, "Target 60 Hz | FPS 60 | UPS 0")
            .unwrap();

        assert_eq!(panel.player_1.status, "Eliminated");
        assert_eq!(panel.player_1.status_fraction, 0.0);
        assert_eq!(
            panel.message_text,
            "P2 Wins | R Restart | B Bench | Esc Launch"
        );
        assert_eq!(panel.performance_text, "Target 60 Hz | FPS 60 | UPS 0");
    }

    #[test]
    fn spacewars_panel_state_reports_pause_message() {
        let scenario = hosted_scenario("spacewars", 0).unwrap();

        let panel = scenario
            .center_panel_state(true, false, "Target 60 Hz | FPS 60 | UPS 0")
            .unwrap();

        assert_eq!(
            panel.message_text,
            "Paused | P/Esc Resume | R Restart | B Bench | Q Launch"
        );
        assert_eq!(panel.performance_text, "Target 60 Hz | FPS 60 | UPS 0");
    }

    #[test]
    fn spacewars_panel_state_reports_benchmark_message() {
        let scenario = HostedScenario::new(
            "spacewars",
            0,
            &Settings::default(),
            TEST_VIEWPORT,
            ScenarioStartMode::Benchmark(BenchmarkConfiguration::default()),
        )
        .unwrap();

        let panel = scenario
            .center_panel_state(false, true, "Target 60 Hz | FPS 42 | UPS 60")
            .unwrap();

        assert_eq!(panel.message_text, "Bench | P/Esc Pause | B Reset | R Game");
        assert_eq!(panel.performance_text, "Target 60 Hz | FPS 42 | UPS 60");
    }

    #[test]
    fn reset_key_restarts_spacewars_with_same_seed() {
        let config = small_duel_config();
        let settings = settings_from_config(&config);
        let mut scenario = hosted_spacewars_with_config(42, &config).unwrap();
        let expected = hosted_spacewars_with_config(42, &config).unwrap();
        let state = spacewars_state_mut(&mut scenario);
        state.tick = 120;
        state.ships[0].position.x += 50.0;

        let mut input = ClientInput::default();
        input.press(input::GameKey::Reset);
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = true;

        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &settings,
            TEST_VIEWPORT,
            &[],
        );

        let state = spacewars_state(&scenario);
        let expected = spacewars_state(&expected);
        assert_eq!(accumulator, Duration::ZERO);
        assert!(!paused);
        assert!(!benchmark_active);
        assert_eq!(state.tick, 0);
        assert_eq!(state.config, config);
        assert_eq!(state.winner, None);
        assert!(!state.players[0].eliminated);
        assert_eq!(state.ships[0].position, expected.ships[0].position);
    }

    #[test]
    fn pause_key_toggles_and_freezes_spacewars_steps() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::ZERO;
        let mut controls = ScenarioControls::default();
        let mut paused = false;
        let mut benchmark_active = false;

        input.press(input::GameKey::Pause);
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::from_secs(1),
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(paused);

        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::from_secs(1),
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        let state = spacewars_state(&scenario);
        assert_eq!(state.tick, 0);
        assert_eq!(accumulator, Duration::ZERO);

        input.press(input::GameKey::Pause);
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(!paused);
    }

    #[test]
    fn controller_disconnect_pause_is_idempotent() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = false;

        input.press(input::GameKey::ForcePause);
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );

        assert!(paused);
        assert_eq!(accumulator, Duration::ZERO);
    }

    #[test]
    fn escape_toggles_pause_and_backs_out_of_controls() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = false;
        let mut benchmark_active = false;

        input.press(input::GameKey::Back);
        let result = step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::from_secs(1),
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(paused);
        assert_eq!(result.ingame_controls_visible, None);
        assert_eq!(accumulator, Duration::ZERO);

        input.press(input::GameKey::Back);
        let result = step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            true,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(paused);
        assert_eq!(result.ingame_controls_visible, Some(false));

        input.press(input::GameKey::Back);
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(!paused);
    }

    #[test]
    fn menu_controls_key_toggles_controls_screen_while_paused() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = false;

        input.press(input::GameKey::Controls);
        let result = step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert_eq!(result.ingame_controls_visible, Some(true));
        assert_eq!(accumulator, Duration::ZERO);

        input.press(input::GameKey::Controls);
        let result = step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            true,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert_eq!(result.ingame_controls_visible, Some(false));
    }

    #[test]
    fn controls_key_opens_the_controls_screen_and_pauses_gameplay() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = false;
        let mut benchmark_active = false;

        input.press(input::GameKey::Controls);
        let result = step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );

        assert!(paused);
        assert_eq!(result.ingame_controls_visible, Some(true));
        assert_eq!(accumulator, Duration::ZERO);
    }

    #[test]
    fn q_returns_to_launcher_from_pause_menu() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = false;

        input.press(input::GameKey::ReturnLauncher);
        let result = step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(result.return_to_launcher);
        assert_eq!(accumulator, Duration::ZERO);
        assert!(!paused);
    }

    #[test]
    fn benchmark_key_starts_dense_spacewars_workload() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = false;

        input.press(input::GameKey::Benchmark);
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );

        let counts = scenario.benchmark_counts();
        assert!(benchmark_active);
        assert!(!paused);
        assert_eq!(accumulator, Duration::ZERO);
        assert_eq!(counts.asteroids, 100);
        assert_eq!(counts.particles, 1_200);
    }

    #[test]
    fn benchmark_key_starts_visible_rapier_pizza_workload() {
        let mut scenario = hosted_scenario("pizza", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = false;

        input.press(input::GameKey::Benchmark);
        step_scenario(
            &mut scenario,
            "pizza",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );

        let counts = scenario.benchmark_counts();
        assert!(benchmark_active);
        assert!(!paused);
        assert_eq!(accumulator, Duration::ZERO);
        assert_eq!(counts.balls, scenario_pizza::DEFAULT_BENCHMARK_BALLS);
        assert_eq!(
            scenario
                .render_frame()
                .layers
                .iter()
                .map(|layer| layer.primitives.len())
                .sum::<usize>(),
            scenario_pizza::DEFAULT_BENCHMARK_BALLS + 1
        );
    }

    #[test]
    fn escape_after_game_over_returns_to_launcher_without_stepping() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let state = spacewars_state_mut(&mut scenario);
        state.winner = Some(1);
        let tick_before = state.tick;

        let mut input = ClientInput::default();
        input.press(input::GameKey::Back);
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = false;

        let result = step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::from_secs(1),
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );

        let state = spacewars_state(&scenario);
        assert!(result.return_to_launcher);
        assert_eq!(result.updates, 0);
        assert_eq!(accumulator, Duration::ZERO);
        assert!(!paused);
        assert_eq!(state.tick, tick_before);
    }

    #[test]
    fn scenario_controls_pause_resume_restart_and_start_benchmark() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = false;
        let mut benchmark_active = false;

        controls.request_pause();
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(paused);
        assert_eq!(accumulator, Duration::ZERO);

        controls.request_resume();
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(!paused);
        assert_eq!(accumulator, Duration::ZERO);

        let state = spacewars_state_mut(&mut scenario);
        state.tick = 120;
        state.ships[0].position.x += 50.0;
        paused = true;
        benchmark_active = true;
        accumulator = Duration::from_secs(1);

        controls.request_restart();
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        let state = spacewars_state(&scenario);
        assert_eq!(state.tick, 0);
        assert!(!paused);
        assert!(!benchmark_active);

        controls.request_benchmark();
        step_scenario(
            &mut scenario,
            "spacewars",
            42,
            TickModel::FixedTimestep { hz: 60 },
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            Duration::ZERO,
            &mut accumulator,
            &mut input,
            &mut controls,
            &mut paused,
            &mut benchmark_active,
            false,
            &Settings::default(),
            TEST_VIEWPORT,
            &[],
        );
        assert!(benchmark_active);
        assert_eq!(scenario.benchmark_counts().asteroids, 100);
    }
}
