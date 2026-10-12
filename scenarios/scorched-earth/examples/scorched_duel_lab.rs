//! Held manual drive on the ordinary seeded hills, with optional combat.
//! Native contacts and per-second samples are outside any performance timer.
use clap::Parser;
use engine_common::Scenario;
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{
    FIXED_HZ, ScorchedConfig, ScorchedScenario, ScorchedState, WheelTuning,
    driving_lab::{chassis_contacts, physics_snapshot, snapshot, step_trial, wheel_contacts},
};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value = "angular", value_parser = ["round", "angular"])]
    shape: String,
    #[arg(long, default_value_t = WheelTuning::default().motor_torque)]
    torque: f32,
    #[arg(long, default_value_t = WheelTuning::default().motor_damping)]
    motor_damping: f32,
    #[arg(long, default_value_t = 0.65)]
    radius: f32,
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(0..=30))]
    delay: u32,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=120))]
    seconds: u32,
    #[arg(long)]
    quiet: bool,
    #[arg(long)]
    replay: bool,
    /// Release drive for two seconds after crossing each X, then resume uphill.
    #[arg(long, allow_hyphen_values = true)]
    stop_at: Vec<f32>,
    #[arg(long)]
    capture_dir: Option<PathBuf>,
}

fn sample(state: &ScorchedState) -> Value {
    json!({"drive": snapshot(state, 0, 1.0), "contacts": wheel_contacts(state, 0), "chassis_contacts": chassis_contacts(state, 0)})
}

fn run(args: &Args) -> (Value, Vec<u8>) {
    let mut state = ScorchedScenario::init(
        ScorchedConfig {
            shape: if args.shape == "round" {
                GrainShape::Round
            } else {
                GrainShape::Hexagon
            },
            suspension: true,
            wheel_tuning: WheelTuning {
                radius: args.radius,
                motor_torque: args.torque,
                motor_damping: args.motor_damping,
            },
            ..Default::default()
        },
        args.seed,
    );
    if args.quiet {
        scenario_scorched_earth::driving_lab::disable_combat(&mut state);
    }
    for _ in 0..args.delay * FIXED_HZ {
        step_trial(&mut state, 0, Some(0.0));
    }
    let mut trace = vec![sample(&state)];
    let mut stall = None;
    let mut native = Vec::new();
    let mut farthest = snapshot(&state, 0, 1.0).position[0];
    let mut previous = farthest;
    let mut stalled_windows = 0;
    let mut finish_tick = None;
    let mut stops = Vec::new();
    let mut next_stop = 0;
    let mut brake_until = 0;
    for i in 0..args.seconds * FIXED_HZ {
        if next_stop < args.stop_at.len()
            && state.tick >= brake_until
            && snapshot(&state, 0, 1.0).position[0] >= args.stop_at[next_stop]
        {
            stops.push(json!({"start": sample(&state)}));
            next_stop += 1;
            brake_until = state.tick + 2 * u64::from(FIXED_HZ);
        }
        let braking = state.tick < brake_until;
        step_trial(&mut state, 0, Some(if braking { 0.0 } else { 1.0 }));
        if state.tick == brake_until {
            stops.last_mut().unwrap()["end"] = sample(&state);
            stalled_windows = 0;
        }
        let drive = snapshot(&state, 0, 1.0);
        farthest = farthest.max(drive.position[0]);
        if drive.position[0] >= 8.0 && drive.supported && drive.health > 0.0 {
            finish_tick.get_or_insert(state.tick);
        }
        if (i + 1) % FIXED_HZ == 0 {
            state.audit().unwrap();
            let s = sample(&state);
            stalled_windows = if farthest - previous < 0.1 {
                stalled_windows + 1
            } else {
                0
            };
            previous = farthest;
            if !braking
                && finish_tick.is_none()
                && stall.is_none()
                && stalled_windows >= 3
                && drive.health > 0.0
            {
                stall = Some(
                    json!({"sample": s, "observation": serde_json::from_slice::<Value>(&ScorchedScenario::observe(&state).payload).unwrap()}),
                );
                native = physics_snapshot(&state);
            }
            trace.push(s);
        }
    }
    (
        json!({
            "seed": args.seed, "shape": args.shape, "wheel_tuning": state.config.wheel_tuning,
            "quiet": args.quiet, "delay_seconds": args.delay, "seconds": args.seconds,
            "stop_at": args.stop_at, "stops": stops,
            "farthest_x": farthest, "finish_tick": finish_tick, "first_stall": stall, "trace": trace,
            "final_observation_hash": format!("{:016x}", state.observation_hash()), "material": state.audit().unwrap(),
        }),
        native,
    )
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    if !args.torque.is_finite()
        || !(30.0..=480.0).contains(&args.torque)
        || !args.radius.is_finite()
        || !(0.5..=1.0).contains(&args.radius)
        || !args.motor_damping.is_finite()
        || !(1.0..=2400.0).contains(&args.motor_damping)
    {
        return Err("torque must be 30..480, radius 0.5..1.0 and motor damping 1..2400".into());
    }
    if args
        .stop_at
        .iter()
        .any(|x| !x.is_finite() || !(-25.0..8.0).contains(x))
        || args.stop_at.windows(2).any(|w| w[0] >= w[1])
    {
        return Err("stop-at positions must increase and be in -25..8".into());
    }
    let (mut result, native) = run(&args);
    if args.replay {
        assert_eq!((result.clone(), native.clone()), run(&args));
    }
    result["replay_checked"] = json!(args.replay);
    if let Some(path) = &args.capture_dir {
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        std::fs::write(path.join("stall.rapier"), native).map_err(|e| e.to_string())?;
        std::fs::write(
            path.join("report.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    println!("{}", serde_json::to_string(&result).unwrap());
    Ok(())
}
