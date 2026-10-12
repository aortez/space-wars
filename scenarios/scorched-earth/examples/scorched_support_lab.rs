//! Continue the first false support refusal with identical physics and goal
//! history. All inspection and auditing is untimed.
use clap::Parser;
use engine_common::Scenario;
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{
    DrivingCourse, FIXED_HZ, ScorchedConfig, ScorchedScenario, ScorchedState, WheelTuning,
    driving_lab::{
        SupportCheck, TravelGoal, physics_snapshot, snapshot, step_support_trial, wheel_contacts,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value = "angular", value_parser = ["round", "angular"])]
    shape: String,
    #[arg(long, default_value = "slope", value_parser = ["slope", "rubble"])]
    course: String,
    #[arg(long, default_value_t = 90.0)]
    wheel_torque: f32,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=60))]
    seconds: u32,
    #[arg(long)]
    replay: bool,
    /// Also compare the new goal policy on corrected support, with retained history.
    #[arg(long)]
    compare_goal: bool,
    /// Capture the old goal's first mechanical stall on corrected support.
    #[arg(long, requires = "compare_goal")]
    stalled_prefix: bool,
    #[arg(long)]
    capture_dir: Option<PathBuf>,
}

fn capture(args: &Args) -> Result<(ScorchedState, TravelGoal), String> {
    let course = if args.course == "slope" {
        DrivingCourse::Slope
    } else {
        DrivingCourse::Rubble
    };
    let mut state = ScorchedScenario::init(
        ScorchedConfig {
            shape: if args.shape == "round" {
                GrainShape::Round
            } else {
                GrainShape::Hexagon
            },
            course: Some(course),
            suspension: true,
            demo: true,
            wheel_tuning: WheelTuning {
                motor_torque: args.wheel_torque,
                ..WheelTuning::REFERENCE
            },
            ..Default::default()
        },
        args.seed,
    );
    for _ in 0..120 {
        step_support_trial(&mut state, 0, Some(0.0), SupportCheck::Reference);
    }
    let mut goal = TravelGoal::new(course.finish_x(), 2.5);
    for _ in 0..30 * FIXED_HZ {
        if !args.stalled_prefix
            && !SupportCheck::Reference.supported(&state, 0)
            && SupportCheck::Current.supported(&state, 0)
        {
            state.audit().map_err(|e| e.to_string())?;
            return Ok((state, goal));
        }
        let support = if args.stalled_prefix {
            SupportCheck::Current
        } else {
            SupportCheck::Reference
        };
        let decision = goal.decide_reference(&state, 0, support);
        if args.stalled_prefix && decision.mode == "cooldown" {
            state.audit().map_err(|e| e.to_string())?;
            return Ok((state, goal));
        }
        step_support_trial(&mut state, 0, Some(decision.axis), support);
    }
    Err("requested failure was not reached within 30 seconds".into())
}

fn sample(state: &ScorchedState) -> Value {
    json!({
        "drive": snapshot(state, 0, 1.0),
        "reference_supported": SupportCheck::Reference.supported(state, 0),
        "contacts": wheel_contacts(state, 0),
    })
}

fn trial(
    start: &ScorchedState,
    goal: &TravelGoal,
    support: SupportCheck,
    axis: Option<f32>,
    ticks: u32,
    improved_goal: bool,
) -> Value {
    let mut state = start.clone();
    let mut goal = goal.clone();
    assert_eq!(physics_snapshot(&state), physics_snapshot(start));
    assert_eq!(state.observation_hash(), start.observation_hash());
    let initial_x = snapshot(start, 0, 1.0).position[0];
    let mut trace = vec![sample(&state)];
    let mut decisions = Vec::new();
    let mut modes = BTreeMap::new();
    let mut unsupported_ticks = 0;
    let mut max_pitch = 0.0_f32;
    let mut finish_tick = None;
    let mut arrival_tick = None;
    for i in 0..ticks {
        let decision = axis.is_none().then(|| {
            if improved_goal {
                goal.decide_with_support(&state, 0, support)
            } else {
                goal.decide_reference(&state, 0, support)
            }
        });
        if let Some(d) = &decision {
            *modes.entry(d.mode).or_insert(0_u32) += 1;
            if d.mode == "arrived" {
                arrival_tick.get_or_insert(state.tick);
            }
        }
        step_support_trial(
            &mut state,
            0,
            Some(axis.unwrap_or_else(|| decision.as_ref().unwrap().axis)),
            support,
        );
        let drive = snapshot(&state, 0, 1.0);
        unsupported_ticks += u32::from(!support.supported(&state, 0));
        max_pitch = max_pitch.max(drive.pitch_degrees.abs());
        if drive.position[0] >= state.config.course.unwrap().finish_x()
            && drive.supported
            && drive.health > 0.0
        {
            finish_tick.get_or_insert(state.tick);
        }
        if i < 8 || (i + 1) % FIXED_HZ == 0 {
            state.audit().expect("conserved material and finite motion");
            trace.push(sample(&state));
            decisions
                .push(json!({"tick": state.tick - 1, "decision": decision, "controller": goal}));
        }
    }
    json!({
        "support": support, "axis_override": axis, "ticks": ticks, "improved_goal": improved_goal,
        "net_progress": snapshot(&state, 0, 1.0).position[0] - initial_x,
        "finish_tick": finish_tick, "arrival_tick": arrival_tick,
        "unsupported_ticks": unsupported_ticks, "max_pitch_degrees": max_pitch,
        "mode_ticks": modes, "controller": goal, "trace": trace, "decisions": decisions,
        "final_observation_hash": format!("{:016x}", state.observation_hash()),
        "material": state.audit().unwrap(),
    })
}

