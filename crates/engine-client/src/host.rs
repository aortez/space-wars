//! Scenario hosting loop for the Slint client.

use std::cell::RefCell;
use std::fmt;
use std::fs::File;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use engine_common::{Action, RenderFrame, Scenario, StepResult, TickModel};
use engine_core::{Color as CoreColor, SpacewarsConfig};
use scenario_null::{NullConfig, NullScenario};
use scenario_spacewars::{ShipForm, SpacewarsBenchmarkCounts, SpacewarsScenario, SpacewarsState};
use slint::{
    Brush, Color as SlintColor, ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel,
};

use crate::input::{self, ClientInput};
use crate::raster;
use crate::render::{self, Viewport};
use crate::{MainWindow, ScenePrimitive};

const TIMER_INTERVAL: Duration = Duration::from_millis(16);
const MAX_FIXED_STEPS_PER_TICK: usize = 5;
const BENCHMARK_VIEWPORT: Viewport = Viewport::new(1280.0, 720.0);

#[derive(Debug, Clone)]
pub struct ScenarioLoopOptions {
    pub start_benchmark: bool,
    pub renderer: RenderBackend,
    pub raster_scale: f32,
    pub controls: Option<SharedScenarioControls>,
    pub spacewars_config: SpacewarsConfig,
}

impl Default for ScenarioLoopOptions {
    fn default() -> Self {
        Self {
            start_benchmark: false,
            renderer: RenderBackend::default(),
            raster_scale: 1.0,
            controls: None,
            spacewars_config: SpacewarsConfig::default(),
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
    pub seed: u64,
    pub seconds: u64,
    pub report_path: Option<PathBuf>,
    pub renderer: RenderBackend,
    pub raster_scale: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RenderBackend {
    #[default]
    Vector,
    Raster,
}

impl RenderBackend {
    fn label(self) -> &'static str {
        match self {
            Self::Vector => "vector",
            Self::Raster => "raster",
        }
    }
}

pub enum HostError {
    UnknownScenario { name: String },
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
        }
    }
}

impl std::error::Error for HostError {}

pub fn validate_scenario(name: &str) -> Result<(), HostError> {
    if is_known_scenario(name) {
        Ok(())
    } else {
        Err(HostError::UnknownScenario { name: name.into() })
    }
}

pub fn scenario_names() -> &'static [&'static str] {
    &["null", "spacewars"]
}

pub fn is_known_scenario(name: &str) -> bool {
    scenario_names().contains(&name)
}

pub fn start_debug_render_loop(
    window: &MainWindow,
    stress_triangles: usize,
    renderer: RenderBackend,
) -> Timer {
    set_spacewars_panel(window, None);

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

pub fn start_scenario_loop(
    window: &MainWindow,
    scenario: &str,
    seed: u64,
    options: ScenarioLoopOptions,
) -> Result<Timer, HostError> {
    let ScenarioLoopOptions {
        start_benchmark,
        renderer,
        raster_scale,
        controls,
        spacewars_config,
    } = options;
    let scenario_name = scenario.to_string();
    let mut scenario = if start_benchmark {
        HostedScenario::new_spacewars_benchmark(seed)
    } else {
        HostedScenario::new(&scenario_name, seed, spacewars_config.clone())?
    };
    let tick_model = scenario.tick_model();
    let fixed_dt = fixed_step_duration(tick_model);
    let input = std::rc::Rc::new(std::cell::RefCell::new(ClientInput::default()));
    input::install_window_input(window, std::rc::Rc::clone(&input));
    let controls = controls.unwrap_or_else(new_scenario_controls);

    let timer = Timer::default();
    let weak_window = window.as_weak();
    let mut last_tick = Instant::now();
    let mut accumulator = Duration::ZERO;
    let mut paused = false;
    let mut benchmark_active = start_benchmark;
    let mut performance = PerformanceStats::new(tick_model, last_tick);
    let mut raster_renderer = raster::RasterRenderer::new();

    if benchmark_active {
        tracing::info!(seed, "started visual Spacewars benchmark.");
    }

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
            &spacewars_config,
        );
        if step_result.return_to_launcher {
            show_running_launcher(&window);
            return;
        }
        if let Some(visible) = step_result.ingame_controls_visible {
            window.set_ingame_controls_visible(visible);
        }
        if window.get_launcher_visible() {
            return;
        }

        performance.record_frame(now, step_result.updates);
        let performance_text = performance.display_text();
        set_spacewars_panel(
            &window,
            scenario.spacewars_panel_state(paused, benchmark_active, &performance_text),
        );
        set_ingame_menu(&window, paused);
        present_frames(
            &window,
            scenario.render_frames(renderer),
            scenario.frame_layout(),
            renderer,
            raster_scale,
            &mut raster_renderer,
        );
    });

    Ok(timer)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HostStepResult {
    updates: usize,
    return_to_launcher: bool,
    ingame_controls_visible: Option<bool>,
}

