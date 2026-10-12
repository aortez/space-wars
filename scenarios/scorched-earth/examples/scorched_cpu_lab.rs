//! Ordinary-match review. Trace collection and replay are deliberately untimed.
use clap::Parser;
use engine_common::Scenario;
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{
    BankStrength, FIXED_HZ, ScorchedConfig, ScorchedScenario, ScorchedState,
    driving_lab::{combat_tanks, physics_snapshot},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value = "angular", value_parser = ["round", "angular"])]
    shape: String,
    #[arg(long, default_value = "off", value_parser = ["off", "weak", "strong"])]
    strength: String,
    #[arg(long)]
    rigid: bool,
    #[arg(long)]
    footing: bool,
    /// Capture the first better-footing opportunity with the old CPU, then
    /// compare the selected candidate against the untouched range controller.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=2), conflicts_with = "footing")]
    fork_player: Option<u8>,
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=180))]
    seconds: u32,
    #[arg(long)]
    replay: bool,
    #[arg(long)]
    capture_dir: Option<PathBuf>,
}

fn initial(args: &Args) -> ScorchedState {
    let config = ScorchedConfig {
        shape: if args.shape == "round" {
            GrainShape::Round
        } else {
            GrainShape::Hexagon
        },
        suspension: !args.rigid,
        demo: true,
        cpu_footing: [args.footing; 2],
        ..Default::default()
    }
    .with_destruction(match args.strength.as_str() {
        "weak" => BankStrength::Weak,
        "strong" => BankStrength::Strong,
        _ => BankStrength::Off,
    });
    ScorchedScenario::init(config, args.seed)
}

fn step(state: &mut ScorchedState) {
    ScorchedScenario::step(
        state,
        &[],
        Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ)),
    );
}

fn trial(mut state: ScorchedState, args: &Args) -> Value {
    let initial = combat_tanks(&state);
    let mut trace = Vec::new();
    let mut shots = Vec::new();
    let mut modes: [BTreeMap<&str, u32>; 2] = Default::default();
    let mut first_death = None;
    for _ in 0..args.seconds * FIXED_HZ {
        let before = combat_tanks(&state);
        step(&mut state);
        let after = combat_tanks(&state);
        if first_death.is_none() {
            for player in 0..2 {
                *modes[player]
                    .entry(after[player].drive.driving.mode)
                    .or_default() += 1;
            }
        }
        for player in 0..2 {
            if after[player].own_shots > before[player].own_shots {
                shots.push(json!({"tick": state.tick-1, "player":player+1,"before": before[player], "after": after[player]}));
            }
        }
        if after.iter().any(|t| t.drive.health <= 0.0) {
            first_death.get_or_insert(state.tick);
        }
        if state.tick.is_multiple_of(u64::from(FIXED_HZ)) {
            state.audit().unwrap();
            trace.push(json!({"tick":state.tick,"tanks":after,"impacts":state.impacts}));
        }
    }
    json!({"seed":args.seed,"shape":args.shape,"strength":args.strength,"suspension":!args.rigid,
        "cpu_footing":state.config.cpu_footing,"initial_tanks":initial,
        "seconds":args.seconds,"mode_ticks_while_fighting":modes,"first_death":first_death,
        "shots":shots,"trace":trace,"final_observation_hash":format!("{:016x}",state.observation_hash()),
        "material":state.audit().unwrap()})
}

fn run(args: &Args) -> Result<(Value, Vec<u8>), String> {
    let mut state = initial(args);
    let Some(player) = args.fork_player.map(|p| usize::from(p - 1)) else {
        return Ok((trial(state, args), Vec::new()));
    };
    for _ in 0..60 * FIXED_HZ {
        if combat_tanks(&state)[player].footing_candidate.is_some() {
            break;
        }
        step(&mut state);
    }
    if combat_tanks(&state)[player].footing_candidate.is_none() {
        return Err("no better-footing opportunity in the first 60 seconds".into());
    }
    let mut candidate = state.clone();
    assert_eq!(state.observation_hash(), candidate.observation_hash());
    let native = physics_snapshot(&state);
    assert_eq!(native, physics_snapshot(&candidate));
    candidate.config.cpu_footing[player] = true;
    let report = json!({"seed":args.seed,"shape":args.shape,"player":player+1,
        "capture_tick":state.tick,"initial_tanks":combat_tanks(&state),
        "initial_observation":serde_json::from_slice::<Value>(&ScorchedScenario::observe(&state).payload).unwrap(),
        "initial_observation_hash":format!("{:016x}",state.observation_hash()),
        "reference":trial(state,args),"candidate":trial(candidate,args),
        "notes":["Both continuations clone the same native world, controllers and shot schedule; only the selected tank enables footing goals.",
        "The opponent retains the range-only controller. No body, velocity, contact, terrain, motor, aim or retry history is reset."]});
    Ok((report, native))
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let (mut result, native) = run(&args)?;
    if args.replay {
        assert_eq!((result.clone(), native.clone()), run(&args)?);
    }
    result["replay_checked"] = json!(args.replay);
    if let Some(path) = &args.capture_dir {
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        if !native.is_empty() {
            std::fs::write(path.join("fork.rapier"), native).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            path.join("report.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    println!("{}", serde_json::to_string(&result).unwrap());
    Ok(())
}