fn comparisons(state: &ScorchedState, goal: &TravelGoal, ticks: u32, compare_goal: bool) -> Value {
    let mut result = serde_json::Map::new();
    for (name, axis) in [("goal", None), ("held", Some(1.0)), ("neutral", Some(0.0))] {
        for (label, support) in [
            ("reference", SupportCheck::Reference),
            ("candidate", SupportCheck::Current),
        ] {
            result.insert(
                format!("{label}_{name}"),
                trial(state, goal, support, axis, ticks, false),
            );
        }
    }
    if compare_goal {
        result.insert(
            "improved_goal".into(),
            trial(state, goal, SupportCheck::Current, None, ticks, true),
        );
    }
    result.into()
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    if !args.wheel_torque.is_finite() || !(30.0..=480.0).contains(&args.wheel_torque) {
        return Err("wheel torque must be finite and in 30..=480".into());
    }
    let (state, goal) = capture(&args)?;
    let trials = comparisons(&state, &goal, args.seconds * FIXED_HZ, args.compare_goal);
    let native = physics_snapshot(&state);
    if args.replay {
        let (repeat, repeat_goal) = capture(&args)?;
        assert_eq!(state.observation_hash(), repeat.observation_hash());
        assert_eq!(native, physics_snapshot(&repeat));
        assert_eq!(json!(goal), json!(repeat_goal));
        assert_eq!(
            trials,
            comparisons(
                &repeat,
                &repeat_goal,
                args.seconds * FIXED_HZ,
                args.compare_goal
            )
        );
    }
    let native_hash = native.iter().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    let observation: Value =
        serde_json::from_slice(&ScorchedScenario::observe(&state).payload).unwrap();
    let report = json!({
        "workload": "scorched-wheel-support-v1", "seed": args.seed, "shape": args.shape, "course": args.course,
        "wheel_tuning": state.config.wheel_tuning, "capture_tick": state.tick,
        "stalled_prefix": args.stalled_prefix, "compare_goal": args.compare_goal,
        "native_snapshot_hash": format!("{native_hash:016x}"),
        "initial_observation_hash": format!("{:016x}", state.observation_hash()),
        "initial_observation": observation, "initial_goal": goal, "initial_sample": sample(&state),
        "replay_checked": args.replay, "trials": trials,
        "notes": [
            "Prefix uses the retained goal policy. By default it captures the old support rejection; stalled_prefix uses corrected support and captures its first mechanical-stall cooldown.",
            "Each branch clones the complete native state and goal history at the first false support refusal.",
            "Only the selected tank's support check changes between each reference/candidate pair; the other tank remains reference.",
            "Goal history is retained, including retries. Held/neutral branches override its requested axis only.",
            "improved_goal changes only goal policy versus candidate_goal, on identical corrected support and with the same retained controller history.",
            "Torque is identical in every branch of one report; different torque reports have different physical starting states.",
            "Current support appears in drive.supported; reference_supported records the old gate on the same state.",
            "Courses disable firing and park tank 2. This is an untimed diagnostic, not a performance benchmark."
        ]
    });
    if let Some(dir) = args.capture_dir {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("start.rapier"), native).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    println!("{}", serde_json::to_string(&report).unwrap());
    Ok(())
}
