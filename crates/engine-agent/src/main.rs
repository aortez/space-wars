//! Spacewars agent: training entry point. Embeds scenarios directly for speed.
//!
//! Stub for M3.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "engine-agent", about = "Spacewars agent training entry point")]
struct Args {
    /// Scenario to train against.
    #[arg(long, default_value = "null")]
    scenario: String,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    tracing::info!(scenario = %args.scenario, "engine-agent starting (stub).");
}
