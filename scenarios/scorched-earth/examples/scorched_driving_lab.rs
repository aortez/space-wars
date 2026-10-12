//! Same-state CPU/neutral/held-drive comparisons. No performance claims: all
//! native inspection, replay, material auditing and trace capture are untimed.
use clap::{Parser, ValueEnum};
use engine_common::Scenario;
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{
    BankStrength, DrivingCourse, FIXED_HZ, ScorchedConfig, ScorchedScenario, ScorchedState,
    WheelTuning,
    driving_lab::{
        SupportCheck, TravelGoal, physics_snapshot, reference_ground_check, snapshot,
        step_reference_support_trial, step_reference_trial, step_support_trial, step_trial,
        wheel_contacts,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, ValueEnum)]
enum Shape {
    Round,
    Angular,
}
#[derive(Clone, Copy, ValueEnum)]
enum Strength {
    Off,
    Weak,
    Strong,
}
#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, value_enum, default_value = "angular")]
    shape: Shape,
    #[arg(long, value_enum, default_value = "off")]
    strength: Strength,
    /// Omit to capture an ordinary duel; those runs require --blocked.
    #[arg(long, value_parser = parse_course, required_unless_present = "blocked")]
    course: Option<DrivingCourse>,
    /// Use rigid tracks instead of the default sprung wheels.
    #[arg(long)]
    rigid: bool,
    /// Compare a drive torque while preserving speed, brakes and suspension.
    #[arg(long, conflicts_with = "rigid")]
    wheel_torque: Option<f32>,
    /// Compare motor response, or use 80 to reproduce retained wheel captures.
    #[arg(long, conflicts_with = "rigid")]
    wheel_damping: Option<f32>,
    /// Compare wheel geometry at constant wheel mass and cruising speed.
    #[arg(long, conflicts_with = "rigid")]
    wheel_radius: Option<f32>,
    /// Add a goal-seeking continuation that stops at the course flag.
    #[arg(long, requires = "course")]
    goal: bool,
    /// Retain the original slow-on-all-rough-ground goal policy for comparisons.
    #[arg(long, requires = "goal")]
    reference_goal: bool,
    #[arg(long, requires = "goal", default_value_t = 2.5)]
    cruise_speed: f32,
    /// Fork after 60 consecutive ticks in 'blocked ground', within 30 seconds.
    #[arg(long)]
    blocked: bool,
    /// Reproduce a blocked prefix with the retained two-probe ground check.
    #[arg(long, requires = "blocked")]
    reference_prefix: bool,
    /// Reproduce the old 40-degree per-contact motor support gate in every trial.
    #[arg(long)]
    reference_support: bool,
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
    player: u8,
    #[arg(long, default_value_t = 12, value_parser = clap::value_parser!(u32).range(1..=60))]
    seconds: u32,
    /// Recreate the prefix and repeat every continuation, comparing all results.
    #[arg(long)]
    replay: bool,
    /// Save the native starting snapshot, observation and report for this case.
    #[arg(long)]
    capture_dir: Option<PathBuf>,
}

fn parse_course(s: &str) -> Result<DrivingCourse, String> {
    DrivingCourse::ALL
        .into_iter()
        .find(|c| c.label().eq_ignore_ascii_case(s))
        .ok_or_else(|| "choose flat, step, slope, trench, or rubble".into())
}

