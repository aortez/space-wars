//! Small fixed-control environment checks, not policy training or qualification.
use clap::Parser;
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::flight_environment::{
    FlightAction, FlightEnvironment, FlightObstacle, FlightOutcome, FlightTask, FlightTrace,
};
use serde_json::json;
use std::{fs, path::PathBuf};

#[derive(Parser)]
struct Args {
    /// New directory for compact JSON traces and the smoke-check summary.
    #[arg(long)]
    out: PathBuf,
    /// Verify an existing trace instead of generating the built-in checks.
    #[arg(long)]
    replay: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    fs::create_dir(&args.out)?;
    if let Some(path) = args.replay {
        let trace: FlightTrace = serde_json::from_slice(&fs::read(path)?)?;
        let env = FlightEnvironment::replay(&trace)?;
        fs::write(
            args.out.join("replay.json"),
            serde_json::to_vec_pretty(&env.progress())?,
        )?;
        println!(
            "Verified {} trace transitions: {:?}",
            trace.steps.len(),
            env.progress().outcome
        );
        return Ok(());
    }
    let arrived = FlightTask {
        goal_offset: Vec2::ZERO,
        ..Default::default()
    };
    let mut moving = arrived.clone();
    moving.start.relative_velocity = Vec2::new(10.0, 0.0);
    moving.goal_velocity = moving.start.relative_velocity;
    let mut spinning = arrived.clone();
    spinning.start.spin = 1.8;
    spinning.deadline_ticks = 60;
    let mut flythrough = FlightTask {
        goal_offset: Vec2::new(10.0, 0.0),
        deadline_ticks: 60,
        ..Default::default()
    };
    flythrough.start.relative_velocity = Vec2::new(70.0, 0.0);
    let mut wall = FlightTask::default();
    wall.start.relative_velocity = Vec2::new(70.0, 0.0);
    wall.obstacles.push(FlightObstacle {
        offset: Vec2::new(25.0, 0.0),
        half_extents: Vec2::new(1.0, 20.0),
        angle: 0.0,
    });
    let mut brake = FlightTask {
        goal_offset: Vec2::new(2.5, 0.0),
        deadline_ticks: 180,
        ..Default::default()
    };
    brake.start.relative_velocity = Vec2::new(10.0, 0.0);
    let checks = [
        (
            "stationary-arrival",
            arrived,
            FlightAction::default(),
            FlightOutcome::Success,
        ),
        (
            "moving-arrival",
            moving,
            FlightAction::default(),
            FlightOutcome::Success,
        ),
        (
            "spinning-arrival",
            spinning,
            FlightAction {
                steering: -1.0,
                ..Default::default()
            },
            FlightOutcome::Deadline,
        ),
        (
            "fast-flythrough",
            flythrough,
            FlightAction::default(),
            FlightOutcome::Deadline,
        ),
        (
            "wall-impact",
            wall,
            FlightAction::default(),
            FlightOutcome::Collision,
        ),
        (
            "braking-arrival",
            brake,
            FlightAction {
                brake: 1.0,
                ..Default::default()
            },
            FlightOutcome::Success,
        ),
    ];
    let mut results = Vec::new();
    for (name, task, action, expected) in checks {
        let mut env = FlightEnvironment::new(task)?;
        while env.progress().outcome.is_none() {
            // Exercise sensing at the same cadence as decisions, including the
            // terminal observation. No evaluator fields generate the controls.
            env.observation()?;
            env.step(action)?;
        }
        env.observation()?;
        let bytes = serde_json::to_vec_pretty(env.trace())?;
        let decoded: FlightTrace = serde_json::from_slice(&bytes)?;
        let replay = FlightEnvironment::replay(&decoded)?;
        assert_eq!(replay.progress().outcome, Some(expected), "{name}");
        fs::write(args.out.join(format!("{name}.json")), bytes)?;
        results.push(json!({ "name": name, "progress": env.progress(), "final_errors": env.frame().errors, "replay_verified": true }));
    }
    let summary = json!({"kind": "environment_smoke_checks", "policy_training": false, "decision_hz": 60, "physics_hz": 60, "checks": results});
    fs::write(
        args.out.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!(
        "{} environment checks and independent replays passed; {}",
        results.len(),
        args.out.display()
    );
    Ok(())
}