impl HostStepResult {
    fn updates(updates: usize) -> Self {
        Self {
            updates,
            return_to_launcher: false,
            ingame_controls_visible: None,
        }
    }

    fn return_to_launcher() -> Self {
        Self {
            updates: 0,
            return_to_launcher: true,
            ingame_controls_visible: None,
        }
    }

    fn set_ingame_controls_visible(visible: bool) -> Self {
        Self {
            updates: 0,
            return_to_launcher: false,
            ingame_controls_visible: Some(visible),
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
    spacewars_config: &SpacewarsConfig,
) -> HostStepResult {
    if let Some(request) = controls.take_request() {
        match request {
            ScenarioControlRequest::Resume => {
                *paused = false;
                *accumulator = Duration::ZERO;
                input.clear();
                tracing::info!(benchmark = *benchmark_active, "resumed from in-game menu.");
                return HostStepResult::default();
            }
            ScenarioControlRequest::Restart => {
                restart_scenario(
                    scenario,
                    scenario_name,
                    seed,
                    accumulator,
                    input,
                    paused,
                    benchmark_active,
                    spacewars_config,
                );
                return HostStepResult::default();
            }
            ScenarioControlRequest::Benchmark => {
                start_benchmark_scenario(
                    scenario,
                    seed,
                    accumulator,
                    input,
                    paused,
                    benchmark_active,
                );
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
        start_benchmark_scenario(scenario, seed, accumulator, input, paused, benchmark_active);
        return HostStepResult::default();
    }

    if input.take_reset_requested() {
        restart_scenario(
            scenario,
            scenario_name,
            seed,
            accumulator,
            input,
            paused,
            benchmark_active,
            spacewars_config,
        );
        return HostStepResult::default();
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
        input.clear();
        tracing::info!(
            paused = *paused,
            benchmark = *benchmark_active,
            "toggled pause."
        );
        return HostStepResult::default();
    }

    if input.take_controls_requested() && *paused && !scenario.is_game_over() {
        *accumulator = Duration::ZERO;
        input.clear();
        return HostStepResult::set_ingame_controls_visible(!ingame_controls_visible);
    }

    if input.take_return_launcher_requested() && (*paused || scenario.is_game_over()) {
        *paused = false;
        *accumulator = Duration::ZERO;
        input.clear();
        return HostStepResult::return_to_launcher();
    }

    if input.take_pause_requested() && !scenario.is_game_over() {
        *paused = !*paused;
        *accumulator = Duration::ZERO;
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

    match (tick_model, fixed_dt) {
        (TickModel::FixedTimestep { .. }, Some(dt)) => {
            *accumulator += elapsed;
            let mut steps = 0;
            while *accumulator >= dt && steps < MAX_FIXED_STEPS_PER_TICK {
                let actions = scenario.actions(input, *benchmark_active);
                scenario.step(&actions, dt);
                *accumulator -= dt;
                steps += 1;
            }
            if steps == MAX_FIXED_STEPS_PER_TICK {
                *accumulator = Duration::ZERO;
            }
            HostStepResult::updates(steps)
        }
        (TickModel::Variable | TickModel::EmulatorClock, _) => {
            let actions = scenario.actions(input, *benchmark_active);
            scenario.step(&actions, elapsed);
            HostStepResult::updates(1)
        }
        (TickModel::FixedTimestep { .. }, None) => HostStepResult::default(),
    }
}

fn show_running_launcher(window: &MainWindow) {
    window.set_primitives(ModelRc::new(VecModel::from(Vec::<ScenePrimitive>::new())));
    window.set_raster_visible(false);
    window.set_spacewars_ui_visible(false);
    window.set_ingame_menu_visible(false);
    window.set_ingame_controls_visible(false);
    window.set_launcher_error_text(SharedString::from(""));
    window.set_launcher_controls_visible(false);
    window.set_launcher_visible(true);
}

fn restart_scenario(
    scenario: &mut HostedScenario,
    scenario_name: &str,
    seed: u64,
    accumulator: &mut Duration,
    input: &mut ClientInput,
    paused: &mut bool,
    benchmark_active: &mut bool,
    spacewars_config: &SpacewarsConfig,
) {
    match HostedScenario::new(scenario_name, seed, spacewars_config.clone()) {
        Ok(reset) => {
            *scenario = reset;
            *accumulator = Duration::ZERO;
            *paused = false;
            *benchmark_active = false;
            input.clear();
            tracing::info!(scenario = scenario_name, seed, "started new game.");
        }
        Err(err) => {
            tracing::error!(error = %err, scenario = scenario_name, "failed to start new game.");
        }
    }
}

fn start_benchmark_scenario(
    scenario: &mut HostedScenario,
    seed: u64,
    accumulator: &mut Duration,
    input: &mut ClientInput,
    paused: &mut bool,
    benchmark_active: &mut bool,
) {
    *scenario = HostedScenario::new_spacewars_benchmark(seed);
    *accumulator = Duration::ZERO;
    *paused = false;
    *benchmark_active = true;
    input.clear();
    tracing::info!(seed, "started visual Spacewars benchmark.");
}

fn set_ingame_menu(window: &MainWindow, paused: bool) {
    let visible = paused && !window.get_launcher_visible();
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
            measured_fps: None,
            measured_ups: None,
        }
    }

    fn record_frame(&mut self, now: Instant, updates: usize) {
        self.frames_in_sample += 1;
        self.updates_in_sample += updates as u32;

        let elapsed = now.saturating_duration_since(self.sample_started);
        let elapsed_secs = elapsed.as_secs_f32();
        if elapsed_secs < 1.0 {
            return;
        }

        self.measured_fps = Some(self.frames_in_sample as f32 / elapsed_secs);
        self.measured_ups = Some(self.updates_in_sample as f32 / elapsed_secs);
        self.sample_started = now;
        self.frames_in_sample = 0;
        self.updates_in_sample = 0;
    }

    fn display_text(&self) -> String {
        format!(
            "Target {} | FPS {} | UPS {}",
            self.target_label,
            measured_label(self.measured_fps),
            measured_label(self.measured_ups)
        )
    }
}

fn performance_target_label(tick_model: TickModel) -> String {
    match tick_model {
        TickModel::FixedTimestep { hz } => format!("{hz} Hz"),
        TickModel::Variable => "variable".into(),
        TickModel::EmulatorClock => "emulator".into(),
    }
}

fn measured_label(value: Option<f32>) -> String {
    value
        .map(|value| format!("{:.0}", value.max(0.0)))
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
    let scene_item_count = match renderer {
        RenderBackend::Vector => {
            let primitives =
                render::scene_primitives_from_frames_with_layout(&frames, viewport, layout);
            let scene_item_count = primitives.len();
            window.set_raster_visible(false);
            window.set_primitives(ModelRc::new(VecModel::from(primitives)));
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
            window.set_raster_frame(image);
            window.set_raster_visible(true);
            primitive_count
        }
    };
    window.window().request_redraw();
    scene_item_count
}

pub fn run_spacewars_benchmark(
    options: BenchmarkOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let seconds = options.seconds.max(1);
    let mut scenario = HostedScenario::new_spacewars_benchmark(options.seed);
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
            let actions = scenario.actions(&mut input, true);
            scenario.step(&actions, fixed_dt);
            sample.step_time += step_started.elapsed();

            let render_started = Instant::now();
            let frames = scenario.render_frames(options.renderer);
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
            sample.max_total_time = sample.max_total_time.max(frame_started.elapsed());
        }

        sample.wall_time = sample_started.elapsed();
        let row = BenchmarkRow::from_sample(
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
            let primitives = render::scene_primitives_from_frames_with_layout(
                frames,
                BENCHMARK_VIEWPORT,
                layout,
            );
            let scene_item_count = primitives.len();
            black_box(primitives);
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
    max_total_time: Duration,
    wall_time: Duration,
}

#[derive(Debug)]
struct BenchmarkRow {
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
    avg_step_ms: f64,
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
    avg_raster_overview_refresh_ms: f64,
    avg_raster_overview_blit_ms: f64,
    avg_raster_overview_live_ms: f64,
    avg_raster_image_ms: f64,
    avg_total_ms: f64,
    max_total_ms: f64,
}

impl BenchmarkRow {
    fn from_sample(
        second: u64,
        renderer: RenderBackend,
        raster_scale: f32,
        sample: BenchmarkSample,
        counts: SpacewarsBenchmarkCounts,
    ) -> Self {
        let frames = sample.frames.max(1);
        let measured_time = sample.step_time + sample.render_time + sample.present_time;
        Self {
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
            avg_step_ms: avg_ms(sample.step_time, frames),
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
            avg_raster_overview_refresh_ms: avg_ms(sample.raster_timings.overview_refresh, frames),
            avg_raster_overview_blit_ms: avg_ms(sample.raster_timings.overview_blit, frames),
            avg_raster_overview_live_ms: avg_ms(sample.raster_timings.overview_live, frames),
            avg_raster_image_ms: avg_ms(sample.raster_timings.image, frames),
            avg_total_ms: avg_ms(measured_time, frames),
            max_total_ms: duration_ms(sample.max_total_time),
        }
    }
}

fn write_benchmark_header(mut writer: impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "renderer,raster_scale,second,frames,updates,throughput_fps,scene_items,asteroids,fragments,shells,particles,avg_step_ms,avg_render_ms,avg_present_ms,avg_raster_clear_ms,avg_raster_player_ms,avg_raster_player_starfield_ms,avg_raster_player_bodies_ms,avg_raster_player_world_ms,avg_raster_player_sun_planets_ms,avg_raster_player_spaceports_ms,avg_raster_player_effects_ms,avg_raster_player_ships_ms,avg_raster_player_debris_ms,avg_raster_player_particles_ms,avg_raster_player_other_ms,avg_raster_overview_refresh_ms,avg_raster_overview_blit_ms,avg_raster_overview_live_ms,avg_raster_image_ms,avg_total_ms,max_total_ms"
    )
}

fn write_benchmark_row(mut writer: impl Write, row: &BenchmarkRow) -> io::Result<()> {
    writeln!(
        writer,
        "{},{:.2},{},{},{},{:.2},{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
        row.renderer,
        row.raster_scale,
        row.second,
        row.frames,
        row.updates,
        row.throughput_fps,
        row.scene_items,
        row.asteroids,
        row.fragments,
        row.shells,
        row.particles,
        row.avg_step_ms,
        row.avg_render_ms,
        row.avg_present_ms,
        row.avg_raster_clear_ms,
        row.avg_raster_player_ms,
        row.avg_raster_player_starfield_ms,
        row.avg_raster_player_bodies_ms,
        row.avg_raster_player_world_ms,
        row.avg_raster_player_sun_planets_ms,
        row.avg_raster_player_spaceports_ms,
        row.avg_raster_player_effects_ms,
        row.avg_raster_player_ships_ms,
        row.avg_raster_player_debris_ms,
        row.avg_raster_player_particles_ms,
        row.avg_raster_player_other_ms,
        row.avg_raster_overview_refresh_ms,
        row.avg_raster_overview_blit_ms,
        row.avg_raster_overview_live_ms,
        row.avg_raster_image_ms,
        row.avg_total_ms,
        row.max_total_ms,
    )
}

fn avg_ms(duration: Duration, samples: u32) -> f64 {
    duration_ms(duration) / samples.max(1) as f64
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(crate) enum HostedScenario {
    Null(<NullScenario as Scenario>::State),
    Spacewars(Box<<SpacewarsScenario as Scenario>::State>),
}

#[derive(Debug, Clone, PartialEq)]
struct SpacewarsPanelState {
    player_1: PlayerPanelState,
    player_2: PlayerPanelState,
    planet_score_label: String,
    player_1_planet_fraction: f32,
    player_2_planet_fraction: f32,
    player_1_planet_score: String,
    free_planet_score: String,
    player_2_planet_score: String,
    message_text: String,
    performance_text: String,
}

#[derive(Debug, Clone, PartialEq)]
struct PlayerPanelState {
    name: String,
    status: String,
    status_fraction: f32,
    color: CoreColor,
}

impl HostedScenario {
    pub(crate) fn new(
        name: &str,
        seed: u64,
        spacewars_config: SpacewarsConfig,
    ) -> Result<Self, HostError> {
        match name {
            "null" => Ok(Self::Null(NullScenario::init(NullConfig, seed))),
            "spacewars" => Ok(Self::Spacewars(Box::new(SpacewarsScenario::init(
                spacewars_config,
                seed,
            )))),
            _ => Err(HostError::UnknownScenario { name: name.into() }),
        }
    }

    pub(crate) fn new_spacewars_benchmark(seed: u64) -> Self {
        Self::Spacewars(Box::new(SpacewarsScenario::init_benchmark(seed)))
    }

    pub(crate) fn tick_model(&self) -> TickModel {
        match self {
            Self::Null(_) => NullScenario::tick_model(),
            Self::Spacewars(_) => SpacewarsScenario::tick_model(),
        }
    }

    pub(crate) fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        match self {
            Self::Null(state) => NullScenario::step(state, actions, dt),
            Self::Spacewars(state) => SpacewarsScenario::step(state, actions, dt),
        }
    }

    pub(crate) fn zoom_player_in(&mut self, player: usize) {
        if let Self::Spacewars(state) = self {
            state.zoom_player_in(player);
        }
    }

    pub(crate) fn zoom_player_out(&mut self, player: usize) {
        if let Self::Spacewars(state) = self {
            state.zoom_player_out(player);
        }
    }

    fn is_game_over(&self) -> bool {
        match self {
            Self::Null(_) => false,
            Self::Spacewars(state) => state.winner.is_some(),
        }
    }

    pub(crate) fn actions(&self, input: &mut ClientInput, benchmark_active: bool) -> Vec<Action> {
        match self {
            Self::Null(_) => Vec::new(),
            Self::Spacewars(state) if benchmark_active => {
                SpacewarsScenario::benchmark_actions(state)
            }
            Self::Spacewars(_) => input.actions_for_spacewars(),
        }
    }

    pub(crate) fn benchmark_counts(&self) -> SpacewarsBenchmarkCounts {
        match self {
            Self::Null(_) => SpacewarsBenchmarkCounts::default(),
            Self::Spacewars(state) => SpacewarsScenario::benchmark_counts(state),
        }
    }

    #[cfg(test)]
    pub(crate) fn render_frame(&self) -> RenderFrame {
        match self {
            Self::Null(state) => NullScenario::render_frame(state),
            Self::Spacewars(state) => SpacewarsScenario::render_frame(state),
        }
    }

    pub(crate) fn render_frames(&self, renderer: RenderBackend) -> Vec<RenderFrame> {
        match self {
            Self::Null(state) => vec![NullScenario::render_frame(state)],
            Self::Spacewars(state) if renderer == RenderBackend::Raster => {
                SpacewarsScenario::render_raster_local_play_frames(state)
            }
            Self::Spacewars(state) => SpacewarsScenario::render_local_play_frames(state),
        }
    }

    pub(crate) fn frame_layout(&self) -> render::FrameLayout {
        match self {
            Self::Null(_) => render::FrameLayout::EqualHorizontal,
            Self::Spacewars(_) => render::FrameLayout::SpacewarsLocalPlay,
        }
    }

    fn spacewars_panel_state(
        &self,
        paused: bool,
        benchmark_active: bool,
        performance_text: &str,
    ) -> Option<SpacewarsPanelState> {
        match self {
            Self::Null(_) => None,
            Self::Spacewars(state) => Some(spacewars_panel_state(
                state,
                paused,
                benchmark_active,
                performance_text,
            )),
        }
    }
}

fn spacewars_panel_state(
    state: &SpacewarsState,
    paused: bool,
    benchmark_active: bool,
    performance_text: &str,
) -> SpacewarsPanelState {
    let player_1_planets = state.players[0].planet_count;
    let player_2_planets = state.players[1].planet_count;
    let free_planets = state
        .planets
        .len()
        .saturating_sub(player_1_planets + player_2_planets);
    let total_planets = state.planets.len().max(1) as f32;

    SpacewarsPanelState {
        player_1: player_panel_state(state, 0),
        player_2: player_panel_state(state, 1),
        planet_score_label: format!(
            "Planets  P1 {player_1_planets} | Free {free_planets} | P2 {player_2_planets}"
        ),
        player_1_planet_fraction: player_1_planets as f32 / total_planets,
        player_2_planet_fraction: player_2_planets as f32 / total_planets,
        player_1_planet_score: planet_score_text(player_1_planets),
        free_planet_score: planet_score_text(free_planets),
        player_2_planet_score: planet_score_text(player_2_planets),
        message_text: spacewars_panel_message(state, paused, benchmark_active),
        performance_text: performance_text.into(),
    }
}

fn planet_score_text(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        count.to_string()
    }
}

fn spacewars_panel_message(state: &SpacewarsState, paused: bool, benchmark_active: bool) -> String {
    if let Some(winner) = state.winner.and_then(|winner| state.players.get(winner)) {
        return format!("P{} Wins | R Restart | B Bench | Esc Launch", winner.id + 1);
    }

    if benchmark_active && paused {
        "Bench Paused | P/Esc Resume | B Reset | Q Launch".into()
    } else if benchmark_active {
        "Bench | P/Esc Pause | B Reset | R Game".into()
    } else if paused {
        "Paused | P/Esc Resume | R Restart | B Bench | Q Launch".into()
    } else {
        "P/Esc Pause | R Restart | B Bench".into()
    }
}

fn player_panel_state(state: &SpacewarsState, player_index: usize) -> PlayerPanelState {
    let player = &state.players[player_index];
    let ship = &state.ships[player_index];
    if player.eliminated {
        return PlayerPanelState {
            name: format!("Player {}: {}", player.id + 1, player.name),
            status: "Eliminated".into(),
            status_fraction: 0.0,
            color: player.color,
        };
    }

    let status_fraction = ship_life_fraction(ship.life, ship.life_max);
    let percent = display_percent(status_fraction);
    let label = match ship.form {
        ShipForm::Ship => "Ship Health",
        ShipForm::EscapePod => "Pod Rebuild",
    };

    PlayerPanelState {
        name: format!("Player {}: {}", player.id + 1, player.name),
        status: format!("{label}: {percent}%"),
        status_fraction,
        color: player.color,
    }
}

fn ship_life_fraction(life: f32, life_max: f32) -> f32 {
    if life_max <= 0.0 {
        return 0.0;
    }

    (life / life_max).clamp(0.0, 1.0)
}

fn display_percent(fraction: f32) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
}

