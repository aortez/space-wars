//! Identical-start comparison of the composed learned flight stack against the
//! native arrival and transfer laws on generated static flight tasks. Every arm
//! flies the same headless environment from the same completed neutral tick,
//! and the environment alone judges arrival, collision and the deadline. The
//! learned arm runs through the gameplay driver, not the training bridge.
//!
//! Usage: flight_stack_comparison --out DIR --seed SEED --per-family N
//!        [--max-rounds N] [--workers N] [--limit N] [--traces all|failures|none]
//!        [--envelope v1|generated|transfer] [--navigation FILE --motor FILE]
//!
//! The `v1` envelope keeps the static families with gravity off and the planet
//! stationary, as the stack was qualified. `generated` keeps every admitted
//! family with the generator's own seeded gravity and planet spin, to probe
//! the stack outside its envelope; the summary then also groups by gravity,
//! spin and goal motion. `transfer` generates match-arena transfers, with
//! `--per-family` as the number of admitted transfers. Without explicit export
//! paths the retained v1 stack flies.
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    flight::FlightControlLimits,
    flight_environment::{
        ArrivalErrors, FlightAction, FlightEnvironment, FlightTask, FlightTrace, generation,
        generation::{FlightFamily, FlightTaskBatch},
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use spacewars_ai::flight_stack::{
    FlightStack, FlightStackDriver, FlightStackError, HOLD_TICKS, NavigationGoal, STACK_ID,
};
use spacewars_ai::guidance::{Binding, Controller, Features, Policy, Request, Target};
use spacewars_ai::shortest_heading_error;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Instant;

const ARMS: [&str; 3] = ["learned_stack", "arrival_reference", "mission_transfer"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum TraceKeep {
    All,
    Failures,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Envelope {
    V1,
    Generated,
    Transfer,
}

struct Args {
    out: PathBuf,
    seed: u64,
    per_family: usize,
    max_rounds: u64,
    workers: usize,
    limit: Option<usize>,
    traces: TraceKeep,
    envelope: Envelope,
    navigation: Option<PathBuf>,
    motor: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut args = Args {
        out: PathBuf::new(),
        seed: 0,
        per_family: 0,
        max_rounds: 4000,
        workers: std::thread::available_parallelism().map_or(4, |n| n.get().min(12)),
        limit: None,
        traces: TraceKeep::Failures,
        envelope: Envelope::V1,
        navigation: None,
        motor: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag.as_str() {
            "--out" => args.out = PathBuf::from(value),
            "--seed" => args.seed = value.parse().expect("seed"),
            "--per-family" => args.per_family = value.parse().expect("per-family"),
            "--max-rounds" => args.max_rounds = value.parse().expect("max-rounds"),
            "--workers" => args.workers = value.parse().expect("workers"),
            "--limit" => args.limit = Some(value.parse().expect("limit")),
            "--traces" => {
                args.traces = match value.as_str() {
                    "all" => TraceKeep::All,
                    "failures" => TraceKeep::Failures,
                    "none" => TraceKeep::None,
                    _ => panic!("unknown trace policy {value}"),
                }
            }
            "--envelope" => {
                args.envelope = match value.as_str() {
                    "v1" => Envelope::V1,
                    "generated" => Envelope::Generated,
                    "transfer" => Envelope::Transfer,
                    _ => panic!("unknown envelope {value}"),
                }
            }
            "--navigation" => args.navigation = Some(PathBuf::from(value)),
            "--motor" => args.motor = Some(PathBuf::from(value)),
            _ => panic!("unknown flag {flag}"),
        }
    }
    assert!(
        args.out != PathBuf::new() && args.seed != 0 && args.per_family != 0,
        "--out, --seed and --per-family are required"
    );
    assert!(
        args.navigation.is_some() == args.motor.is_some(),
        "--navigation and --motor go together"
    );
    args
}

fn load_stack(args: &Args) -> FlightStack {
    match (&args.navigation, &args.motor) {
        (Some(navigation), Some(motor)) => {
            let navigation =
                serde_json::from_slice(&std::fs::read(navigation).expect("navigation export"))
                    .expect("navigation export json");
            let motor = serde_json::from_slice(&std::fs::read(motor).expect("motor export"))
                .expect("motor export json");
            FlightStack::from_files(navigation, motor).expect("stack contracts")
        }
        _ => FlightStack::retained().expect("retained stack"),
    }
}

#[derive(Clone, Serialize)]
struct TaskRow {
    index: usize,
    family: String,
    recipe: String,
    candidate_index: u64,
    reference_time_seconds: Option<f32>,
    gravity: bool,
    planet_spin: f32,
    goal_speed: f32,
    task: FlightTask,
}

/// The admitted tasks of the generated batch. Under the v1 envelope these are
/// the static families with gravity off and the planet stationary, indexed as
/// the LF-082 and LF-083 sets were; under the generated envelope every family
/// keeps its seeded gravity and planet spin.
fn generate(seed: u64, per_family: usize, max_rounds: u64, envelope: Envelope) -> Vec<TaskRow> {
    if envelope == Envelope::Transfer {
        return generation::transfer::generate_transfers(
            seed,
            per_family,
            max_rounds.min(8192),
            generation::transfer::TransferRange::Standard,
        )
        .expect("transfer batch")
        .into_iter()
        .filter(|a| a.admitted)
        .enumerate()
        .map(|(index, a)| TaskRow {
            index,
            family: "transfer".into(),
            recipe: "transfer".into(),
            candidate_index: a.candidate.index,
            reference_time_seconds: a.reference_time_seconds,
            gravity: true,
            planet_spin: 0.0,
            goal_speed: 0.0,
            task: a.candidate.task,
        })
        .collect();
    }
    let batch = FlightTaskBatch::generate(seed, per_family, max_rounds).expect("task batch");
    assert!(batch.complete, "task generation quota incomplete");
    batch
        .attempts
        .into_iter()
        .filter(|a| {
            a.admitted
                && (envelope == Envelope::Generated || a.candidate.family != FlightFamily::Moving)
        })
        .enumerate()
        .map(|(index, a)| {
            let mut task = a.candidate.task.clone();
            if envelope == Envelope::V1 {
                task.start.gravity = false;
                task.start.planet_spin = 0.0;
            }
            TaskRow {
                gravity: task.start.gravity,
                planet_spin: task.start.planet_spin,
                goal_speed: task.goal_velocity.length(),
                index,
                family: serde_json::to_value(a.candidate.family)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
                recipe: a.candidate.recipe.clone(),
                candidate_index: a.candidate.index,
                reference_time_seconds: a.evidence.as_ref().and_then(|e| e.reference_time_seconds),
                task,
            }
        })
        .collect()
}

#[derive(Clone, Serialize)]
struct ArmResult {
    arm: String,
    index: usize,
    family: String,
    outcome: String,
    ticks: u32,
    final_errors: ArrivalErrors,
    minimum_separation: Option<f32>,
    /// Navigation commands issued by the learned arm.
    commands: u32,
    error: Option<String>,
    nanoseconds_per_tick: u64,
}

fn finish(
    arm: &str,
    row: &TaskRow,
    env: &FlightEnvironment,
    commands: u32,
    error: Option<String>,
    started: Instant,
) -> (ArmResult, FlightTrace) {
    let progress = env.progress();
    let outcome = match (progress.outcome, &error) {
        (Some(outcome), _) => serde_json::to_value(outcome)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned(),
        (None, Some(_)) => "controller_error".into(),
        (None, None) => "incomplete".into(),
    };
    let minimum_separation = env
        .trace()
        .steps
        .iter()
        .filter_map(|s| s.evidence.minimum_separation)
        .reduce(f32::min);
    let ticks = progress.elapsed_ticks;
    (
        ArmResult {
            arm: arm.into(),
            index: row.index,
            family: row.family.clone(),
            outcome,
            ticks,
            final_errors: env.frame().errors,
            minimum_separation,
            commands,
            error,
            nanoseconds_per_tick: started.elapsed().as_nanos() as u64 / u64::from(ticks.max(1)),
        },
        env.trace().clone(),
    )
}

fn run_learned(
    stack: &FlightStack,
    driver: &mut FlightStackDriver,
    row: &TaskRow,
) -> (ArmResult, FlightTrace) {
    let started = Instant::now();
    let mut env = FlightEnvironment::new(row.task.clone()).expect("environment");
    let mut commands = 0;
    let mut error = None;
    while env.progress().outcome.is_none() {
        // The goal is where the environment's target is now, as a host
        // issues it each tick: a transfer goal rides its planet's orbit and
        // spin, a moving goal its planet's frame.
        let goal = NavigationGoal {
            id: row.index as u64 + 1,
            position: env.frame().target_position,
            velocity: env.frame().target_velocity,
            arrival: row.task.arrival,
            deadline_ticks: row.task.deadline_ticks,
        };
        let decision = match driver.decide(stack, env.state(), goal) {
            Ok(decision) => decision,
            Err(FlightStackError::Ended(outcome)) => {
                error = Some(format!("driver ended before the environment: {outcome:?}"));
                break;
            }
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        };
        commands += u32::from(decision.navigation.is_some());
        env.step(decision.action).expect("step");
    }
    finish("learned_stack", row, &env, commands, error, started)
}

/// The guidance component's native arrival law, `arrival_reference_v1`,
/// steering the ship's center of mass to the goal so both arms are judged at
/// the same point.
fn run_arrival_reference(row: &TaskRow) -> (ArmResult, FlightTrace) {
    let started = Instant::now();
    let mut env = FlightEnvironment::new(row.task.clone()).expect("environment");
    let own = env.state().free_flight_observation(0).expect("ship");
    let binding = Binding {
        owner: own.owner,
        spaceling: own.spaceling,
        vehicle: own.vehicle,
        planet: own.planet,
        epoch: 1,
    };
    let mut controller = Controller::new(Policy::Reference, binding).expect("controller");
    let mut error = None;
    while env.progress().outcome.is_none() {
        let own = env.state().free_flight_observation(0).expect("ship");
        let frame = env.frame();
        let target = Target {
            id: 1,
            position: frame.target_position - (frame.center_of_mass - frame.origin),
            velocity: frame.target_velocity,
        };
        let request = Request {
            binding,
            tick: own.tick,
            target_id: 1,
            features: Features::read(&own, target),
        };
        let movement = match controller.decide(request) {
            Ok(decision) => decision.movement,
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        };
        env.step(FlightAction {
            steering: movement.turn,
            thrust: f32::from(movement.thrust),
            brake: f32::from(movement.brake),
        })
        .expect("step");
    }
    finish("arrival_reference", row, &env, 0, error, started)
}

/// The mission pilot's transfer guidance toward a point, as `guide_with_brake`
/// flies a transfer leg with no boundary, assist or gravity terms: desired
/// velocity 0.7 times the offset capped at 55, braking when nearly there or
/// too fast, and the same spin and thrust duty laws.
fn run_mission_transfer(row: &TaskRow) -> (ArmResult, FlightTrace) {
    let started = Instant::now();
    let mut env = FlightEnvironment::new(row.task.clone()).expect("environment");
    let limits = FlightControlLimits::for_sweep(0.0);
    let mut pwm = 0.0_f32;
    while env.progress().outcome.is_none() {
        let own = env.state().free_flight_observation(0).expect("ship");
        let frame = env.frame();
        let delta =
            frame.target_position - (frame.center_of_mass - frame.origin) - own.ship.position;
        let desired_world =
            frame.target_velocity + delta.normalized() * (delta.length() * 0.7).min(55.0);
        let relative = own.ship.velocity - own.frame_velocity;
        let brake = desired_world.length() < 10.0
            || own.ship.velocity.length() > desired_world.length() + 4.0;
        let acceleration = (desired_world - own.ship.velocity) * 2.0
            - own.gravity
            - if brake {
                limits.braking(relative)
            } else {
                Vec2::ZERO
            };
        let direction = acceleration.normalized();
        let error = shortest_heading_error(direction.rotate_radians(-own.ship.angle));
        let spin = -error * 3.0;
        let horizontal =
            (-(spin - (own.ship.spin - spin) * 0.2) / limits.turn_speed).clamp(-1.0, 1.0);
        let available =
            limits.thrust_acceleration * limits.thrust_fraction(own.flight.forward_speed);
        let aligned = Vec2::Y.rotate_radians(own.ship.angle).dot(direction) > 0.97;
        let duty = if aligned && available > 0.01 {
            (acceleration.length() / available).clamp(0.0, 1.0)
        } else {
            0.0
        };
        pwm = (pwm + duty).min(2.0);
        let thrust = pwm >= 1.0;
        if thrust {
            pwm -= 1.0;
        }
        env.step(FlightAction {
            steering: horizontal,
            thrust: f32::from(thrust),
            brake: f32::from(brake),
        })
        .expect("step");
    }
    finish("mission_transfer", row, &env, 0, None, started)
}

fn save(path: &Path, value: &impl Serialize) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).expect("write");
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    Some(if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    })
}

