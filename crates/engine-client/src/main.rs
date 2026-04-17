//! Spacewars client: Slint UI + custom drawing + input + audio + scenario host.
//!
//! M6 state: opens an empty Slint window, loads user settings from disk, and
//! saves them back on exit. Custom drawing, scenario hosting, and crash
//! handling arrive in subsequent milestones.

use std::sync::{Arc, RwLock};

use clap::Parser;
use engine_common::{CrashBehavior, Settings};

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
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let settings_path = engine_common::settings_path()?;
    let loaded = Settings::load()?;
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
        w.last_scenario = Some(args.scenario.clone());
        w.save()?;
        tracing::info!(path = %settings_path.display(), "saved settings.");
    }

    let window = MainWindow::new()?;
    window.run()?;
    Ok(())
}
