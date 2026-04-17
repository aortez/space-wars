//! Spacewars client: Slint UI + custom drawing + input + audio + scenario host.
//!
//! M4 state: opens an empty Slint window on Linux desktop. Custom drawing,
//! scenario hosting, settings, and crash handling arrive in subsequent
//! milestones.

use clap::Parser;

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

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    tracing::info!(
        scenario = %args.scenario,
        dev = args.dev,
        "engine-client starting."
    );

    let window = MainWindow::new()?;
    window.run()?;
    Ok(())
}