fn summarize(rows: &[TaskRow], results: &[ArmResult]) -> Value {
    let mut groups: Vec<(String, Vec<&TaskRow>)> = Vec::new();
    for row in rows {
        if !groups.iter().any(|(name, _)| name == &row.family) {
            groups.push((
                row.family.clone(),
                rows.iter().filter(|r| r.family == row.family).collect(),
            ));
        }
    }
    let partitions: [(&str, fn(&TaskRow) -> bool); 6] = [
        ("gravity_on", |r| r.gravity),
        ("gravity_off", |r| !r.gravity),
        ("spin_on", |r| r.planet_spin != 0.0),
        ("spin_off", |r| r.planet_spin == 0.0),
        ("goal_moving", |r| r.goal_speed > 0.0),
        ("goal_static", |r| r.goal_speed == 0.0),
    ];
    for (name, keep) in partitions {
        let selected: Vec<&TaskRow> = rows.iter().filter(|r| keep(r)).collect();
        if !selected.is_empty() && selected.len() != rows.len() {
            groups.push((name.into(), selected));
        }
    }
    groups.push(("all".into(), rows.iter().collect()));
    let mut summary = serde_json::Map::new();
    for (family, selected) in &groups {
        let mut arms = serde_json::Map::new();
        let passed_by = |arm: &str| -> BTreeMap<usize, u32> {
            results
                .iter()
                .filter(|r| r.arm == arm && r.outcome == "success")
                .filter(|r| selected.iter().any(|t| t.index == r.index))
                .map(|r| (r.index, r.ticks))
                .collect()
        };
        let learned = passed_by("learned_stack");
        for arm in ARMS {
            let rows_of: Vec<&ArmResult> = results
                .iter()
                .filter(|r| r.arm == arm && selected.iter().any(|t| t.index == r.index))
                .collect();
            let mut outcomes = BTreeMap::new();
            for r in &rows_of {
                *outcomes.entry(r.outcome.clone()).or_insert(0u32) += 1;
            }
            let passed = passed_by(arm);
            let ticks: Vec<f64> = passed.values().map(|&t| f64::from(t)).collect();
            let shared: Vec<(u32, u32)> = passed
                .iter()
                .filter_map(|(index, &ticks)| learned.get(index).map(|&l| (l, ticks)))
                .collect();
            let paired: Vec<f64> = shared
                .iter()
                .map(|&(l, t)| f64::from(l) - f64::from(t))
                .collect();
            arms.insert(
                arm.into(),
                json!({
                    "cases": rows_of.len(),
                    "passed": passed.len(),
                    "outcomes": outcomes,
                    "mean_ticks_passed": mean(&ticks),
                    "median_ticks_passed": median(&ticks),
                    "mean_nanoseconds_per_tick": mean(&rows_of.iter().map(|r| r.nanoseconds_per_tick as f64).collect::<Vec<_>>()),
                    "shared_passes_with_learned": shared.len(),
                    "learned_minus_arm_ticks_mean": mean(&paired),
                    "learned_minus_arm_ticks_median": median(&paired),
                    "learned_faster_on_shared": paired.iter().filter(|&&d| d < 0.0).count(),
                    "mean_ticks_on_shared": mean(&shared.iter().map(|&(_, t)| f64::from(t)).collect::<Vec<_>>()),
                    "learned_mean_ticks_on_shared": mean(&shared.iter().map(|&(l, _)| f64::from(l)).collect::<Vec<_>>()),
                }),
            );
        }
        summary.insert(
            family.clone(),
            json!({"cases": selected.len(), "arms": arms}),
        );
    }
    Value::Object(summary)
}

