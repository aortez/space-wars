//! Spacewars client: Slint UI + custom drawing + input + audio + scenario host.
//!
//! M9 state: opens a Slint window, loads/saves user settings, hosts null or
//! Spacewars scenarios, and renders their `RenderFrame`s.

mod host;
mod input;
mod raster;
mod render;
mod settings;

use std::cell::RefCell;
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use clap::{Parser, ValueEnum};
use engine_common::{CrashBehavior, RendererSetting, Settings};
use settings::LoadStatus;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};
use tracing_subscriber::EnvFilter;

slint::include_modules!();

const MIN_RASTER_SCALE: f32 = 0.1;
const MAX_RASTER_SCALE: f32 = 3.0;
const DEFAULT_RASTER_SCALE: f32 = 1.0;

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

    /// Start the Spacewars visual benchmark workload in the UI.
    #[arg(long)]
    benchmark: bool,

    /// Run the Spacewars benchmark without a window and print CSV rows.
    #[arg(long)]
    benchmark_headless: bool,

    /// Number of seconds to run --benchmark-headless.
    #[arg(long, default_value_t = 30)]
    benchmark_seconds: u64,

    /// Optional CSV file path for --benchmark-headless output.
    #[arg(long)]
    benchmark_report: Option<PathBuf>,

    /// Presentation renderer to use for game frames.
    #[arg(long, value_enum)]
    renderer: Option<RendererArg>,

    /// Internal resolution scale for --renderer raster.
    #[arg(long)]
    raster_scale: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum RendererArg {
    #[default]
    Vector,
    Raster,
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

    let settings_path = settings::settings_path()?;
    let loaded_settings = settings::load_settings(&settings_path)?;
    let mut loaded = loaded_settings.settings;
    let mut needs_writeback = loaded_settings.status.needs_writeback();
    needs_writeback |= normalize_log_level(&mut loaded);

    init_tracing(&loaded);
    log_settings_load_status(&settings_path, &loaded_settings.status);
    needs_writeback |= normalize_launch_settings(&mut loaded);
    let effective_launch = effective_launch_options(&args, &loaded);

    if !args.uses_debug_render() {
        host::validate_scenario(effective_launch.scenario.as_str())?;
    }
    if args.uses_benchmark() && effective_launch.scenario != "spacewars" {
        return Err("benchmark mode currently supports only the spacewars scenario".into());
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
        host::run_spacewars_benchmark(host::BenchmarkOptions {
            seed: effective_launch.seed,
            seconds: args.benchmark_seconds,
            report_path: args.benchmark_report.clone(),
            renderer: effective_launch.renderer,
            raster_scale: effective_launch.raster_scale,
        })?;
        return Ok(());
    }

    let launch_directly = should_launch_directly(&args);
    let settings = Arc::new(RwLock::new(loaded));
    save_startup_settings_if_needed(&settings, &settings_path, needs_writeback)?;

    select_slint_backend()?;
    let window = MainWindow::new()?;
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

    if args.uses_debug_render() {
        *render_timer.borrow_mut() = Some(host::start_debug_render_loop(
            &window,
            args.debug_triangles,
            effective_launch.renderer,
        ));
    } else if launch_directly {
        *render_timer.borrow_mut() = Some(start_scenario_from_launch(
            &window,
            &effective_launch,
            args.benchmark,
            Rc::clone(&scenario_controls),
        )?);
    } else {
        show_launcher(&window, &effective_launch);
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

    if settings.launch.scenario.trim().is_empty()
        || !host::is_known_scenario(settings.launch.scenario.as_str())
    {
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

fn show_launcher(window: &MainWindow, launch: &EffectiveLaunch) {
    window.set_primitives(ModelRc::new(VecModel::from(Vec::<ScenePrimitive>::new())));
    window.set_raster_visible(false);
    window.set_spacewars_ui_visible(false);
    window.set_ingame_menu_visible(false);
    window.set_ingame_controls_visible(false);
    window.set_launcher_scenario(SharedString::from(launch.scenario.clone()));
    window.set_launcher_seed_text(SharedString::from(launch.seed.to_string()));
    window.set_launcher_renderer(SharedString::from(renderer_label(launch.renderer)));
    window.set_launcher_raster_scale_text(SharedString::from(format_raster_scale(
        launch.raster_scale,
    )));
    window.set_launcher_error_text(SharedString::from(""));
    window.set_launcher_controls_visible(false);
    window.set_launcher_visible(true);
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
    let launch = launch_from_settings(&settings.read().unwrap());
    show_launcher(&window, &launch);
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

    let launch = match launch_options_from_window(&window) {
        Ok(launch) => launch,
        Err(message) => {
            window.set_launcher_error_text(SharedString::from(message));
            return;
        }
    };
    if start_benchmark && launch.scenario != "spacewars" {
        window.set_launcher_error_text(SharedString::from(
            "Benchmark mode currently supports only spacewars.",
        ));
        return;
    }
    if let Err(err) = persist_launch_settings(settings, settings_path, &launch) {
        window.set_launcher_error_text(SharedString::from(format!(
            "Could not save settings: {err}"
        )));
        return;
    }

    match start_scenario_from_launch(
        &window,
        &launch,
        start_benchmark,
        Rc::clone(scenario_controls),
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

fn persist_launch_settings(
    settings: &Arc<RwLock<Settings>>,
    settings_path: &Path,
    launch: &EffectiveLaunch,
) -> Result<bool, settings::SettingsError> {
    let mut settings = settings.write().unwrap();
    let renderer = renderer_setting(launch.renderer);
    let raster_scale = normalize_raster_scale(launch.raster_scale);
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

fn launch_options_from_window(window: &MainWindow) -> Result<EffectiveLaunch, String> {
    launch_options_from_values(
        window.get_launcher_scenario().as_str(),
        window.get_launcher_seed_text().as_str(),
        window.get_launcher_renderer().as_str(),
        window.get_launcher_raster_scale_text().as_str(),
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

fn start_scenario_from_launch(
    window: &MainWindow,
    launch: &EffectiveLaunch,
    start_benchmark: bool,
    controls: host::SharedScenarioControls,
) -> Result<Timer, host::HostError> {
    controls.borrow_mut().clear();
    host::start_scenario_loop(
        window,
        launch.scenario.as_str(),
        launch.seed,
        host::ScenarioLoopOptions {
            start_benchmark,
            renderer: launch.renderer,
            raster_scale: launch.raster_scale,
            controls: Some(controls),
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

fn select_slint_backend() -> Result<(), Box<dyn std::error::Error>> {
    let selector = slint::BackendSelector::new();
    if env::var_os("SLINT_BACKEND").is_some() {
        selector.select()?;
    } else {
        selector.backend_name("winit".into()).select()?;
    }
    Ok(())
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
            renderer: None,
            raster_scale: None,
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
    }

    #[test]
    fn persist_launch_settings_updates_defaults_and_last_scenario() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let settings = Arc::new(RwLock::new(Settings::default()));
        let launch = EffectiveLaunch {
            scenario: "spacewars".into(),
            seed: 123,
            renderer: host::RenderBackend::Raster,
            raster_scale: 2.0,
        };

        assert!(persist_launch_settings(&settings, &path, &launch).unwrap());
        assert!(!persist_launch_settings(&settings, &path, &launch).unwrap());

        let stored = settings.read().unwrap();
        assert_eq!(stored.launch.scenario, "spacewars");
        assert_eq!(stored.launch.seed, 123);
        assert_eq!(stored.launch.renderer, RendererSetting::Raster);
        assert_eq!(stored.launch.raster_scale, 2.0);
        assert_eq!(stored.last_scenario.as_deref(), Some("spacewars"));
        drop(stored);

        let reloaded = settings::load_settings(&path).unwrap();
        assert_eq!(reloaded.status, settings::LoadStatus::Existing);
        assert_eq!(reloaded.settings.launch.seed, 123);
        assert_eq!(reloaded.settings.launch.renderer, RendererSetting::Raster);
        assert_eq!(reloaded.settings.launch.raster_scale, 2.0);
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
