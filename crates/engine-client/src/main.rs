//! Scenario client: Slint UI, input, rendering, settings, and scenario host.
//!
//! The compile-time registry currently hosts Pizza, Rover Lab, and Spacewars
//! from the launcher, with Null retained as a hidden test scenario.

mod client_scenarios;
mod host;
mod input;
mod ipc;
mod raster;
mod render;
mod settings;

use std::cell::RefCell;
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use clap::{Parser, ValueEnum};
use engine_common::{
    CrashBehavior, MAX_PIZZA_BALL_SPAWN_RATE, MAX_PIZZA_DESIRED_BALLS,
    MAX_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC, MAX_SPACEWARS_PLAYER_HEALTH_PERCENT,
    MAX_SPACEWARS_PLAYER_VIEW_HEIGHT, MAX_SPACEWARS_UNIVERSE_RADIUS, MIN_PIZZA_BALL_SPAWN_RATE,
    MIN_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC, MIN_SPACEWARS_PLAYER_HEALTH_PERCENT,
    MIN_SPACEWARS_PLAYER_VIEW_HEIGHT, MIN_SPACEWARS_UNIVERSE_RADIUS, PizzaSettings,
    RendererSetting, Settings, SpacewarsSettings,
};
#[cfg(test)]
use engine_core::SpacewarsConfig;
use scenario_pizza::{
    MAX_BENCHMARK_BALLS, PizzaBenchmarkConfig, PizzaBenchmarkWorkload, PizzaGravityModel,
    PizzaPhysicsBackend,
};
use settings::LoadStatus;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};
use tracing_subscriber::EnvFilter;

slint::include_modules!();

const MIN_RASTER_SCALE: f32 = 0.1;
const MAX_RASTER_SCALE: f32 = 3.0;
const DEFAULT_RASTER_SCALE: f32 = 1.0;
const PRESET_CUSTOM: &str = "Custom";
const PRESET_ORIGINAL: &str = "Original";
const PRESET_SMALL_DUEL: &str = "Small Duel";
const PRESET_DENSE_ASTEROIDS: &str = "Dense Asteroids";
const PRESET_LONG_GAME: &str = "Long Game";

#[derive(Parser, Debug)]
#[command(name = "engine-client", about = "Spacewars scenario host")]
struct Args {
    /// Scenario to load.
    #[arg(long)]
    scenario: Option<String>,

    /// Scenario seed.
    #[arg(long)]
    seed: Option<u64>,

    /// Force CrashBehavior::Freeze for this run without writing settings.
    #[arg(long)]
    dev: bool,

    /// Render an internal moving debug frame instead of an empty window.
    #[arg(long)]
    debug_render: bool,

    /// Add this many triangles to the debug render frame for renderer stress checks.
    #[arg(long, default_value_t = 0)]
    debug_triangles: usize,

    /// Start the selected scenario's visual benchmark workload in the UI.
    #[arg(long)]
    benchmark: bool,

    /// Run the selected scenario's benchmark without a window and print CSV rows.
    #[arg(long)]
    benchmark_headless: bool,

    /// Number of seconds to run --benchmark-headless.
    #[arg(long, default_value_t = 30)]
    benchmark_seconds: u64,

    /// Optional CSV file path for --benchmark-headless output.
    #[arg(long)]
    benchmark_report: Option<PathBuf>,

    /// Pizza benchmark population. The visual default is intentionally inspectable.
    #[arg(long, default_value_t = scenario_pizza::DEFAULT_BENCHMARK_BALLS)]
    pizza_benchmark_balls: usize,

    /// Pizza benchmark physics implementation.
    #[arg(long, value_enum, default_value = "rapier")]
    pizza_benchmark_backend: PizzaBenchmarkBackendArg,

    /// Pizza benchmark gravity implementation/accuracy.
    #[arg(long, value_enum, default_value = "fast")]
    pizza_benchmark_gravity: PizzaBenchmarkGravityArg,

    /// Pizza benchmark workload shape.
    #[arg(long, value_enum, default_value = "dense")]
    pizza_benchmark_workload: PizzaBenchmarkWorkloadArg,

    /// Settings directory override. Useful for Pi/systemd services.
    #[arg(long)]
    config_dir: Option<PathBuf>,

    /// Presentation renderer to use for game frames.
    #[arg(long, value_enum)]
    renderer: Option<RendererArg>,

    /// Internal resolution scale for --renderer raster.
    #[arg(long)]
    raster_scale: Option<f32>,

    /// Request fullscreen presentation for this run.
    #[arg(long, conflicts_with = "windowed")]
    fullscreen: bool,

    /// Force windowed presentation even if settings request fullscreen.
    #[arg(long)]
    windowed: bool,

