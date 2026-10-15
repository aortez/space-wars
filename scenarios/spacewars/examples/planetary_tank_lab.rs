//! Headless producer for the shared planetary tank fixture.
use clap::{Parser, ValueEnum};
use engine_common::DirtMode;
use scenario_spacewars::tank_fixture::{
    FIXTURE_VERSION, FixtureCase, MEASURED_STEPS, TankFixture, WARMUP_STEPS,
};
use sha2::{Digest, Sha256};
use std::{error::Error, fs, path::PathBuf, time::Instant};

#[path = "terrain_soak/svg.rs"]
mod svg;

#[derive(Clone, Copy, ValueEnum)]
enum Shape {
    Round,
    Angular,
}
#[derive(Clone, Copy, ValueEnum)]
enum Case {
    Drive,
    Cut,
}
#[derive(Parser)]
struct Args {
    #[arg(long, value_enum)]
    shape: Shape,
    #[arg(long, value_enum)]
    case: Case,
    #[arg(long,value_parser=clap::value_parser!(u8))]
    tanks: u8,
    #[arg(long)]
    seed: u64,
    #[arg(long)]
    warmup_steps: usize,
    #[arg(long)]
    steps: usize,
    #[arg(long)]
    json: bool,
    /// Optional local trace/draw-list evidence; never part of measured timing.
    #[arg(long)]
    output: Option<PathBuf>,
}
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    if !args.json
        || ![0, 2].contains(&args.tanks)
        || args.steps != MEASURED_STEPS
        || args.warmup_steps != WARMUP_STEPS
    {
        return Err("requires --json, --tanks 0|2, --warmup-steps 120 and --steps 600".into());
    }
    if let Some(path) = &args.output {
        fs::create_dir(path)?;
    }
    let shape = match args.shape {
        Shape::Round => DirtMode::Round,
        Shape::Angular => DirtMode::Angular,
    };
    let case = match args.case {
        Case::Drive => FixtureCase::Drive,
        Case::Cut => FixtureCase::Cut,
    };
    let mut fixture = TankFixture::with_tanks(args.seed, shape, case, args.tanks as usize);
    let mut trace = Vec::with_capacity(args.steps);
    let mut step_us = Vec::with_capacity(args.steps);
    for tick in 0..args.steps {
        fixture.prepare_step();
        let started = Instant::now();
        fixture.step();
        step_us.push(started.elapsed().as_secs_f64() * 1_000_000.0);
        trace.push(fixture.sample());
        if let Some(path) = &args.output {
            if [0, 119, 239, 419, 429, 599].contains(&tick) {
                fs::write(
                    path.join(format!("step-{tick:03}.svg")),
                    svg::svg(&fixture.frame()),
                )?;
            }
        }
    }
    let mut assertions = fixture.assertions(&trace);
    assertions.insert("bounded_lifecycle", fixture.cleanup());
    if let Some(path) = &args.output {
        fs::write(path.join("cleanup.svg"), svg::svg(&fixture.frame()))?;
    }
    let bytes = serde_json::to_vec(&trace)?;
    let replay_sha256 = format!("{:x}", Sha256::digest(&bytes));
    // Rebuild from the seed, independently of the timed world's contact caches.
    let mut replay = TankFixture::with_tanks(args.seed, shape, case, args.tanks as usize);
    let mut repeated = Vec::with_capacity(args.steps);
    for _ in 0..args.steps {
        replay.prepare_step();
        replay.step();
        repeated.push(replay.sample());
    }
    let replay_matches = bytes == serde_json::to_vec(&repeated)?;
    let result = serde_json::json!({
        "schema_version":1,"fixture":FIXTURE_VERSION,
        "shape":match args.shape {Shape::Round=>"round",Shape::Angular=>"angular"},
        "case":match args.case {Case::Drive=>"drive",Case::Cut=>"cut"},
        "tanks":args.tanks,"seed":args.seed,"warmup_steps":args.warmup_steps,"steps":args.steps,
        "replay_matches":replay_matches,"replay_sha256":replay_sha256,
        "step_us":step_us,
        "tank_queries":trace.iter().map(|s|s.queries).collect::<Vec<_>>(),
        "tank_contacts":trace.iter().map(|s|s.contact_traversal).collect::<Vec<_>>(),
        "assertions":assertions,
    });
    if let Some(path) = &args.output {
        fs::write(path.join("trace.json"), bytes)?;
        fs::write(
            path.join("result.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
    }
    println!("{}", serde_json::to_string(&result)?);
    if !replay_matches || assertions.values().any(|&v| !v) {
        return Err("physical fixture assertion or replay failed".into());
    }
    Ok(())
}
