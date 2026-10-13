//! Observed-shot comparisons; no hidden trial trajectories or reset between shots.
use clap::Parser;
use engine_common::Scenario;
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{
    CpuPolicy, FIXED_HZ, ScorchedConfig, ScorchedScenario, ScorchedState,
    bot_lab::{AimCase, stationary_case},
};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "flat", value_parser = ["flat", "uphill", "downhill", "ridge", "duel"])]
    case: String,
    #[arg(long, default_value = "calibrated", value_parser = ["sweep", "calibrated", "reactive"])]
    bot: String,
    #[arg(long, default_value = "angular", value_parser = ["round", "angular"])]
    shape: String,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value_t = 45, value_parser = clap::value_parser!(u32).range(1..=120))]
    seconds: u32,
    #[arg(long)]
    replay: bool,
}

fn run(args: &Args) -> Value {
    let policy = match args.bot.as_str() {
        "reactive" => CpuPolicy::Reactive,
        "calibrated" => CpuPolicy::Calibrated,
        _ => CpuPolicy::Sweep,
    };
    let shape = if args.shape == "round" {
        GrainShape::Round
    } else {
        GrainShape::Hexagon
    };
    let mut state = if args.case == "duel" {
        ScorchedState::new(
            ScorchedConfig {
                shape,
                demo: true,
                suspension: true,
                cpu_footing: [true; 2],
                cpu_policy: [policy; 2],
                ..Default::default()
            },
            args.seed,
        )
    } else {
        stationary_case(
            shape,
            match args.case.as_str() {
                "uphill" => AimCase::Uphill,
                "downhill" => AimCase::Downhill,
                "ridge" => AimCase::Ridge,
                _ => AimCase::Flat,
            },
            policy,
        )
    };
    let initial = state.observation_hash();
    let mut trace = Vec::new();
    for i in 0..args.seconds * FIXED_HZ {
        ScorchedScenario::step(
            &mut state,
            &[],
            Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ)),
        );
        if i % FIXED_HZ == 0 {
            state.audit().unwrap();
            trace.push(json!({"tick":state.tick,"tanks":scenario_scorched_earth::driving_lab::combat_tanks(&state),"bots":state.cpu_diagnostics()}));
        }
    }
    json!({"case":args.case,"seed":if args.case == "duel" {args.seed} else {42},"shape":args.shape,"bot":args.bot,
        "seconds":args.seconds,"initial_hash":format!("{initial:016x}"),"final_hash":format!("{:016x}",state.observation_hash()),
        "shots":state.shot_observations(),"bots":state.cpu_diagnostics(),"trace":trace,"material":state.audit().unwrap()})
}

fn main() {
    let args = Args::parse();
    let mut report = run(&args);
    if args.replay {
        assert_eq!(report, run(&args));
    }
    report["replay_checked"] = json!(args.replay);
    println!("{}", report);
}
