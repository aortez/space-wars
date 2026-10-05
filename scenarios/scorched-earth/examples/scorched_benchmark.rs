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
    /// Rain shells independently of tank health, then measure a quiet tail.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=600))]
    bombardment_seconds: u32,
    /// Emit one audited sample per second to stderr, outside the timer.
    #[arg(long)]
    diagnostics: bool,
    #[arg(long)]
    slumping: bool,
    /// Save frozen packing inputs after the run, outside all timers.
    #[arg(long)]
    packing_dir: Option<std::path::PathBuf>,
}

fn timing(values: &[f64]) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    serde_json::json!({
        "ticks": values.len(),
        "mean_ms": values.iter().sum::<f64>() / values.len() as f64,
        "p95_ms": values[(values.len() * 95 / 100).min(values.len() - 1)],
        "max_ms": values.last(),
    })
}
fn main() {
    let args = Args::parse();
    assert!(
        args.bombardment_seconds <= args.seconds,
        "bombardment must fit inside --seconds"
    );
    let config = ScorchedConfig {
        demo: true,
        slumping: args.slumping,
        bombardment_seconds: args.bombardment_seconds,
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
    let mut active = Vec::new();
    let mut tail = Vec::new();
    let mut peak_bodies = 0;
    let mut peak_loose = 0;
    let mut peak_contacts = 0;
    let mut checkpoints = Vec::new();
    for tick in 0..ticks {
        let started = Instant::now();
        ScorchedScenario::step(&mut state, &[], dt);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        timings.push(elapsed);
        let firing = args.bombardment_seconds == 0 || tick < args.bombardment_seconds * FIXED_HZ;
        if firing {
            active.push(elapsed);
        } else {
            tail.push(elapsed);
        }
        peak_bodies = peak_bodies.max(state.body_count());
        peak_loose = peak_loose.max(state.loose_cells());
        peak_contacts = peak_contacts.max(state.last_physics.contact_pairs);
        if (tick + 1) % FIXED_HZ == 0 {
            state
                .audit()
                .expect("material conservation and finite motion");
            checkpoints.push(state.observation_hash());
            if args.diagnostics {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "tick": state.tick, "phase": if firing { "active" } else { "tail" },
                        "loose": state.loose_cells(), "deposited": state.deposited_cells(),
                        "shots": state.shots, "impacts": state.impacts, "rejected": state.rejected_blasts,
                        "settling": state.settling_diagnostics(),
                        "slumping": state.slumping_diagnostics(),
                        "observation_hash": format!("{:016x}", state.observation_hash()),
                    })
                );
            }
        }
    }
    if let Some(directory) = &args.packing_dir {
        std::fs::create_dir_all(directory).unwrap();
        let before = state.observation_hash();
        for (i, snapshot) in state.capture_packing().unwrap().iter().enumerate() {
            std::fs::write(
                directory.join(format!("case-{i}.packing")),
                snapshot.to_bytes().unwrap(),
            )
            .unwrap();
            std::fs::write(
                directory.join(format!("case-{i}.json")),
                serde_json::to_vec_pretty(&snapshot.inspect().unwrap()).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(before, state.observation_hash(), "inspection changed state");
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
            "workload": if args.bombardment_seconds > 0 { "scorched-earth-bombardment-v1" } else { "scorched-earth-scripted-duel-v2" },
            "slumping_enabled": args.slumping,
            "seed": args.seed, "ticks": ticks, "bombardment_seconds": args.bombardment_seconds,
            "shape": match args.shape { Shape::Round => "round", Shape::Angular => "angular" },
            "grain_limit": args.limit, "shots": state.shots, "impacts": state.impacts,
            "rejected_blasts": state.rejected_blasts, "deposited_cells": state.deposited_cells(),
            "peak_loose": peak_loose, "peak_bodies": peak_bodies, "peak_contacts": peak_contacts,
            "mean_ms": timings.iter().sum::<f64>() / timings.len() as f64,
            "p95_ms": timings[(timings.len() * 95 / 100).min(timings.len() - 1)],
            "max_ms": timings.last(), "material": state.audit().unwrap(),
            "observation_hash": format!("{:016x}", state.observation_hash()), "replay_checked": args.replay,
            "winner": state.winner().map(|p| p + 1),
            "phases": { "active": timing(&active), "tail": timing(&tail) },
            "settling": state.settling_diagnostics(),
            "slumping": state.slumping_diagnostics(),
        })
    );
}