    /// Pi/kiosk launch mode: fullscreen, direct launch, and no forced desktop backend.
    #[arg(long, conflicts_with = "windowed")]
    kiosk: bool,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum RendererArg {
    #[default]
    Vector,
    Raster,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum PizzaBenchmarkBackendArg {
    Classic,
    #[default]
    Rapier,
}

impl From<PizzaBenchmarkBackendArg> for PizzaPhysicsBackend {
    fn from(value: PizzaBenchmarkBackendArg) -> Self {
        match value {
            PizzaBenchmarkBackendArg::Classic => Self::Classic,
            PizzaBenchmarkBackendArg::Rapier => Self::Rapier,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum PizzaBenchmarkGravityArg {
    Exact,
    Full,
    #[default]
    Fast,
}

impl From<PizzaBenchmarkGravityArg> for PizzaGravityModel {
    fn from(value: PizzaBenchmarkGravityArg) -> Self {
        match value {
            PizzaBenchmarkGravityArg::Exact => Self::Exact,
            PizzaBenchmarkGravityArg::Full => Self::Full,
            PizzaBenchmarkGravityArg::Fast => Self::Fast,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum PizzaBenchmarkWorkloadArg {
    Sparse,
    #[default]
    Dense,
    Churn,
}

impl From<PizzaBenchmarkWorkloadArg> for PizzaBenchmarkWorkload {
    fn from(value: PizzaBenchmarkWorkloadArg) -> Self {
        match value {
            PizzaBenchmarkWorkloadArg::Sparse => Self::Sparse,
            PizzaBenchmarkWorkloadArg::Dense => Self::Dense,
            PizzaBenchmarkWorkloadArg::Churn => Self::Churn,
        }
    }
}

impl From<RendererArg> for host::RenderBackend {
    fn from(value: RendererArg) -> Self {
        match value {
            RendererArg::Vector => Self::Vector,
            RendererArg::Raster => Self::Raster,
        }
    }
}

impl From<RendererSetting> for host::RenderBackend {
    fn from(value: RendererSetting) -> Self {
        match value {
            RendererSetting::Vector => Self::Vector,
            RendererSetting::Raster => Self::Raster,
        }
    }
}

impl Args {
    fn uses_debug_render(&self) -> bool {
        self.debug_render || self.debug_triangles != 0
    }

    fn uses_benchmark(&self) -> bool {
        self.benchmark || self.benchmark_headless
    }

    fn has_launch_override(&self) -> bool {
        self.scenario.is_some()
            || self.seed.is_some()
            || self.renderer.is_some()
            || self.raster_scale.is_some()
            || self.kiosk
    }

    fn benchmark_configuration(&self) -> host::BenchmarkConfiguration {
        host::BenchmarkConfiguration {
            pizza: PizzaBenchmarkConfig {
                backend: self.pizza_benchmark_backend.into(),
                gravity: self.pizza_benchmark_gravity.into(),
                workload: self.pizza_benchmark_workload.into(),
                ball_count: self.pizza_benchmark_balls.min(MAX_BENCHMARK_BALLS),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EffectiveLaunch {
    scenario: String,
    seed: u64,
    renderer: host::RenderBackend,
    raster_scale: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let settings_path = settings_path_from_args(&args)?;
    let loaded_settings = settings::load_settings(&settings_path)?;
    let mut loaded = loaded_settings.settings;
    let mut needs_writeback = loaded_settings.status.needs_writeback();
    needs_writeback |= normalize_log_level(&mut loaded);

    init_tracing(&loaded);
    log_settings_load_status(&settings_path, &loaded_settings.status);
    needs_writeback |= normalize_launch_settings(&mut loaded);
    needs_writeback |= normalize_spacewars_settings(&mut loaded);
    needs_writeback |= normalize_pizza_settings(&mut loaded);
    let effective_launch = effective_launch_options(&args, &loaded);

    if !args.uses_debug_render() {
        host::validate_scenario(effective_launch.scenario.as_str())?;
    }
    if args.uses_benchmark()
        && !host::scenario_registration(effective_launch.scenario.as_str())
            .is_some_and(|registration| registration.capabilities.benchmark)
    {
        return Err(format!(
            "scenario {:?} does not support benchmark mode",
            effective_launch.scenario
        )
        .into());
    }
    tracing::info!(
        path = %settings_path.display(),
        last_scenario = ?loaded.last_scenario,
        launch_scenario = %loaded.launch.scenario,
        launch_seed = loaded.launch.seed,
        launch_renderer = ?loaded.launch.renderer,
        launch_raster_scale = loaded.launch.raster_scale,
        crash_behavior = ?loaded.runtime.crash_behavior,
        "loaded settings."
    );

    let effective_crash_behavior = if args.dev {
        CrashBehavior::Freeze
    } else {
        loaded.runtime.crash_behavior
    };
    tracing::info!(
        scenario = %effective_launch.scenario,
        seed = effective_launch.seed,
        renderer = ?effective_launch.renderer,
        raster_scale = effective_launch.raster_scale,
        dev = args.dev,
        crash_behavior = ?effective_crash_behavior,
        "engine-client starting."
    );

    if args.benchmark_headless {
        host::run_benchmark(host::BenchmarkOptions {
            scenario: effective_launch.scenario.clone(),
            seed: effective_launch.seed,
            seconds: args.benchmark_seconds,
            report_path: args.benchmark_report.clone(),
            renderer: effective_launch.renderer,
            raster_scale: effective_launch.raster_scale,
            configuration: args.benchmark_configuration(),
            settings: loaded.clone(),
        })?;
        return Ok(());
    }

    let launch_directly = should_launch_directly(&args);
    let settings = Arc::new(RwLock::new(loaded));
    save_startup_settings_if_needed(&settings, &settings_path, needs_writeback)?;

    select_slint_backend(&args)?;
    let window = MainWindow::new()?;
    let _control_server = ipc::start_control_server(&window, ipc::control_socket_path());
    let render_timer = Rc::new(RefCell::new(None));
    let scenario_controls = host::new_scenario_controls();
    install_launcher_callbacks(
        &window,
        Rc::clone(&render_timer),
        Rc::clone(&scenario_controls),
        Arc::clone(&settings),
        settings_path.clone(),
    );
    install_ingame_menu_callbacks(
        &window,
        Rc::clone(&render_timer),
        Rc::clone(&scenario_controls),
        Arc::clone(&settings),
    );
    apply_video_settings(&window, &args, &settings.read().unwrap());

    if args.uses_debug_render() {
        *render_timer.borrow_mut() = Some(host::start_debug_render_loop(
            &window,
            args.debug_triangles,
            effective_launch.renderer,
        ));
    } else if launch_directly {
        let scenario_settings = settings.read().unwrap().clone();
        *render_timer.borrow_mut() = Some(start_scenario_from_launch(
            &window,
            &effective_launch,
            args.benchmark,
            args.benchmark_configuration(),
            Rc::clone(&scenario_controls),
            scenario_settings,
        )?);
    } else {
        show_launcher(&window, &effective_launch, &settings.read().unwrap());
    }

    window.run()?;
    Ok(())
}

fn effective_launch_options(args: &Args, settings: &Settings) -> EffectiveLaunch {
    let scenario = if args.uses_benchmark() && args.scenario.is_none() {
        "spacewars".into()
    } else {
        args.scenario
            .clone()
            .unwrap_or_else(|| settings.launch.scenario.clone())
    };

    EffectiveLaunch {
        scenario,
        seed: args.seed.unwrap_or(settings.launch.seed),
        renderer: args
            .renderer
            .map(host::RenderBackend::from)
            .unwrap_or_else(|| settings.launch.renderer.into()),
        raster_scale: normalize_raster_scale(
            args.raster_scale.unwrap_or(settings.launch.raster_scale),
        ),
    }
}

fn normalize_launch_settings(settings: &mut Settings) -> bool {
    let mut changed = false;

    let saved_scenario_is_launchable =
        host::scenario_registration(settings.launch.scenario.as_str())
            .is_some_and(|registration| registration.launcher_visible);
    if settings.launch.scenario.trim().is_empty() || !saved_scenario_is_launchable {
        tracing::warn!(
            scenario = %settings.launch.scenario,
            "invalid saved launch scenario; falling back to spacewars."
        );
        settings.launch.scenario = "spacewars".into();
        changed = true;
    }

    let raster_scale = normalize_raster_scale(settings.launch.raster_scale);
    if raster_scale != settings.launch.raster_scale {
        tracing::warn!(
            raster_scale = settings.launch.raster_scale,
            normalized = raster_scale,
            "invalid saved raster scale; using normalized value."
        );
        settings.launch.raster_scale = raster_scale;
        changed = true;
    }

    changed
}

fn normalize_spacewars_settings(settings: &mut Settings) -> bool {
    let normalized = settings.spacewars.normalized();
    if normalized == settings.spacewars {
        return false;
    }

    tracing::warn!(
        universe_radius = settings.spacewars.universe_radius,
        normalized_universe_radius = normalized.universe_radius,
        asteroid_probability_per_sec = settings.spacewars.asteroid_probability_per_sec,
        normalized_asteroid_probability_per_sec = normalized.asteroid_probability_per_sec,
        player_health_percent = settings.spacewars.player_health_percent,
        normalized_player_health_percent = normalized.player_health_percent,
        player_1_view_height = settings.spacewars.player_1_view_height,
        normalized_player_1_view_height = normalized.player_1_view_height,
        player_2_view_height = settings.spacewars.player_2_view_height,
        normalized_player_2_view_height = normalized.player_2_view_height,
        "invalid saved Spacewars setup; using normalized values."
    );
    settings.spacewars = normalized;
    true
}

fn normalize_pizza_settings(settings: &mut Settings) -> bool {
    let normalized = settings.pizza.normalized();
    if normalized == settings.pizza {
        return false;
    }
    tracing::warn!(
        desired_balls = settings.pizza.desired_balls,
        normalized_desired_balls = normalized.desired_balls,
        ball_spawn_rate = settings.pizza.ball_spawn_rate,
        normalized_ball_spawn_rate = normalized.ball_spawn_rate,
        "invalid Pizza setup; using normalized values."
    );
    settings.pizza = normalized;
    true
}

fn normalize_raster_scale(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_RASTER_SCALE, MAX_RASTER_SCALE)
    } else {
        DEFAULT_RASTER_SCALE
    }
}

fn should_launch_directly(args: &Args) -> bool {
    args.uses_debug_render() || args.uses_benchmark() || args.has_launch_override()
}

fn show_launcher(window: &MainWindow, launch: &EffectiveLaunch, settings: &Settings) {
    window.set_primitives(ModelRc::new(VecModel::from(Vec::<ScenePrimitive>::new())));
    window.set_vector_minimaps_visible(false);
    window.set_raster_visible(false);
    window.set_spacewars_ui_visible(false);
    window.set_scenario_pointer_enabled(false);
    window.set_ingame_menu_visible(false);
    window.set_ingame_controls_visible(false);
    let scenario_names = host::launcher_scenario_names()
        .into_iter()
        .map(SharedString::from)
        .collect::<Vec<_>>();
    window.set_launcher_scenarios(ModelRc::new(VecModel::from(scenario_names)));
    window.set_launcher_scenario(SharedString::from(launch.scenario.clone()));
    apply_scenario_metadata(window, launch.scenario.as_str());
    window.set_launcher_seed_text(SharedString::from(launch.seed.to_string()));
    window.set_launcher_renderer(SharedString::from(renderer_label(launch.renderer)));
    window.set_launcher_raster_scale_text(SharedString::from(format_raster_scale(
        launch.raster_scale,
    )));
    let setup = settings.spacewars.normalized();
    window.set_launcher_spacewars_preset(SharedString::from(preset_label_for_setup(&setup)));
    window.set_launcher_universe_radius_text(SharedString::from(setup.universe_radius.to_string()));
    window.set_launcher_use_planets(SharedString::from(bool_label(setup.use_planets)));
    window.set_launcher_asteroids_enabled(SharedString::from(bool_label(setup.asteroids_enabled)));
    window.set_launcher_asteroid_probability_text(SharedString::from(format_float_setting(
        setup.asteroid_probability_per_sec,
    )));
    window.set_launcher_player_health_text(SharedString::from(
        setup.player_health_percent.to_string(),
    ));
    let pizza = settings.pizza.normalized();
    window
        .set_launcher_pizza_desired_balls_text(SharedString::from(pizza.desired_balls.to_string()));
    window.set_launcher_pizza_spawn_rate_text(SharedString::from(format_float_setting(
        pizza.ball_spawn_rate,
    )));
    window.set_launcher_error_text(SharedString::from(""));
    window.set_launcher_controls_visible(false);
    window.set_launcher_visible(true);
}

fn apply_scenario_metadata(window: &MainWindow, scenario: &str) {
    let Some(registration) = host::scenario_registration(scenario) else {
        window.set_scenario_benchmark_available(false);
        window.set_scenario_player_zoom_available(false);
        window.set_scenario_controls_help(SharedString::from(""));
        return;
    };
    window.set_scenario_benchmark_available(registration.capabilities.benchmark);
    window.set_scenario_player_zoom_available(registration.capabilities.player_zoom);
    window.set_scenario_controls_help(SharedString::from(registration.controls_help));
}

fn save_startup_settings_if_needed(
    settings: &Arc<RwLock<Settings>>,
    settings_path: &Path,
    needs_writeback: bool,
) -> Result<bool, settings::SettingsError> {
    if !needs_writeback {
        return Ok(false);
    }

    let settings = settings.read().unwrap();
    settings::save_settings(&settings, settings_path)?;
    tracing::info!(path = %settings_path.display(), "saved settings.");
    Ok(true)
}

fn settings_path_from_args(args: &Args) -> Result<PathBuf, settings::SettingsError> {
    if let Some(config_dir) = &args.config_dir {
        Ok(config_dir.join(settings::SETTINGS_FILENAME))
    } else {
        settings::settings_path()
    }
}

fn apply_video_settings(window: &MainWindow, args: &Args, settings: &Settings) {
    window.set_app_full_screen(effective_fullscreen(args, settings));
}

fn effective_fullscreen(args: &Args, settings: &Settings) -> bool {
    if args.windowed {
        false
    } else {
        args.kiosk || args.fullscreen || settings.video.fullscreen
    }
}

fn install_launcher_callbacks(
    window: &MainWindow,
    render_timer: Rc<RefCell<Option<Timer>>>,
    scenario_controls: host::SharedScenarioControls,
    settings: Arc<RwLock<Settings>>,
    settings_path: PathBuf,
) {
    let weak = window.as_weak();
    let game_timer = Rc::clone(&render_timer);
    let game_controls = Rc::clone(&scenario_controls);
    let game_settings = Arc::clone(&settings);
    let game_settings_path = settings_path.clone();
    window.on_launcher_start_game(move || {
        handle_launcher_start(
            &weak,
            &game_timer,
            &game_controls,
            &game_settings,
            &game_settings_path,
            false,
        );
    });

    let weak = window.as_weak();
    let benchmark_timer = Rc::clone(&render_timer);
    let benchmark_controls = Rc::clone(&scenario_controls);
    let benchmark_settings = Arc::clone(&settings);
    window.on_launcher_start_benchmark(move || {
        handle_launcher_start(
            &weak,
            &benchmark_timer,
            &benchmark_controls,
            &benchmark_settings,
            &settings_path,
            true,
        );
    });

    let weak = window.as_weak();
    window.on_launcher_scenario_selected(move |scenario| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        apply_scenario_metadata(&window, scenario.as_str());
    });

    let weak = window.as_weak();
    window.on_launcher_apply_preset(move || {
        handle_launcher_apply_preset(&weak);
    });

    let weak = window.as_weak();
    window.on_launcher_p1_zoom_in(move || {
        handle_launcher_zoom(&weak, 0, true);
    });

    let weak = window.as_weak();
    window.on_launcher_p1_zoom_out(move || {
        handle_launcher_zoom(&weak, 0, false);
    });

    let weak = window.as_weak();
    window.on_launcher_p2_zoom_in(move || {
        handle_launcher_zoom(&weak, 1, true);
    });

    let weak = window.as_weak();
    window.on_launcher_p2_zoom_out(move || {
        handle_launcher_zoom(&weak, 1, false);
    });

    window.on_launcher_quit(move || {
        if let Err(err) = slint::quit_event_loop() {
            tracing::error!(error = %err, "failed to quit event loop.");
        }
    });
}

fn install_ingame_menu_callbacks(
    window: &MainWindow,
    render_timer: Rc<RefCell<Option<Timer>>>,
    scenario_controls: host::SharedScenarioControls,
    settings: Arc<RwLock<Settings>>,
) {
    let controls = Rc::clone(&scenario_controls);
    window.on_ingame_resume(move || {
        controls.borrow_mut().request_resume();
    });

    let controls = Rc::clone(&scenario_controls);
    window.on_ingame_restart(move || {
        controls.borrow_mut().request_restart();
    });

    let controls = Rc::clone(&scenario_controls);
    window.on_ingame_start_benchmark(move || {
        controls.borrow_mut().request_benchmark();
    });

    let controls = Rc::clone(&scenario_controls);
    window.on_ingame_p1_zoom_in(move || {
        controls.borrow_mut().request_zoom_in(0);
    });

    let controls = Rc::clone(&scenario_controls);
    window.on_ingame_p1_zoom_out(move || {
        controls.borrow_mut().request_zoom_out(0);
    });

    let controls = Rc::clone(&scenario_controls);
    window.on_ingame_p2_zoom_in(move || {
        controls.borrow_mut().request_zoom_in(1);
    });

    let controls = Rc::clone(&scenario_controls);
    window.on_ingame_p2_zoom_out(move || {
        controls.borrow_mut().request_zoom_out(1);
    });

    let weak = window.as_weak();
    window.on_ingame_return_launcher(move || {
        handle_return_to_launcher(&weak, &render_timer, &scenario_controls, &settings);
    });
}

fn handle_return_to_launcher(
    weak_window: &slint::Weak<MainWindow>,
    render_timer: &Rc<RefCell<Option<Timer>>>,
    scenario_controls: &host::SharedScenarioControls,
    settings: &Arc<RwLock<Settings>>,
) {
    let Some(window) = weak_window.upgrade() else {
        return;
    };

    if let Some(timer) = render_timer.borrow_mut().take() {
        timer.stop();
    }
    scenario_controls.borrow_mut().clear();
    let settings = settings.read().unwrap();
    let launch = launch_from_settings(&settings);
    show_launcher(&window, &launch, &settings);
}

fn handle_launcher_apply_preset(weak_window: &slint::Weak<MainWindow>) {
    let Some(window) = weak_window.upgrade() else {
        return;
    };
    window.set_launcher_error_text(SharedString::from(""));

    let preset = window.get_launcher_spacewars_preset();
    match spacewars_preset_from_label(preset.as_str()) {
        Ok(Some(setup)) => set_spacewars_setup_fields(&window, &setup),
        Ok(None) => {}
        Err(message) => window.set_launcher_error_text(SharedString::from(message)),
    }
}

fn handle_launcher_zoom(weak_window: &slint::Weak<MainWindow>, player: usize, zoom_in: bool) {
    let Some(window) = weak_window.upgrade() else {
        return;
    };

    let universe_radius = match parse_u32_setting(
        window.get_launcher_universe_radius_text().as_str(),
        "World radius",
        MIN_SPACEWARS_UNIVERSE_RADIUS,
        MAX_SPACEWARS_UNIVERSE_RADIUS,
    ) {
        Ok(value) => value,
        Err(message) => {
            window.set_launcher_error_text(SharedString::from(message));
            return;
        }
    };

    let current_text = match player {
        0 => window.get_launcher_p1_zoom_text(),
        1 => window.get_launcher_p2_zoom_text(),
        _ => return,
    };
    let current = match parse_f32_setting(
        current_text.as_str(),
        "Zoom",
        MIN_SPACEWARS_PLAYER_VIEW_HEIGHT,
        MAX_SPACEWARS_PLAYER_VIEW_HEIGHT,
    ) {
        Ok(value) => value,
        Err(message) => {
            window.set_launcher_error_text(SharedString::from(message));
            return;
        }
    };

    let adjusted = adjust_player_view_height(current, universe_radius, zoom_in);
    let adjusted = SharedString::from(format_float_setting(adjusted));
    match player {
        0 => window.set_launcher_p1_zoom_text(adjusted),
        1 => window.set_launcher_p2_zoom_text(adjusted),
        _ => {}
    }
    window.set_launcher_spacewars_preset(SharedString::from(PRESET_CUSTOM));
    window.set_launcher_error_text(SharedString::from(""));
}

fn handle_launcher_start(
    weak_window: &slint::Weak<MainWindow>,
    render_timer: &Rc<RefCell<Option<Timer>>>,
    scenario_controls: &host::SharedScenarioControls,
    settings: &Arc<RwLock<Settings>>,
    settings_path: &Path,
    start_benchmark: bool,
) {
    let Some(window) = weak_window.upgrade() else {
        return;
    };
    window.set_launcher_error_text(SharedString::from(""));

    let current_settings = settings.read().unwrap().clone();
    let selections = match launcher_selections_from_window(&window, &current_settings) {
        Ok(selections) => selections,
        Err(message) => {
            window.set_launcher_error_text(SharedString::from(message));
            return;
        }
    };
    if start_benchmark
        && !host::scenario_registration(selections.launch.scenario.as_str())
            .is_some_and(|registration| registration.capabilities.benchmark)
    {
        window.set_launcher_error_text(SharedString::from(
            "The selected scenario does not support benchmark mode.",
        ));
        return;
    }
    if let Err(err) = persist_launcher_settings(settings, settings_path, &selections) {
        window.set_launcher_error_text(SharedString::from(format!(
            "Could not save settings: {err}"
        )));
        return;
    }
    let scenario_settings = settings.read().unwrap().clone();

    match start_scenario_from_launch(
        &window,
        &selections.launch,
        start_benchmark,
        host::BenchmarkConfiguration::default(),
        Rc::clone(scenario_controls),
        scenario_settings,
    ) {
        Ok(timer) => {
            let mut timer_slot = render_timer.borrow_mut();
            if let Some(old_timer) = timer_slot.take() {
                old_timer.stop();
            }
            scenario_controls.borrow_mut().clear();
            *timer_slot = Some(timer);
            window.set_launcher_visible(false);
            window.set_launcher_controls_visible(false);
            window.set_ingame_menu_visible(false);
            window.set_ingame_controls_visible(false);
        }
        Err(err) => {
            window.set_launcher_error_text(SharedString::from(err.to_string()));
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct LauncherSelections {
    launch: EffectiveLaunch,
    spacewars: SpacewarsSettings,
    pizza: PizzaSettings,
}

fn persist_launcher_settings(
    settings: &Arc<RwLock<Settings>>,
    settings_path: &Path,
    selections: &LauncherSelections,
) -> Result<bool, settings::SettingsError> {
    let mut settings = settings.write().unwrap();
    let launch = &selections.launch;
    let renderer = renderer_setting(launch.renderer);
    let raster_scale = normalize_raster_scale(launch.raster_scale);
    let spacewars = selections.spacewars.normalized();
    let pizza = selections.pizza.normalized();
    let mut changed = false;

    if settings.launch.scenario != launch.scenario {
        settings.launch.scenario = launch.scenario.clone();
        changed = true;
    }
    if settings.launch.seed != launch.seed {
        settings.launch.seed = launch.seed;
        changed = true;
    }
    if settings.launch.renderer != renderer {
        settings.launch.renderer = renderer;
        changed = true;
    }
    if settings.launch.raster_scale != raster_scale {
        settings.launch.raster_scale = raster_scale;
        changed = true;
    }
    if settings.last_scenario.as_deref() != Some(launch.scenario.as_str()) {
        settings.last_scenario = Some(launch.scenario.clone());
        changed = true;
    }
    if settings.spacewars != spacewars {
        settings.spacewars = spacewars;
        changed = true;
    }
    if settings.pizza != pizza {
        settings.pizza = pizza;
        changed = true;
    }

    if changed {
        settings::save_settings(&settings, settings_path)?;
        tracing::info!(path = %settings_path.display(), "saved launcher settings.");
    }

    Ok(changed)
}

fn launch_from_settings(settings: &Settings) -> EffectiveLaunch {
    EffectiveLaunch {
        scenario: settings.launch.scenario.clone(),
        seed: settings.launch.seed,
        renderer: settings.launch.renderer.into(),
        raster_scale: normalize_raster_scale(settings.launch.raster_scale),
    }
}

#[cfg(test)]
fn spacewars_config_from_settings(settings: &Settings) -> SpacewarsConfig {
    spacewars_config_from_setup(&settings.spacewars.normalized())
}

#[cfg(test)]
fn spacewars_config_from_setup(setup: &SpacewarsSettings) -> SpacewarsConfig {
    let mut config = SpacewarsConfig {
        universe_radius: setup.universe_radius,
        use_planets: setup.use_planets,
        asteroid_probability_per_sec: if setup.asteroids_enabled {
            setup.asteroid_probability_per_sec
        } else {
            0.0
        },
        player_view_heights: [setup.player_1_view_height, setup.player_2_view_height],
        ..SpacewarsConfig::default()
    };

    for player in &mut config.players {
        player.health_percent = setup.player_health_percent;
    }

    config
}

fn set_spacewars_setup_fields(window: &MainWindow, setup: &SpacewarsSettings) {
    let setup = setup.normalized();
    window.set_launcher_spacewars_preset(SharedString::from(preset_label_for_setup(&setup)));
    window.set_launcher_universe_radius_text(SharedString::from(setup.universe_radius.to_string()));
    window.set_launcher_use_planets(SharedString::from(bool_label(setup.use_planets)));
    window.set_launcher_asteroids_enabled(SharedString::from(bool_label(setup.asteroids_enabled)));
    window.set_launcher_asteroid_probability_text(SharedString::from(format_float_setting(
        setup.asteroid_probability_per_sec,
    )));
    window.set_launcher_player_health_text(SharedString::from(
        setup.player_health_percent.to_string(),
    ));
    window.set_launcher_p1_zoom_text(SharedString::from(format_float_setting(
        setup.player_1_view_height,
    )));
    window.set_launcher_p2_zoom_text(SharedString::from(format_float_setting(
        setup.player_2_view_height,
    )));
}

fn spacewars_preset_from_label(label: &str) -> Result<Option<SpacewarsSettings>, String> {
    match label.trim() {
        PRESET_CUSTOM => Ok(None),
        PRESET_ORIGINAL => Ok(Some(original_spacewars_preset())),
        PRESET_SMALL_DUEL => Ok(Some(small_duel_spacewars_preset())),
        PRESET_DENSE_ASTEROIDS => Ok(Some(dense_asteroids_spacewars_preset())),
        PRESET_LONG_GAME => Ok(Some(long_game_spacewars_preset())),
        other => Err(format!("Unknown preset {other:?}.")),
    }
}

fn preset_label_for_setup(setup: &SpacewarsSettings) -> &'static str {
    let setup = setup.normalized();
    if setup == original_spacewars_preset() {
        PRESET_ORIGINAL
    } else if setup == small_duel_spacewars_preset() {
        PRESET_SMALL_DUEL
    } else if setup == dense_asteroids_spacewars_preset() {
        PRESET_DENSE_ASTEROIDS
    } else if setup == long_game_spacewars_preset() {
        PRESET_LONG_GAME
    } else {
        PRESET_CUSTOM
    }
}

fn original_spacewars_preset() -> SpacewarsSettings {
    SpacewarsSettings::default()
}

fn small_duel_spacewars_preset() -> SpacewarsSettings {
    SpacewarsSettings {
        universe_radius: 600,
        use_planets: false,
        asteroids_enabled: false,
        asteroid_probability_per_sec: 0.0,
        player_health_percent: 100,
        ..SpacewarsSettings::default()
    }
}

fn dense_asteroids_spacewars_preset() -> SpacewarsSettings {
    SpacewarsSettings {
        universe_radius: 1200,
        use_planets: true,
        asteroids_enabled: true,
        asteroid_probability_per_sec: 80.0,
        player_health_percent: 100,
        ..SpacewarsSettings::default()
    }
}

fn long_game_spacewars_preset() -> SpacewarsSettings {
    SpacewarsSettings {
        universe_radius: 2400,
        use_planets: true,
        asteroids_enabled: true,
        asteroid_probability_per_sec: 10.0,
        player_health_percent: 250,
        ..SpacewarsSettings::default()
    }
}

fn launcher_selections_from_window(
    window: &MainWindow,
    current_settings: &Settings,
) -> Result<LauncherSelections, String> {
    let launch = launch_options_from_window(window)?;
    let (spacewars, pizza) = match launch.scenario.as_str() {
        "spacewars" => (
            spacewars_setup_from_window(window)?,
            current_settings.pizza.clone(),
        ),
        "pizza" => (
            current_settings.spacewars.clone(),
            pizza_setup_from_window(window)?,
        ),
        _ => (
            current_settings.spacewars.clone(),
            current_settings.pizza.clone(),
        ),
    };
    Ok(LauncherSelections {
        launch,
        spacewars,
        pizza,
    })
}

fn launch_options_from_window(window: &MainWindow) -> Result<EffectiveLaunch, String> {
    launch_options_from_values(
        window.get_launcher_scenario().as_str(),
        window.get_launcher_seed_text().as_str(),
        window.get_launcher_renderer().as_str(),
        window.get_launcher_raster_scale_text().as_str(),
    )
}

fn spacewars_setup_from_window(window: &MainWindow) -> Result<SpacewarsSettings, String> {
    spacewars_setup_from_values(
        window.get_launcher_universe_radius_text().as_str(),
        window.get_launcher_use_planets().as_str(),
        window.get_launcher_asteroids_enabled().as_str(),
        window.get_launcher_asteroid_probability_text().as_str(),
        window.get_launcher_player_health_text().as_str(),
        window.get_launcher_p1_zoom_text().as_str(),
        window.get_launcher_p2_zoom_text().as_str(),
    )
}

fn pizza_setup_from_window(window: &MainWindow) -> Result<PizzaSettings, String> {
    pizza_setup_from_values(
        window.get_launcher_pizza_desired_balls_text().as_str(),
        window.get_launcher_pizza_spawn_rate_text().as_str(),
    )
}

fn launch_options_from_values(
    scenario: &str,
    seed: &str,
    renderer: &str,
    raster_scale: &str,
) -> Result<EffectiveLaunch, String> {
    let scenario = scenario.trim();
    if !host::is_known_scenario(scenario) {
        return Err(format!("Unknown scenario {scenario:?}."));
    }

    let seed = seed
        .trim()
        .parse::<u64>()
        .map_err(|_| "Seed must be a non-negative integer.".to_string())?;
    let renderer = renderer_from_label(renderer)?;
    let raster_scale = raster_scale
        .trim()
        .parse::<f32>()
        .map(normalize_raster_scale)
        .map_err(|_| "Raster scale must be a number.".to_string())?;

    Ok(EffectiveLaunch {
        scenario: scenario.into(),
        seed,
        renderer,
        raster_scale,
    })
}

fn spacewars_setup_from_values(
    universe_radius: &str,
    use_planets: &str,
    asteroids_enabled: &str,
    asteroid_probability_per_sec: &str,
    player_health_percent: &str,
    player_1_view_height: &str,
    player_2_view_height: &str,
) -> Result<SpacewarsSettings, String> {
    let setup = SpacewarsSettings {
        universe_radius: parse_u32_setting(
            universe_radius,
            "World radius",
            MIN_SPACEWARS_UNIVERSE_RADIUS,
            MAX_SPACEWARS_UNIVERSE_RADIUS,
        )?,
        use_planets: bool_from_label(use_planets)?,
        asteroids_enabled: bool_from_label(asteroids_enabled)?,
        asteroid_probability_per_sec: parse_f32_setting(
            asteroid_probability_per_sec,
            "Asteroid rate",
            MIN_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC,
            MAX_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC,
        )?,
        player_health_percent: parse_u32_setting(
            player_health_percent,
            "Player health",
            MIN_SPACEWARS_PLAYER_HEALTH_PERCENT,
            MAX_SPACEWARS_PLAYER_HEALTH_PERCENT,
        )?,
        player_1_view_height: parse_f32_setting(
            player_1_view_height,
            "Player 1 zoom",
            MIN_SPACEWARS_PLAYER_VIEW_HEIGHT,
            MAX_SPACEWARS_PLAYER_VIEW_HEIGHT,
        )?,
        player_2_view_height: parse_f32_setting(
            player_2_view_height,
            "Player 2 zoom",
            MIN_SPACEWARS_PLAYER_VIEW_HEIGHT,
            MAX_SPACEWARS_PLAYER_VIEW_HEIGHT,
        )?,
    };

    Ok(setup.normalized())
}

fn pizza_setup_from_values(
    desired_balls: &str,
    ball_spawn_rate: &str,
) -> Result<PizzaSettings, String> {
    Ok(PizzaSettings {
        desired_balls: parse_u32_setting(
            desired_balls,
            "Desired balls",
            0,
            MAX_PIZZA_DESIRED_BALLS,
        )?,
        ball_spawn_rate: parse_f32_setting(
            ball_spawn_rate,
            "Spawn rate",
            MIN_PIZZA_BALL_SPAWN_RATE,
            MAX_PIZZA_BALL_SPAWN_RATE,
        )?,
    })
}

fn parse_u32_setting(value: &str, label: &str, min: u32, max: u32) -> Result<u32, String> {
    value
        .trim()
        .parse::<u32>()
        .map(|value| value.clamp(min, max))
        .map_err(|_| format!("{label} must be an integer from {min} to {max}."))
}

fn parse_f32_setting(value: &str, label: &str, min: f32, max: f32) -> Result<f32, String> {
    value
        .trim()
        .parse::<f32>()
        .map(|value| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                min
            }
        })
        .map_err(|_| format!("{label} must be a number from {min:.0} to {max:.0}."))
}

fn adjust_player_view_height(current: f32, universe_radius: u32, zoom_in: bool) -> f32 {
    let step = player_zoom_step(universe_radius);
    let next = if zoom_in {
        current - step
    } else {
        current + step
    };
    let max = player_zoom_max(universe_radius);
    next.clamp(MIN_SPACEWARS_PLAYER_VIEW_HEIGHT, max)
}

fn player_zoom_step(universe_radius: u32) -> f32 {
    (player_zoom_max(universe_radius) / 100.0).max(1.0)
}

fn player_zoom_max(universe_radius: u32) -> f32 {
    let diameter = universe_radius.saturating_mul(2) as f32;
    diameter
        .hypot(diameter)
        .min(MAX_SPACEWARS_PLAYER_VIEW_HEIGHT)
        .max(MIN_SPACEWARS_PLAYER_VIEW_HEIGHT)
}

fn start_scenario_from_launch(
    window: &MainWindow,
    launch: &EffectiveLaunch,
    start_benchmark: bool,
    benchmark_configuration: host::BenchmarkConfiguration,
    controls: host::SharedScenarioControls,
    settings: Settings,
) -> Result<Timer, host::HostError> {
    controls.borrow_mut().clear();
    apply_scenario_metadata(window, launch.scenario.as_str());
    host::start_scenario_loop(
        window,
        launch.scenario.as_str(),
        launch.seed,
        host::ScenarioLoopOptions {
            start_benchmark,
            benchmark_configuration,
            renderer: launch.renderer,
            raster_scale: launch.raster_scale,
            controls: Some(controls),
            settings,
        },
    )
}

fn renderer_from_label(label: &str) -> Result<host::RenderBackend, String> {
    match label.trim().to_ascii_lowercase().as_str() {
        "vector" => Ok(host::RenderBackend::Vector),
        "raster" => Ok(host::RenderBackend::Raster),
        _ => Err(format!("Unknown renderer {label:?}.")),
    }
}

fn renderer_label(renderer: host::RenderBackend) -> &'static str {
    match renderer {
        host::RenderBackend::Vector => "vector",
        host::RenderBackend::Raster => "raster",
    }
}

fn renderer_setting(renderer: host::RenderBackend) -> RendererSetting {
    match renderer {
        host::RenderBackend::Vector => RendererSetting::Vector,
        host::RenderBackend::Raster => RendererSetting::Raster,
    }
}

fn format_raster_scale(value: f32) -> String {
    format!("{:.1}", normalize_raster_scale(value))
}

fn format_float_setting(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn bool_from_label(label: &str) -> Result<bool, String> {
    match label.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => Err(format!("Expected on or off, got {label:?}.")),
    }
}

fn select_slint_backend(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let selector = slint::BackendSelector::new();
    let selector = if let Some(backend_name) = forced_slint_backend(args) {
        selector.backend_name(backend_name.into())
    } else {
        selector
    };
    selector.select()?;
    Ok(())
}

fn forced_slint_backend(args: &Args) -> Option<&'static str> {
    forced_slint_backend_for_env(args, env::var_os("SLINT_BACKEND").is_some())
}

fn forced_slint_backend_for_env(args: &Args, slint_backend_is_set: bool) -> Option<&'static str> {
    if slint_backend_is_set || args.kiosk {
        None
    } else {
        Some("winit")
    }
}

fn normalize_log_level(settings: &mut Settings) -> bool {
    if EnvFilter::try_new(settings.runtime.log_level.as_str()).is_ok() {
        return false;
    }

    eprintln!(
        "invalid runtime.log_level {:?}; falling back to \"info\"",
        settings.runtime.log_level
    );
    settings.runtime.log_level = "info".into();
    true
}

fn init_tracing(settings: &Settings) {
    let filter = match env::var("RUST_LOG") {
        Ok(rust_log) => EnvFilter::try_new(&rust_log).unwrap_or_else(|e| {
            eprintln!(
                "invalid RUST_LOG {:?}: {e}; falling back to settings runtime.log_level {:?}",
                rust_log, settings.runtime.log_level
            );
            EnvFilter::try_new(settings.runtime.log_level.as_str())
                .expect("runtime.log_level was normalized before tracing init")
        }),
        Err(_) => EnvFilter::try_new(settings.runtime.log_level.as_str())
            .expect("runtime.log_level was normalized before tracing init"),
    };

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}

fn log_settings_load_status(path: &Path, status: &LoadStatus) {
    match status {
        LoadStatus::Existing => {}
        LoadStatus::Missing => tracing::info!(
            path = %path.display(),
            "settings file missing; using defaults."
        ),
        LoadStatus::Migrated => tracing::info!(
            path = %path.display(),
            "loaded settings; normalized writeback required."
        ),
        LoadStatus::RecoveredMalformed {
            backup_path,
            reason,
        } => tracing::warn!(
            path = %path.display(),
            backup_path = %backup_path.display(),
            reason = %reason,
            "settings file was malformed; using defaults."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::DEFAULT_SPACEWARS_PLAYER_VIEW_HEIGHT;

    fn base_args() -> Args {
        Args {
            scenario: None,
            seed: None,
            dev: false,
            debug_render: false,
            debug_triangles: 0,
            benchmark: false,
            benchmark_headless: false,
            benchmark_seconds: 30,
            benchmark_report: None,
            pizza_benchmark_balls: scenario_pizza::DEFAULT_BENCHMARK_BALLS,
            pizza_benchmark_backend: PizzaBenchmarkBackendArg::Rapier,
            pizza_benchmark_gravity: PizzaBenchmarkGravityArg::Fast,
            pizza_benchmark_workload: PizzaBenchmarkWorkloadArg::Dense,
            config_dir: None,
            renderer: None,
            raster_scale: None,
            fullscreen: false,
            windowed: false,
            kiosk: false,
        }
    }

    #[test]
    fn effective_launch_uses_saved_settings_when_cli_is_absent() {
        let mut settings = Settings::default();
        settings.launch.scenario = "null".into();
        settings.launch.seed = 77;
        settings.launch.renderer = RendererSetting::Raster;
        settings.launch.raster_scale = 2.0;

        let launch = effective_launch_options(&base_args(), &settings);

        assert_eq!(launch.scenario, "null");
        assert_eq!(launch.seed, 77);
        assert_eq!(launch.renderer, host::RenderBackend::Raster);
        assert_eq!(launch.raster_scale, 2.0);
    }

    #[test]
    fn effective_launch_cli_args_override_saved_settings() {
        let mut settings = Settings::default();
        settings.launch.scenario = "null".into();
        settings.launch.seed = 77;
        settings.launch.renderer = RendererSetting::Vector;
        settings.launch.raster_scale = 1.0;
        let mut args = base_args();
        args.scenario = Some("spacewars".into());
        args.seed = Some(99);
        args.renderer = Some(RendererArg::Raster);
        args.raster_scale = Some(2.5);

        let launch = effective_launch_options(&args, &settings);

        assert_eq!(launch.scenario, "spacewars");
        assert_eq!(launch.seed, 99);
        assert_eq!(launch.renderer, host::RenderBackend::Raster);
        assert_eq!(launch.raster_scale, 2.5);
    }

    #[test]
    fn benchmark_without_cli_scenario_uses_spacewars() {
        let mut settings = Settings::default();
        settings.launch.scenario = "null".into();
        let mut args = base_args();
        args.benchmark = true;

        let launch = effective_launch_options(&args, &settings);

        assert_eq!(launch.scenario, "spacewars");
    }

    #[test]
    fn launcher_values_parse_to_effective_launch() {
        let launch = launch_options_from_values("spacewars", "123", "raster", "2.0").unwrap();

        assert_eq!(launch.scenario, "spacewars");
        assert_eq!(launch.seed, 123);
        assert_eq!(launch.renderer, host::RenderBackend::Raster);
        assert_eq!(launch.raster_scale, 2.0);
    }

    #[test]
    fn launcher_values_report_invalid_input() {
        assert!(launch_options_from_values("bogus", "0", "vector", "1.0").is_err());
        assert!(launch_options_from_values("spacewars", "-1", "vector", "1.0").is_err());
        assert!(launch_options_from_values("spacewars", "0", "unknown", "1.0").is_err());
        assert!(launch_options_from_values("spacewars", "0", "vector", "wide").is_err());
    }

    #[test]
    fn spacewars_setup_values_parse_and_normalize() {
        let setup =
            spacewars_setup_from_values("2400", "off", "on", "75.5", "250", "420", "640").unwrap();

        assert_eq!(setup.universe_radius, 2400);
        assert!(!setup.use_planets);
        assert!(setup.asteroids_enabled);
        assert_eq!(setup.asteroid_probability_per_sec, 75.5);
        assert_eq!(setup.player_health_percent, 250);
        assert_eq!(setup.player_1_view_height, 420.0);
        assert_eq!(setup.player_2_view_height, 640.0);

        let clamped =
            spacewars_setup_from_values("99999", "on", "off", "9999", "0", "1", "99999").unwrap();
        assert_eq!(clamped.universe_radius, MAX_SPACEWARS_UNIVERSE_RADIUS);
        assert!(clamped.use_planets);
        assert!(!clamped.asteroids_enabled);
        assert_eq!(
            clamped.asteroid_probability_per_sec,
            MAX_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC
        );
        assert_eq!(
            clamped.player_health_percent,
            MIN_SPACEWARS_PLAYER_HEALTH_PERCENT
        );
        assert_eq!(
            clamped.player_1_view_height,
            MIN_SPACEWARS_PLAYER_VIEW_HEIGHT
        );
        assert_eq!(
            clamped.player_2_view_height,
            MAX_SPACEWARS_PLAYER_VIEW_HEIGHT
        );
    }

    #[test]
    fn spacewars_setup_values_report_invalid_input() {
        assert!(
            spacewars_setup_from_values("wide", "on", "on", "20", "100", "320", "320").is_err()
        );
        assert!(
            spacewars_setup_from_values("1200", "maybe", "on", "20", "100", "320", "320").is_err()
        );
        assert!(
            spacewars_setup_from_values("1200", "on", "sometimes", "20", "100", "320", "320")
                .is_err()
        );
        assert!(
            spacewars_setup_from_values("1200", "on", "on", "dense", "100", "320", "320").is_err()
        );
        assert!(
            spacewars_setup_from_values("1200", "on", "on", "20", "strong", "320", "320").is_err()
        );
        assert!(
            spacewars_setup_from_values("1200", "on", "on", "20", "100", "near", "320").is_err()
        );
        assert!(
            spacewars_setup_from_values("1200", "on", "on", "20", "100", "320", "far").is_err()
        );
    }

    #[test]
    fn spacewars_presets_map_to_setup_fields() {
        assert_eq!(
            spacewars_preset_from_label(PRESET_ORIGINAL).unwrap(),
            Some(SpacewarsSettings::default())
        );
        assert_eq!(
            spacewars_preset_from_label(PRESET_SMALL_DUEL).unwrap(),
            Some(SpacewarsSettings {
                universe_radius: 600,
                use_planets: false,
                asteroids_enabled: false,
                asteroid_probability_per_sec: 0.0,
                player_health_percent: 100,
                ..SpacewarsSettings::default()
            })
        );
        assert_eq!(
            spacewars_preset_from_label(PRESET_DENSE_ASTEROIDS).unwrap(),
            Some(SpacewarsSettings {
                universe_radius: 1200,
                use_planets: true,
                asteroids_enabled: true,
                asteroid_probability_per_sec: 80.0,
                player_health_percent: 100,
                ..SpacewarsSettings::default()
            })
        );
        assert_eq!(
            spacewars_preset_from_label(PRESET_LONG_GAME).unwrap(),
            Some(SpacewarsSettings {
                universe_radius: 2400,
                use_planets: true,
                asteroids_enabled: true,
                asteroid_probability_per_sec: 10.0,
                player_health_percent: 250,
                ..SpacewarsSettings::default()
            })
        );
        assert_eq!(spacewars_preset_from_label(PRESET_CUSTOM).unwrap(), None);
        assert!(spacewars_preset_from_label("Arcade").is_err());
    }

    #[test]
    fn setup_fields_infer_matching_preset_label() {
        assert_eq!(
            preset_label_for_setup(&SpacewarsSettings::default()),
            PRESET_ORIGINAL
        );
        assert_eq!(
            preset_label_for_setup(&small_duel_spacewars_preset()),
            PRESET_SMALL_DUEL
        );

        let mut custom = SpacewarsSettings::default();
        custom.universe_radius = 1800;
        assert_eq!(preset_label_for_setup(&custom), PRESET_CUSTOM);
    }

    #[test]
    fn launch_directly_only_when_cli_or_special_mode_requests_it() {
        let mut args = base_args();
        assert!(!should_launch_directly(&args));

        args.scenario = Some("spacewars".into());
        assert!(should_launch_directly(&args));

        let mut args = base_args();
        args.benchmark = true;
        assert!(should_launch_directly(&args));

        let mut args = base_args();
        args.debug_render = true;
        assert!(should_launch_directly(&args));

        let mut args = base_args();
        args.kiosk = true;
        assert!(should_launch_directly(&args));

        let mut args = base_args();
        args.fullscreen = true;
        assert!(!should_launch_directly(&args));
    }

    #[test]
    fn settings_path_from_args_uses_config_dir_override() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = base_args();
        args.config_dir = Some(dir.path().to_path_buf());

        assert_eq!(
            settings_path_from_args(&args).unwrap(),
            dir.path().join(settings::SETTINGS_FILENAME)
        );
    }

    #[test]
    fn effective_fullscreen_uses_cli_and_settings_precedence() {
        let mut settings = Settings::default();
        let mut args = base_args();
        assert!(!effective_fullscreen(&args, &settings));

        settings.video.fullscreen = true;
        assert!(effective_fullscreen(&args, &settings));

        args.windowed = true;
        assert!(!effective_fullscreen(&args, &settings));

        args.windowed = false;
        args.fullscreen = true;
        settings.video.fullscreen = false;
        assert!(effective_fullscreen(&args, &settings));

        args.fullscreen = false;
        args.kiosk = true;
        assert!(effective_fullscreen(&args, &settings));
    }

    #[test]
    fn kiosk_mode_does_not_force_desktop_slint_backend() {
        let mut args = base_args();
        assert_eq!(forced_slint_backend_for_env(&args, false), Some("winit"));
        assert_eq!(forced_slint_backend_for_env(&args, true), None);

        args.kiosk = true;
        assert_eq!(forced_slint_backend_for_env(&args, false), None);
    }

    #[test]
    fn persist_launcher_settings_updates_defaults_setup_and_last_scenario() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let settings = Arc::new(RwLock::new(Settings::default()));
        let selections = LauncherSelections {
            launch: EffectiveLaunch {
                scenario: "spacewars".into(),
                seed: 123,
                renderer: host::RenderBackend::Raster,
                raster_scale: 2.0,
            },
            spacewars: SpacewarsSettings {
                universe_radius: 2400,
                use_planets: false,
                asteroids_enabled: false,
                asteroid_probability_per_sec: 80.0,
                player_health_percent: 250,
                player_1_view_height: 420.0,
                player_2_view_height: 640.0,
            },
            pizza: PizzaSettings {
                desired_balls: 321,
                ball_spawn_rate: 0.42,
            },
        };

        assert!(persist_launcher_settings(&settings, &path, &selections).unwrap());
        assert!(!persist_launcher_settings(&settings, &path, &selections).unwrap());

        let stored = settings.read().unwrap();
        assert_eq!(stored.launch.scenario, "spacewars");
        assert_eq!(stored.launch.seed, 123);
        assert_eq!(stored.launch.renderer, RendererSetting::Raster);
        assert_eq!(stored.launch.raster_scale, 2.0);
        assert_eq!(stored.spacewars, selections.spacewars);
        assert_eq!(stored.pizza, selections.pizza);
        assert_eq!(stored.last_scenario.as_deref(), Some("spacewars"));
        drop(stored);

        let reloaded = settings::load_settings(&path).unwrap();
        assert_eq!(reloaded.status, settings::LoadStatus::Existing);
        assert_eq!(reloaded.settings.launch.seed, 123);
        assert_eq!(reloaded.settings.launch.renderer, RendererSetting::Raster);
        assert_eq!(reloaded.settings.launch.raster_scale, 2.0);
        assert_eq!(reloaded.settings.spacewars, selections.spacewars);
        assert_eq!(reloaded.settings.pizza, selections.pizza);
        assert_eq!(
            reloaded.settings.last_scenario.as_deref(),
            Some("spacewars")
        );
    }

    #[test]
    fn startup_settings_writeback_does_not_record_cli_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut settings = Settings::default();
        settings.last_scenario = None;
        let settings = Arc::new(RwLock::new(settings));

        assert!(save_startup_settings_if_needed(&settings, &path, true).unwrap());

        let reloaded = settings::load_settings(&path).unwrap();
        assert_eq!(reloaded.status, settings::LoadStatus::Existing);
        assert_eq!(reloaded.settings.last_scenario, None);
    }

    #[test]
    fn default_spacewars_settings_map_to_default_config() {
        assert_eq!(
            spacewars_config_from_settings(&Settings::default()),
            SpacewarsConfig::default()
        );
    }

    #[test]
    fn spacewars_settings_map_to_scenario_config() {
        let mut settings = Settings::default();
        settings.spacewars = SpacewarsSettings {
            universe_radius: 2400,
            use_planets: false,
            asteroids_enabled: true,
            asteroid_probability_per_sec: 75.0,
            player_health_percent: 250,
            player_1_view_height: 420.0,
            player_2_view_height: 640.0,
        };

        let config = spacewars_config_from_settings(&settings);

        assert_eq!(config.universe_radius, 2400);
        assert!(!config.use_planets);
        assert_eq!(config.asteroid_probability_per_sec, 75.0);
        assert_eq!(config.players[0].health_percent, 250);
        assert_eq!(config.players[1].health_percent, 250);
        assert_eq!(config.player_view_heights, [420.0, 640.0]);
        assert!(config.use_starfield);
        assert!(config.use_textures);
        assert_eq!(config.fps, 60);
    }

    #[test]
    fn disabled_asteroids_keep_density_setting_but_disable_scenario_spawn() {
        let mut settings = Settings::default();
        settings.spacewars.asteroids_enabled = false;
        settings.spacewars.asteroid_probability_per_sec = 75.0;

        let config = spacewars_config_from_settings(&settings);

        assert_eq!(settings.spacewars.asteroid_probability_per_sec, 75.0);
        assert_eq!(config.asteroid_probability_per_sec, 0.0);
    }

    #[test]
    fn spacewars_settings_normalization_clamps_saved_values() {
        let mut settings = Settings::default();
        settings.spacewars.universe_radius = 1;
        settings.spacewars.asteroid_probability_per_sec = 9999.0;
        settings.spacewars.player_health_percent = 0;
        settings.spacewars.player_1_view_height = f32::NAN;
        settings.spacewars.player_2_view_height = 99999.0;

        assert!(normalize_spacewars_settings(&mut settings));
        assert_eq!(settings.spacewars.universe_radius, 300);
        assert_eq!(
            settings.spacewars.asteroid_probability_per_sec,
            MAX_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC
        );
        assert_eq!(settings.spacewars.player_health_percent, 1);
        assert_eq!(
            settings.spacewars.player_1_view_height,
            DEFAULT_SPACEWARS_PLAYER_VIEW_HEIGHT
        );
        assert_eq!(
            settings.spacewars.player_2_view_height,
            MAX_SPACEWARS_PLAYER_VIEW_HEIGHT
        );

        assert!(!normalize_spacewars_settings(&mut settings));
    }

    #[test]
    fn pizza_settings_normalization_clamps_saved_values() {
        let mut settings = Settings::default();
        settings.pizza.desired_balls = MAX_PIZZA_DESIRED_BALLS + 1;
        settings.pizza.ball_spawn_rate = f32::NAN;

        assert!(normalize_pizza_settings(&mut settings));
        assert_eq!(settings.pizza.desired_balls, MAX_PIZZA_DESIRED_BALLS);
        assert_eq!(
            settings.pizza.ball_spawn_rate,
            engine_common::DEFAULT_PIZZA_BALL_SPAWN_RATE
        );
        assert!(!normalize_pizza_settings(&mut settings));
    }

    #[test]
    fn pizza_setup_values_parse_and_validate_caps() {
        assert_eq!(
            pizza_setup_from_values("321", "0.42").unwrap(),
            PizzaSettings {
                desired_balls: 321,
                ball_spawn_rate: 0.42,
            }
        );
        assert_eq!(
            pizza_setup_from_values("501", "1.0").unwrap(),
            PizzaSettings {
                desired_balls: MAX_PIZZA_DESIRED_BALLS,
                ball_spawn_rate: MAX_PIZZA_BALL_SPAWN_RATE,
            }
        );
        assert!(pizza_setup_from_values("many", "0.42").is_err());
        assert!(pizza_setup_from_values("24", "often").is_err());
    }

    #[test]
    fn player_zoom_buttons_adjust_view_height_by_original_slider_step() {
        let zoomed_in = adjust_player_view_height(320.0, 1200, true);
        let zoomed_out = adjust_player_view_height(320.0, 1200, false);

        assert_eq!(zoomed_in, 320.0 - player_zoom_step(1200));
        assert_eq!(zoomed_out, 320.0 + player_zoom_step(1200));
        assert_eq!(
            adjust_player_view_height(1.0, 1200, true),
            MIN_SPACEWARS_PLAYER_VIEW_HEIGHT
        );
        assert_eq!(
            adjust_player_view_height(99999.0, 1200, false),
            player_zoom_max(1200)
        );
    }

    #[test]
    fn launch_settings_normalization_recovers_invalid_saved_values() {
        let mut settings = Settings::default();
        settings.launch.scenario.clear();
        settings.launch.raster_scale = 99.0;

        assert!(normalize_launch_settings(&mut settings));
        assert_eq!(settings.launch.scenario, "spacewars");
        assert_eq!(settings.launch.raster_scale, MAX_RASTER_SCALE);

        settings.launch.raster_scale = f32::NAN;
        assert!(normalize_launch_settings(&mut settings));
        assert_eq!(settings.launch.raster_scale, DEFAULT_RASTER_SCALE);
    }
}
