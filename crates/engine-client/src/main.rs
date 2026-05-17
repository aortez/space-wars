//! Spacewars client: Slint UI + custom drawing + input + audio + scenario host.
//!
//! M9 state: opens a Slint window, loads/saves user settings, hosts null or
//! Spacewars scenarios, and renders their `RenderFrame`s.

mod host;
mod input;
mod raster;
mod render;
mod settings;

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use clap::{Parser, ValueEnum};
use engine_common::{CrashBehavior, Settings};
use settings::LoadStatus;
use tracing_subscriber::EnvFilter;

slint::include_modules!();

#[derive(Parser, Debug)]
#[command(name = "engine-client", about = "Spacewars scenario host")]
struct Args {
    /// Scenario to load.
    #[arg(long, default_value = "null")]
    scenario: String,

    /// Scenario seed.
    #[arg(long, default_value_t = 0)]
    seed: u64,

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
    #[arg(long, value_enum, default_value_t = RendererArg::Vector)]
    renderer: RendererArg,

    /// Internal resolution scale for --renderer raster.
    #[arg(long, default_value_t = 1.0)]
    raster_scale: f32,
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

impl Args {
    fn uses_debug_render(&self) -> bool {
        self.debug_render || self.debug_triangles != 0
    }

    fn uses_benchmark(&self) -> bool {
        self.benchmark || self.benchmark_headless
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let scenario_name = if args.uses_benchmark() && args.scenario == "null" {
        "spacewars".to_string()
    } else {
        args.scenario.clone()
    };

    let settings_path = settings::settings_path()?;
    let loaded_settings = settings::load_settings(&settings_path)?;
    let mut loaded = loaded_settings.settings;
    let mut needs_writeback = loaded_settings.status.needs_writeback();
    needs_writeback |= normalize_log_level(&mut loaded);

    init_tracing(&loaded);
    log_settings_load_status(&settings_path, &loaded_settings.status);
    if !args.uses_debug_render() {
        host::validate_scenario(&scenario_name)?;
    }
    if args.uses_benchmark() && scenario_name != "spacewars" {
        return Err("benchmark mode currently supports only the spacewars scenario".into());
    }
    tracing::info!(
        path = %settings_path.display(),
        last_scenario = ?loaded.last_scenario,
        crash_behavior = ?loaded.runtime.crash_behavior,
        "loaded settings."
    );

    let effective_crash_behavior = if args.dev {
        CrashBehavior::Freeze
    } else {
        loaded.runtime.crash_behavior
    };
    tracing::info!(
        scenario = %scenario_name,
        seed = args.seed,
        dev = args.dev,
        crash_behavior = ?effective_crash_behavior,
        "engine-client starting."
    );

    if args.benchmark_headless {
        host::run_spacewars_benchmark(host::BenchmarkOptions {
            seed: args.seed,
            seconds: args.benchmark_seconds,
            report_path: args.benchmark_report.clone(),
            renderer: args.renderer.into(),
            raster_scale: args.raster_scale,
        })?;
        return Ok(());
    }

    let settings = Arc::new(RwLock::new(loaded));
    {
        let mut w = settings.write().unwrap();
        let scenario_changed =
            !args.uses_debug_render() && w.last_scenario.as_deref() != Some(scenario_name.as_str());
        if scenario_changed {
            w.last_scenario = Some(scenario_name.clone());
        }
        if needs_writeback || scenario_changed {
            settings::save_settings(&w, &settings_path)?;
            tracing::info!(path = %settings_path.display(), "saved settings.");
        }
    }

    select_slint_backend()?;
    let window = MainWindow::new()?;
    let _render_timer = if args.uses_debug_render() {
        host::start_debug_render_loop(&window, args.debug_triangles, args.renderer.into())
    } else {
        host::start_scenario_loop(
            &window,
            &scenario_name,
            args.seed,
            host::ScenarioLoopOptions {
                start_benchmark: args.benchmark,
                renderer: args.renderer.into(),
                raster_scale: args.raster_scale,
            },
        )?
    };
    window.run()?;
    Ok(())
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
