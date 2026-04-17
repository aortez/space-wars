//! Spacewars client: Slint UI + custom drawing + input + audio + scenario host.
//!
//! Stub for M3. Real work lands in subsequent milestones:
//!   M4: Slint window.
//!   M6: settings load, crash-behavior panic handler.
//!   M7: scenario host loop driving scenarios/spacewars.

use clap::Parser;

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

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    tracing::info!(
        scenario = %args.scenario,
        dev = args.dev,
        "engine-client starting (stub)."
    );
}