fn set_spacewars_panel(window: &MainWindow, state: Option<SpacewarsPanelState>) {
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

    fn hosted_scenario(name: &str, seed: u64) -> Result<HostedScenario, HostError> {
        HostedScenario::new(name, seed, SpacewarsConfig::default())
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
    fn performance_stats_reports_target_and_measured_rates() {
        let start = Instant::now();
        let mut stats = PerformanceStats::new(TickModel::FixedTimestep { hz: 60 }, start);

        assert_eq!(stats.display_text(), "Target 60 Hz | FPS -- | UPS --");

        for frame in 1..=60 {
            stats.record_frame(start + Duration::from_secs_f64(frame as f64 / 60.0), 1);
        }

        assert_eq!(stats.display_text(), "Target 60 Hz | FPS 60 | UPS 60");
    }

    #[test]
    fn null_scenario_renders_empty_frame() {
        let scenario = hosted_scenario("null", 0).unwrap();

        assert!(scenario.render_frame().layers.is_empty());
        assert_eq!(scenario.spacewars_panel_state(false, false, ""), None);
    }

    #[test]
    fn spacewars_scenario_renders_initial_world() {
        let scenario = hosted_scenario("spacewars", 0).unwrap();
        let frame = scenario.render_frame();

        match &scenario {
            HostedScenario::Spacewars(state) => {
                assert_eq!(state.config, SpacewarsConfig::default());
                assert!(state.sun.is_some());
                assert!(!state.planets.is_empty());
            }
            HostedScenario::Null(_) => panic!("spacewars scenario should not host null"),
        }
        assert!(!frame.layers.is_empty());
        assert!(matches!(
            scenario.tick_model(),
            TickModel::FixedTimestep { hz: 60 }
        ));
    }

    #[test]
    fn spacewars_scenario_uses_supplied_config() {
        let config = small_duel_config();
        let scenario = HostedScenario::new("spacewars", 0, config.clone()).unwrap();

        match &scenario {
            HostedScenario::Spacewars(state) => {
                assert_eq!(state.config, config);
                assert!(state.sun.is_none());
                assert!(state.planets.is_empty());
            }
            HostedScenario::Null(_) => panic!("spacewars scenario should not host null"),
        }
    }

    #[test]
    fn spacewars_scenario_renders_original_style_local_play_frames_for_client() {
        let scenario = hosted_scenario("spacewars", 0).unwrap();
        let frames = scenario.render_frames(RenderBackend::Vector);

        let HostedScenario::Spacewars(state) = &scenario else {
            panic!("spacewars scenario should not host null");
        };

        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].camera.center.x, state.ships[0].position.x);
        assert_eq!(frames[0].camera.center.y, state.ships[0].position.y);
        assert_eq!(frames[1].camera.center.x, state.ships[1].position.x);
        assert_eq!(frames[1].camera.center.y, state.ships[1].position.y);
        assert_eq!(frames[0].camera.height, frames[1].camera.height);
        assert_eq!(frames[2].camera.center.x, 1200.0);
        assert_eq!(frames[2].camera.center.y, 1200.0);
        assert_eq!(frames[3].camera, frames[2].camera);
        assert_eq!(
            scenario.frame_layout(),
            render::FrameLayout::SpacewarsLocalPlay
        );
    }

    #[test]
    fn spacewars_panel_state_reports_health_pod_and_planet_score() {
        let mut scenario = hosted_scenario("spacewars", 0).unwrap();
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };
        let total_planets = state.planets.len().max(1) as f32;
        state.ships[0].life = state.ships[0].life_max * 0.5;
        state.ships[1].form = ShipForm::EscapePod;
        state.ships[1].life = state.ships[1].life_max * 0.25;
        state.players[0].planet_count = 1;
        state.players[1].planet_count = 2;
        let free_planets = state.planets.len().saturating_sub(3);

        let panel = spacewars_panel_state(state, false, false, "Target 60 Hz | FPS 60 | UPS 60");

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
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };
        state.players[0].eliminated = true;
        state.winner = Some(1);

        let panel = spacewars_panel_state(state, false, false, "Target 60 Hz | FPS 60 | UPS 0");

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
        let mut scenario = hosted_scenario("spacewars", 0).unwrap();
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };

        let panel = spacewars_panel_state(state, true, false, "Target 60 Hz | FPS 60 | UPS 0");

        assert_eq!(
            panel.message_text,
            "Paused | P/Esc Resume | R Restart | B Bench | Q Launch"
        );
        assert_eq!(panel.performance_text, "Target 60 Hz | FPS 60 | UPS 0");
    }

    #[test]
    fn spacewars_panel_state_reports_benchmark_message() {
        let mut scenario = HostedScenario::new_spacewars_benchmark(0);
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };

        let panel = spacewars_panel_state(state, false, true, "Target 60 Hz | FPS 42 | UPS 60");

        assert_eq!(panel.message_text, "Bench | P/Esc Pause | B Reset | R Game");
        assert_eq!(panel.performance_text, "Target 60 Hz | FPS 42 | UPS 60");
    }

    #[test]
    fn reset_key_restarts_spacewars_with_same_seed() {
        let config = small_duel_config();
        let mut scenario = HostedScenario::new("spacewars", 42, config.clone()).unwrap();
        let expected = HostedScenario::new("spacewars", 42, config.clone()).unwrap();
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };
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
            &config,
        );

        let HostedScenario::Spacewars(state) = &scenario else {
            panic!("spacewars scenario should not host null");
        };
        let HostedScenario::Spacewars(expected) = &expected else {
            panic!("spacewars scenario should not host null");
        };
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
            &SpacewarsConfig::default(),
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
            &SpacewarsConfig::default(),
        );
        let HostedScenario::Spacewars(state) = &scenario else {
            panic!("spacewars scenario should not host null");
        };
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
            &SpacewarsConfig::default(),
        );
        assert!(!paused);
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
            &SpacewarsConfig::default(),
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
            &SpacewarsConfig::default(),
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
            &SpacewarsConfig::default(),
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
            &SpacewarsConfig::default(),
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
            &SpacewarsConfig::default(),
        );
        assert_eq!(result.ingame_controls_visible, Some(false));
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
            &SpacewarsConfig::default(),
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
            &SpacewarsConfig::default(),
        );

        let counts = scenario.benchmark_counts();
        assert!(benchmark_active);
        assert!(!paused);
        assert_eq!(accumulator, Duration::ZERO);
        assert_eq!(counts.asteroids, 100);
        assert_eq!(counts.particles, 1_200);
    }

    #[test]
    fn escape_after_game_over_returns_to_launcher_without_stepping() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };
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
            &SpacewarsConfig::default(),
        );

        let HostedScenario::Spacewars(state) = &scenario else {
            panic!("spacewars scenario should not host null");
        };
        assert!(result.return_to_launcher);
        assert_eq!(result.updates, 0);
        assert_eq!(accumulator, Duration::ZERO);
        assert!(!paused);
        assert_eq!(state.tick, tick_before);
    }

    #[test]
    fn scenario_controls_resume_restart_and_start_benchmark() {
        let mut scenario = hosted_scenario("spacewars", 42).unwrap();
        let mut input = ClientInput::default();
        let mut accumulator = Duration::from_secs(1);
        let mut controls = ScenarioControls::default();
        let mut paused = true;
        let mut benchmark_active = false;

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
            &SpacewarsConfig::default(),
        );
        assert!(!paused);
        assert_eq!(accumulator, Duration::ZERO);

        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };
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
            &SpacewarsConfig::default(),
        );
        let HostedScenario::Spacewars(state) = &scenario else {
            panic!("spacewars scenario should not host null");
        };
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
            &SpacewarsConfig::default(),
        );
        assert!(benchmark_active);
        assert_eq!(scenario.benchmark_counts().asteroids, 100);
    }
}