fn capture(args: &Args) -> Result<ScorchedState, String> {
    if args.course.is_some() && args.player != 1 {
        return Err("the course parks tank 2; choose --player 1".into());
    }
    let config = ScorchedConfig {
        shape: match args.shape {
            Shape::Round => GrainShape::Round,
            Shape::Angular => GrainShape::Hexagon,
        },
        suspension: !args.rigid,
        wheel_tuning: WheelTuning {
            radius: args.wheel_radius.unwrap_or(WheelTuning::default().radius),
            motor_torque: args
                .wheel_torque
                .unwrap_or(WheelTuning::default().motor_torque),
            motor_damping: args
                .wheel_damping
                .unwrap_or(WheelTuning::default().motor_damping),
        },
        course: args.course,
        demo: true,
        ..Default::default()
    }
    .with_destruction(match args.strength {
        Strength::Off => BankStrength::Off,
        Strength::Weak => BankStrength::Weak,
        Strength::Strong => BankStrength::Strong,
    });
    let mut state = ScorchedScenario::init(config, args.seed);
    let player = usize::from(args.player - 1);
    if args.blocked {
        let mut streak = 0;
        for _ in 0..30 * FIXED_HZ {
            if args.reference_prefix {
                if args.reference_support {
                    step_reference_support_trial(&mut state, player, None, false);
                } else {
                    step_reference_trial(&mut state, player, None, false);
                }
            } else if args.reference_support {
                step_support_trial(&mut state, player, None, SupportCheck::Reference);
            } else {
                step_trial(&mut state, player, None);
            }
            streak = if state.driving_diagnostics()[player].mode == "blocked ground" {
                streak + 1
            } else {
                0
            };
            if streak >= FIXED_HZ {
                state.audit().map_err(|e| e.to_string())?;
                return Ok(state);
            }
        }
        Err("no sustained blocked-ground case within 30 seconds".into())
    } else {
        // Settle the native assembly and rubble identically before all trials.
        for _ in 0..120 {
            if args.reference_support {
                step_support_trial(&mut state, player, Some(0.0), SupportCheck::Reference);
            } else {
                step_trial(&mut state, player, Some(0.0));
            }
        }
        state.audit().map_err(|e| e.to_string())?;
        Ok(state)
    }
}

