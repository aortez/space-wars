//! Whole-scene timing; auditing, observations and replay are outside the timer.
use clap::{Parser, ValueEnum};
use engine_common::Scenario;
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{FIXED_HZ, ScorchedConfig, ScorchedScenario};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, ValueEnum)]
enum Shape {
    Round,
    Angular,
}
#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=600))]
    seconds: u32,
    #[arg(long, value_enum, default_value = "angular")]
    shape: Shape,
    #[arg(long, default_value_t = 192, value_parser = clap::value_parser!(u32).range(1..=512))]
    limit: u32,
    #[arg(long)]
    replay: bool,
}
fn main() {
    let args = Args::parse();
    let config = ScorchedConfig {
        demo: true,
        max_grains: args.limit as usize,
        shape: match args.shape {
            Shape::Round => GrainShape::Round,
            Shape::Angular => GrainShape::Hexagon,
        },
    };
    let mut state = ScorchedScenario::init(config, args.seed);
    let dt = Duration::from_secs_f64(1.0 / FIXED_HZ as f64);
    let ticks = args.seconds * FIXED_HZ;
    let mut timings = Vec::new();
    let mut peak_bodies = 0;
    let mut peak_loose = 0;
    let mut peak_contacts = 0;
    let mut checkpoints = Vec::new();
    for tick in 0..ticks {
        let started = Instant::now();
        ScorchedScenario::step(&mut state, &[], dt);
        timings.push(started.elapsed().as_secs_f64() * 1000.0);
        peak_bodies = peak_bodies.max(state.body_count());
        peak_loose = peak_loose.max(state.loose_cells());
        peak_contacts = peak_contacts.max(state.last_physics.contact_pairs);
        if (tick + 1) % FIXED_HZ == 0 {
            state
                .audit()
                .expect("material conservation and finite motion");
            checkpoints.push(state.observation_hash());
        }
    }
    if args.replay {
        let mut replay = ScorchedScenario::init(config, args.seed);
        for tick in 0..ticks {
            ScorchedScenario::step(&mut replay, &[], dt);
            if (tick + 1) % FIXED_HZ == 0 {
                assert_eq!(
                    replay.observation_hash(),
                    checkpoints[(tick / FIXED_HZ) as usize],
                    "replay at tick {}",
                    tick + 1
                );
            }
        }
        replay.audit().unwrap();
    }
    timings.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({
            "workload": "scorched-earth-scripted-duel-v1", "seed": args.seed, "ticks": ticks,
            "shape": match args.shape { Shape::Round => "round", Shape::Angular => "angular" },
            "grain_limit": args.limit, "shots": state.shots, "impacts": state.impacts,
            "rejected_blasts": state.rejected_blasts, "deposited_cells": state.deposited_cells(),
            "peak_loose": peak_loose, "peak_bodies": peak_bodies, "peak_contacts": peak_contacts,
            "mean_ms": timings.iter().sum::<f64>() / timings.len() as f64,
            "p95_ms": timings[(timings.len() * 95 / 100).min(timings.len() - 1)],
            "max_ms": timings.last(), "material": state.audit().unwrap(),
            "observation_hash": format!("{:016x}", state.observation_hash()), "replay_checked": args.replay,
            "winner": state.winner().map(|p| p + 1),
        })
    );
}
