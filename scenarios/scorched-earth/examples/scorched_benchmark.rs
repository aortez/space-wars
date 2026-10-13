//! Whole-scene timing; auditing, observations and replay are outside the timer.
use clap::{Parser, ValueEnum};
use engine_common::Scenario;
use engine_rapier::terrain::{GrainShape, SettlingWork};
use scenario_scorched_earth::{
    BankStrength, CpuPolicy, DropHeight, FIXED_HZ, ScorchedConfig, ScorchedScenario, WheelTuning,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, ValueEnum)]
enum Shape {
    Round,
    Angular,
}
#[derive(Clone, Copy, ValueEnum)]
enum Cohesion {
    Off,
    Weak,
    Strong,
}
#[derive(Clone, Copy, ValueEnum)]
enum Drop {
    Low,
    High,
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
    /// Compare moving CPU tanks; omitted retains the original stationary duel.
    #[arg(long)]
    cpu_driving: bool,
    /// Enable nearby firing-footing goals for the moving CPU comparison.
    #[arg(long, requires = "cpu_driving")]
    cpu_footing: bool,
    /// Retain the sweep, learn from shots, or also relocate after incoming hits.
    #[arg(long, default_value = "sweep", value_parser = ["sweep", "calibrated", "reactive"])]
    bot: String,
    /// Use larger physical wheels with native spring/damper suspension.
    #[arg(long)]
    suspension: bool,
    /// Fix drive torque explicitly when comparing old and new wheel settings.
    #[arg(long, requires = "suspension")]
    wheel_torque: Option<f32>,
    /// Fix motor response; 80 reproduces retained wheel benchmark settings.
    #[arg(long, requires = "suspension")]
    wheel_damping: Option<f32>,
    /// Rain shells independently of tank health, then measure a quiet tail.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=600))]
    bombardment_seconds: u32,
    /// Emit one audited sample per second to stderr, outside the timer.
    #[arg(long)]
    diagnostics: bool,
    /// Emit inclusive shared-stage timings for each tick to stderr.
    #[arg(long)]
    profile: bool,
    #[arg(long)]
    slumping: bool,
    #[arg(long, value_enum, default_value = "off")]
    cohesion: Cohesion,
    #[arg(long)]
    undercut: bool,
    #[arg(long)]
    hillside: bool,
    #[arg(long, conflicts_with_all = ["hillside", "undercut", "drop"])]
    roof: bool,
    #[arg(long, requires = "roof")]
    require_roof: bool,
    /// Fail if this run misses bank failure, impact chipping or deposition.
    #[arg(long, requires = "hillside")]
    require_lifecycle: bool,
    #[arg(long)]
    breakup: bool,
    #[arg(long, value_enum)]
    drop: Option<Drop>,
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
fn add_work(sum: &mut SettlingWork, work: SettlingWork) {
    sum.attempted_groups += work.attempted_groups;
    sum.attempted_grains += work.attempted_grains;
    sum.reused_failures += work.reused_failures;
}
fn main() {
    let args = Args::parse();
    assert!(
        args.wheel_torque
            .is_none_or(|v| v.is_finite() && (30.0..=480.0).contains(&v)),
        "wheel torque must be finite and in 30..=480"
    );
    assert!(
        args.wheel_damping
            .is_none_or(|v| v.is_finite() && (1.0..=2400.0).contains(&v)),
        "motor damping must be finite and in 1..=2400"
    );
    assert!(
        args.bombardment_seconds <= args.seconds,
        "bombardment must fit inside --seconds"
    );
    let config = ScorchedConfig {
        demo: true,
        cpu_driving: args.cpu_driving,
        cpu_footing: [args.cpu_footing; 2],
        cpu_policy: [match args.bot.as_str() {
            "calibrated" => CpuPolicy::Calibrated,
            "reactive" => CpuPolicy::Reactive,
            _ => CpuPolicy::Sweep,
        }; 2],
        suspension: args.suspension,
        wheel_tuning: WheelTuning {
            motor_torque: args
                .wheel_torque
                .unwrap_or(WheelTuning::default().motor_torque),
            motor_damping: args
                .wheel_damping
                .unwrap_or(WheelTuning::default().motor_damping),
            ..Default::default()
        },
        course: None,
        course_goal_driving: false,
        undercut: args.undercut,
        hillside: args.hillside,
        roof: args.roof,
        breakup: args.breakup,
        drop_height: args.drop.map(|h| match h {
            Drop::Low => DropHeight::Low,
            Drop::High => DropHeight::High,
        }),
        cohesion: match args.cohesion {
            Cohesion::Off => BankStrength::Off,
            Cohesion::Weak => BankStrength::Weak,
            Cohesion::Strong => BankStrength::Strong,
        },
        slumping: args.slumping,
        bombardment_seconds: args.bombardment_seconds,
        max_grains: args.limit as usize,
        shape: match args.shape {
            Shape::Round => GrainShape::Round,
            Shape::Angular => GrainShape::Hexagon,
        },
    };
    let config = if args.hillside || args.roof {
        config.with_destruction(config.cohesion)
    } else {
        config
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
    let mut packing_work = SettlingWork::default();
    let mut tail_work = SettlingWork::default();
    engine_rapier::profile::set_enabled(args.profile);
    for tick in 0..ticks {
        let started = Instant::now();
        ScorchedScenario::step(&mut state, &[], dt);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        timings.push(elapsed);
        if args.profile {
            let physics = state.last_physics;
            eprintln!(
                "{}",
                serde_json::json!({
                    "tick": tick + 1, "total_ms": elapsed,
                    "profile": engine_rapier::profile::take(),
                    "rapier_ms": physics.rapier_step_time.as_secs_f64() * 1000.0,
                    "broad_phase_ms": physics.broad_phase_time.as_secs_f64() * 1000.0,
                    "narrow_phase_ms": physics.narrow_phase_time.as_secs_f64() * 1000.0,
                    "solver_ms": physics.solver_time.as_secs_f64() * 1000.0,
                    "loose": state.loose_cells(), "impacts": state.impacts,
                    "rejected": state.rejected_blasts,
                })
            );
        }
        add_work(&mut packing_work, state.settling_work());
        let firing = if args.roof {
            tick < 5 * FIXED_HZ
        } else if args.hillside {
            tick < 20 * FIXED_HZ
        } else {
            args.bombardment_seconds == 0 || tick < args.bombardment_seconds * FIXED_HZ
        };
        if firing {
            active.push(elapsed);
        } else {
            tail.push(elapsed);
            add_work(&mut tail_work, state.settling_work());
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
                        "available_slots": state.config.max_grains - state.loose_cells(),
                        "last_capacity_rejection": state.last_capacity_rejection,
                        "packing_work": packing_work,
                        "slumping": state.slumping_diagnostics(),
                        "cohesion": state.cohesion_diagnostics(),
                        "impact_fracture": state.impact_diagnostics(),
                        "observation_hash": format!("{:016x}", state.observation_hash()),
                        "driving": state.driving_diagnostics(),
                    })
                );
            }
        }
    }
    engine_rapier::profile::set_enabled(false);
    if args.require_roof {
        assert_eq!(state.shots, 1, "one ordinary tank shot");
        assert!(
            state.cohesion_diagnostics().roof_failures > 0,
            "roof must crack"
        );
        assert!(
            state.cohesion_diagnostics().failures > state.cohesion_diagnostics().roof_failures,
            "remaining ledge must fail"
        );
        assert!(state.impact_diagnostics().failures > 0, "clump must chip");
        assert!(state.deposited_cells() > 0, "some dirt must return");
    }
    if args.require_lifecycle {
        assert_eq!(state.shots, 3, "three ordinary tank shots");
        assert!(state.cohesion_diagnostics().failures > 0, "bank must fail");
        assert!(state.impact_diagnostics().failures > 0, "clump must chip");
        assert!(state.deposited_cells() > 0, "some dirt must return");
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
    let mut report = serde_json::json!({
        "workload": if args.roof { "scorched-earth-roof-v1" } else if args.hillside { "scorched-earth-hillside-v2" } else if args.drop.is_some() { "scorched-earth-drop-v1" } else if args.undercut { "scorched-earth-undercut-v1" } else if args.bombardment_seconds > 0 { "scorched-earth-bombardment-v1" } else if args.suspension { "scorched-earth-wheeled-duel-v1" } else if args.cpu_driving { "scorched-earth-driving-duel-v1" } else { "scorched-earth-scripted-duel-v2" },
        "cpu_driving": args.cpu_driving, "cpu_footing": args.cpu_footing,
        "driving": state.driving_diagnostics(),
        "footing": args.cpu_footing.then(|| scenario_scorched_earth::driving_lab::combat_tanks(&state).map(|tank| tank.controller["footing"].clone())),
        "suspension": args.suspension,
        "wheel_tuning": args.suspension.then_some(config.wheel_tuning),
        "slumping_enabled": state.config.slumping,
        "cohesion_strength": state.config.cohesion.label(),
        "drop": state.config.drop_height.map(DropHeight::label), "breakup": state.config.breakup,
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
        "available_slots": state.config.max_grains - state.loose_cells(),
        "last_capacity_rejection": state.last_capacity_rejection,
        "packing_work": packing_work, "tail_packing_work": tail_work,
        "slumping": state.slumping_diagnostics(),
                    "cohesion": state.cohesion_diagnostics(),
                    "impact_fracture": state.impact_diagnostics(),
    });
    report["bot"] = serde_json::json!({"policy": args.bot, "controllers": state.cpu_diagnostics(), "shots": state.shot_observations()});
    println!("{}", report);
}