fn trial(
    start: &ScorchedState,
    player: usize,
    direction: f32,
    axis: Option<f32>,
    args: &Args,
    candidate: bool,
    mut goal: Option<TravelGoal>,
) -> Value {
    let ticks = args.seconds * FIXED_HZ;
    let support = if args.reference_support {
        SupportCheck::Reference
    } else {
        SupportCheck::Current
    };
    let mut state = start.clone();
    assert_eq!(state.observation_hash(), start.observation_hash());
    assert_eq!(physics_snapshot(&state), physics_snapshot(start));
    let mut first = snapshot(&state, player, direction);
    first.supported = support.supported(&state, player);
    let mut samples = vec![first.clone()];
    let mut modes = BTreeMap::<&str, u32>::new();
    let mut reasons = BTreeMap::<&str, u32>::new();
    let mut reference_reasons = BTreeMap::<&str, u32>::new();
    let mut unsupported_ticks = 0;
    let mut max_pitch = first.pitch_degrees.abs();
    let mut farthest_progress = 0.0_f32;
    let mut finish_tick = None;
    let mut min_spring = f32::INFINITY;
    let mut max_spring = f32::NEG_INFINITY;
    let mut goal_modes = BTreeMap::<&str, u32>::new();
    let mut goal_samples = Vec::new();
    let mut speed_error = 0.0_f64;
    let mut moving_goal_ticks = 0;
    let mut arrived_ticks = 0;
    let mut arrival_tick = None;
    let mut goal_contact_samples = Vec::new();
    let mut rejected_contact_samples = Vec::new();
    let mut rejected_support_ticks = 0;
    for i in 0..ticks {
        let decision = goal.as_mut().map(|g| {
            if args.reference_goal {
                g.decide_reference(&state, player, support)
            } else {
                g.decide_with_support(&state, player, support)
            }
        });
        if let Some(d) = &decision {
            *goal_modes.entry(d.mode).or_default() += 1;
            if d.desired_speed > 0.0 {
                speed_error += f64::from((d.desired_speed - d.actual_speed).abs());
                moving_goal_ticks += 1;
            }
            arrived_ticks += u32::from(d.mode == "arrived");
            if d.mode == "arrived" {
                arrival_tick.get_or_insert(state.tick);
            }
        }
        let step = if args.reference_support {
            step_reference_support_trial
        } else {
            step_reference_trial
        };
        step(
            &mut state,
            player,
            decision.as_ref().map(|d| d.axis).or(axis),
            candidate,
        );
        let mut s = snapshot(&state, player, direction);
        s.supported = support.supported(&state, player);
        if goal.is_some()
            && !s.supported
            && let Some(contacts) = wheel_contacts(&state, player)
            && contacts.iter().flatten().any(|c| c.separation <= 0.03)
        {
            rejected_support_ticks += 1;
            // Retain the first few transitions as well as steady samples.
            if rejected_contact_samples.len() < 8 {
                rejected_contact_samples
                    .push(json!({"tick": state.tick, "drive": s, "contacts": contacts}));
            }
        }
        *reference_reasons
            .entry(
                reference_ground_check(&state, player, direction)
                    .reason
                    .unwrap_or("clear"),
            )
            .or_default() += 1;
        *modes.entry(s.driving.mode).or_default() += 1;
        *reasons
            .entry(s.ground.reason.unwrap_or("clear"))
            .or_default() += 1;
        unsupported_ticks += u32::from(!s.supported);
        max_pitch = max_pitch.max(s.pitch_degrees.abs());
        farthest_progress = farthest_progress.max((s.position[0] - first.position[0]) * direction);
        if let Some(lengths) = s.suspension_lengths {
            for v in lengths {
                min_spring = min_spring.min(v);
                max_spring = max_spring.max(v);
            }
        }
        if state
            .config
            .course
            .is_some_and(|c| s.position[0] >= c.finish_x())
            && s.supported
            && s.pitch_degrees.abs() < 60.0
            && s.health > 0.0
        {
            finish_tick.get_or_insert(s.tick);
        }
        if (i + 1) % FIXED_HZ == 0 || i + 1 == ticks {
            state.audit().expect("material and finite motion");
            samples.push(s);
            if let Some(decision) = decision {
                goal_samples.push(
                    json!({"tick": state.tick - 1, "decision": decision, "controller": goal}),
                );
                goal_contact_samples
                    .push(json!({"tick": state.tick, "contacts": wheel_contacts(&state, player)}));
            }
        }
    }
    let last = samples.last().unwrap();
    let mut result = json!({
        "axis_override": axis, "ticks": ticks, "candidate_ground_check": candidate,
        "initial_observation_hash": format!("{:016x}", start.observation_hash()),
        "final_observation_hash": format!("{:016x}", state.observation_hash()),
        "net_progress": (last.position[0] - first.position[0]) * direction,
        "farthest_progress": farthest_progress, "finish_tick": finish_tick,
        "unsupported_ticks": unsupported_ticks, "max_pitch_degrees": max_pitch,
        "spring_range": min_spring.is_finite().then_some([min_spring, max_spring]),
        "controller_mode_ticks": modes, "route_reason_ticks": reasons,
        "reference_route_reason_ticks": reference_reasons,
        "new_shots": state.shots - start.shots, "new_impacts": state.impacts - start.impacts,
        "material": state.audit().unwrap(), "samples": samples,
    });
    if let Some(goal) = goal {
        result["goal"] = json!({
            "controller": goal, "mode_ticks": goal_modes, "samples": goal_samples,
            "arrived_ticks": arrived_ticks, "arrival_tick": arrival_tick,
            "contact_samples": goal_contact_samples, "rejected_support_ticks": rejected_support_ticks,
            "rejected_contact_samples": rejected_contact_samples,
            "mean_absolute_speed_error": (moving_goal_ticks > 0).then(|| speed_error / f64::from(moving_goal_ticks)),
            "speed_error_ticks": moving_goal_ticks,
        });
    }
    result
}

