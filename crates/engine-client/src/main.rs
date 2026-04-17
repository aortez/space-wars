//! Spacewars client: Slint UI + custom drawing + input + audio + scenario host.
//!
//! M6 state: opens an empty Slint window, loads user settings from disk, and
//! saves them back on exit. Custom drawing, scenario hosting, and crash
//! handling arrive in subsequent milestones.

mod settings;

use std::env;
use std::path::Path;
use std::sync::{Arc, RwLock};

use clap::Parser;
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

    /// Force CrashBehavior::Freeze for this run without writing settings.
    #[arg(long)]
    dev: bool,
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
        scenario = %args.scenario,
        dev = args.dev,
        crash_behavior = ?effective_crash_behavior,
        "engine-client starting."
    );

    let settings = Arc::new(RwLock::new(loaded));
    {
        let mut w = settings.write().unwrap();
        let scenario_changed = w.last_scenario.as_deref() != Some(args.scenario.as_str());
        if scenario_changed {
            w.last_scenario = Some(args.scenario.clone());
        }
        if needs_writeback || scenario_changed {
            settings::save_settings(&w, &settings_path)?;
            tracing::info!(path = %settings_path.display(), "saved settings.");
        }
    }

    let window = MainWindow::new()?;
    window.run()?;
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

    tracing_subscriber::fmt().with_env_filter(filter).init();
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