fn main() {
    let args = parse_args();
    std::fs::create_dir_all(&args.out).expect("output directory");
    let traces_dir = args.out.join("traces");
    std::fs::create_dir_all(&traces_dir).expect("traces directory");
    let mut rows = generate(args.seed, args.per_family, args.max_rounds, args.envelope);
    if let Some(limit) = args.limit {
        rows.truncate(limit);
    }
    let stack = load_stack(&args);
    let fixture = |name: &str| {
        let path = match (name, &args.navigation, &args.motor) {
            ("navigation.json", Some(path), _) | ("motor.json", _, Some(path)) => path.clone(),
            _ => Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/learned-flight-stack-v1")
                .join(name),
        };
        let bytes = std::fs::read(&path).expect("export");
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(&bytes))
    };
    save(
        &args.out.join("plan.json"),
        &json!({
            "kind": "flight-stack-comparison-v1",
            "stack": STACK_ID,
            "contract": stack.contract(),
            "navigation_export": args.navigation.as_ref().map(|p| p.display().to_string()),
            "motor_export": args.motor.as_ref().map(|p| p.display().to_string()),
            "navigation_sha256": fixture("navigation.json"),
            "motor_sha256": fixture("motor.json"),
            "hold_ticks": HOLD_TICKS,
            "arms": ARMS,
            "seed": args.seed, "per_family": args.per_family, "max_rounds": args.max_rounds,
            "limit": args.limit, "workers": args.workers,
            "envelope": match args.envelope { Envelope::V1 => "v1", Envelope::Generated => "generated", Envelope::Transfer => "transfer" },
            "profile": if cfg!(debug_assertions) { "ci" } else { "release" },
            "tasks": rows.len(),
        }),
    );
    save(&args.out.join("tasks.json"), &rows);
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<ArmResult>> = Mutex::new(Vec::new());
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..args.workers {
            scope.spawn(|| {
                let mut driver = FlightStackDriver::new(0).expect("driver");
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(row) = rows.get(index) else {
                        break;
                    };
                    let outputs = [
                        run_learned(&stack, &mut driver, row),
                        run_arrival_reference(row),
                        run_mission_transfer(row),
                    ];
                    let mut line = format!("task {index:03} {:<10}", row.family);
                    for (result, trace) in outputs {
                        line.push_str(&format!(
                            "  {}: {} in {} ticks",
                            result.arm, result.outcome, result.ticks
                        ));
                        let keep = match args.traces {
                            TraceKeep::All => true,
                            TraceKeep::Failures => result.outcome != "success",
                            TraceKeep::None => false,
                        };
                        if keep {
                            save(
                                &traces_dir.join(format!("{}-{index:03}.json", result.arm)),
                                &trace,
                            );
                        }
                        results.lock().unwrap().push(result);
                    }
                    println!("{line}");
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|r| (r.index, ARMS.iter().position(|a| *a == r.arm)));
    save(&args.out.join("results.json"), &results);
    let mut summary = summarize(&rows, &results);
    summary["seconds"] = json!(started.elapsed().as_secs_f64());
    save(&args.out.join("summary.json"), &summary);
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}