fn comparisons(
    start: &ScorchedState,
    player: usize,
    direction: f32,
    args: &Args,
    goal: Option<TravelGoal>,
) -> Value {
    let mut result = json!({
        "cpu": trial(start, player, direction, None, args, true, None),
        "reference_cpu": trial(start, player, direction, None, args, false, None),
        "neutral": trial(start, player, direction, Some(0.0), args, false, None),
        "held": trial(start, player, direction, Some(direction), args, false, None),
    });
    if goal.is_some() {
        result["goal"] = trial(start, player, direction, None, args, true, goal);
    }
    result
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    if args
        .wheel_torque
        .is_some_and(|v| !v.is_finite() || !(30.0..=480.0).contains(&v))
        || args
            .wheel_damping
            .is_some_and(|v| !v.is_finite() || !(1.0..=2400.0).contains(&v))
        || args
            .wheel_radius
            .is_some_and(|v| !v.is_finite() || !(0.5..=1.0).contains(&v))
        || !args.cruise_speed.is_finite()
        || !(0.1..=2.5).contains(&args.cruise_speed)
    {
        return Err("choose torque 30..480, motor damping 1..2400, radius 0.5..1.0 and cruise speed 0.1..2.5".into());
    }
    let start = capture(&args)?;
    let player = usize::from(args.player - 1);
    let here = snapshot(&start, player, 1.0).position[0];
    let other = snapshot(&start, 1 - player, 1.0).position[0];
    // Matches the intent of the existing range policy at a blocked attempt.
    let toward = (other - here).signum();
    let direction = if args.course.is_some() || (other - here).abs() > 32.0 {
        toward
    } else {
        -toward
    };
    let goal = args
        .goal
        .then(|| TravelGoal::new(args.course.unwrap().finish_x(), args.cruise_speed));
    let runs = comparisons(&start, player, direction, &args, goal.clone());
    if args.replay {
        let replay = capture(&args)?;
        assert_eq!(
            start.observation_hash(),
            replay.observation_hash(),
            "recreated prefix"
        );
        assert_eq!(
            physics_snapshot(&start),
            physics_snapshot(&replay),
            "native prefix"
        );
        assert_eq!(
            runs,
            comparisons(&replay, player, direction, &args, goal),
            "full trial replay"
        );
    }
    let native = physics_snapshot(&start);
    let native_hash = native.iter().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    let observation: Value =
        serde_json::from_slice(&ScorchedScenario::observe(&start).payload).unwrap();
    let mut initial_drive = snapshot(&start, player, direction);
    if args.reference_support {
        initial_drive.supported = SupportCheck::Reference.supported(&start, player);
    }
    let mut report = json!({
        "workload": "scorched-driving-course-v2", "seed": args.seed, "course": args.course,
        "shape": match args.shape { Shape::Round => "round", Shape::Angular => "angular" },
        "strength": start.config.cohesion.label(), "suspension": !args.rigid,
        "player": args.player, "direction": direction, "captured_blocked": args.blocked,
        "reference_prefix": args.reference_prefix,
        "reference_support": args.reference_support,
        "reference_goal": args.reference_goal,
        "capture_tick": start.tick, "native_snapshot_hash": format!("{native_hash:016x}"),
        "initial_observation": observation, "initial_drive": initial_drive,
        "initial_reference_ground": reference_ground_check(&start, player, direction),
        "replay_checked": args.replay, "trials": runs,
        "notes": [
            "All four trials clone the same native world and controller state; no reset at the fork.",
            "The candidate changes only the selected tank's ground check. The other tank retains the reference ground check in all continuations.",
            "Ground samples show the candidate height profile; reference_route_reason_ticks records the retained two-probe check on the same world.",
            "The reference_support flag selects the old per-contact support gate for every trial, including its support diagnostics. Omit it for current wheelbase support.",
            "Held drive changes only the selected drive axis. Support gates and motor limits remain active.",
            "Overridden trials retain the stored CPU mode/history; controller_mode_ticks is not a fresh CPU decision there.",
            "Ordinary-duel aim, shot schedules and the other tank remain active. Neutral measures uncommanded motion.",
            "Course firing is disabled and tank 2 is parked; the shared dirt lifecycle remains active.",
            "A failed held-drive trial is inconclusive about all possible approaches. No timing is measured.",
        ],
    });
    if args.goal || args.wheel_radius.is_some() || args.wheel_torque.is_some() {
        report["workload"] = json!("scorched-driving-goal-v1");
        report["wheel_tuning"] = json!(start.config.wheel_tuning);
        report["goal_enabled"] = json!(args.goal);
        report["cruise_speed"] = json!(args.cruise_speed);
        report["notes"].as_array_mut().unwrap().push(json!(
            "Goal continuation requests drive input toward the flag; desired/actual speed and bounded retry history are recorded separately from the production CPU. Wheel tuning is fixed for every continuation of this starting state."
        ));
    }
    if let Some(dir) = args.capture_dir {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("start.rapier"), native).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("start.json"),
            serde_json::to_vec_pretty(&report["initial_observation"]).unwrap(),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    println!("{}", serde_json::to_string(&report).unwrap());
    Ok(())
}
