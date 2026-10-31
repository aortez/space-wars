//! Observation inspection and local timing, not a learned-flight evaluation.
use clap::Parser;
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    flight_environment::{
        FlightAction, FlightEnvironment, FlightObstacle, FlightTask, FlightTrace,
    },
    flight_policy_observation::schema,
};
use serde_json::json;
use std::{fs, hint::black_box, path::PathBuf, time::Instant};

#[derive(Parser)]
struct Args {
    /// Fresh output directory; existing evidence is never overwritten.
    #[arg(long)]
    out: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    fs::create_dir(&args.out)?;
    fs::write(
        args.out.join("schema.json"),
        serde_json::to_vec_pretty(&schema())?,
    )?;
    let base = FlightTask::default();
    let mut rotating = base.clone();
    rotating.start.spin = 1.0;
    rotating.start.planet_spin = 0.015;
    rotating.start.heading_offset = 0.7;
    let mut wall = base.clone();
    wall.obstacles.push(FlightObstacle {
        offset: Vec2::new(25.0, 0.0),
        half_extents: Vec2::new(1.0, 20.0),
        angle: 0.3,
    });
    let mut gravity = base.clone();
    gravity.start.gravity = true;
    gravity.goal_velocity = Vec2::new(0.0, 10.0);
    let mut cases = Vec::new();
    for (name, task) in [
        ("stationary", base),
        ("rotating-surfaces", rotating),
        ("independent-wall", wall),
        ("gravity-moving-goal", gravity),
    ] {
        let mut env = FlightEnvironment::new(task)?;
        let initial = env.policy_observation()?;
        let raw = env.observation()?;
        for _ in 0..5 {
            black_box(env.policy_observation()?);
        }
        let mut micros = Vec::with_capacity(100);
        for _ in 0..100 {
            let start = Instant::now();
            let observation = black_box(env.policy_observation()?);
            micros.push(start.elapsed().as_secs_f64() * 1e6);
            assert_eq!(observation, initial);
        }
        for _ in 0..30 {
            if env.progress().outcome.is_some() {
                break;
            }
            env.step(FlightAction::default())?;
            assert!(
                env.policy_observation()?
                    .inputs
                    .values()
                    .all(f32::is_finite)
            );
        }
        let final_observation = env.policy_observation()?;
        let trace: FlightTrace = serde_json::from_slice(&serde_json::to_vec(env.trace())?)?;
        let replay = FlightEnvironment::replay(&trace)?;
        assert_eq!(final_observation, replay.policy_observation()?);
        fs::write(
            args.out.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&json!({
                "initial_raw":raw,"initial_policy":initial,"final_policy":final_observation,
                "trace":trace,"timing_microseconds":micros
            }))?,
        )?;
        micros.sort_by(f64::total_cmp);
        cases.push(json!({
            "name":name,"objects":initial.object_sources.iter().flatten().count(),
            "ray_hits":initial.ray_sources.iter().flatten().count(),
            "values":initial.inputs.values().count(),"transitions":trace.steps.len(),
            "replay_policy_equal":true,"raw_polar_version":raw.surroundings.geometry.version,
            "native_queries":raw.surroundings.geometry.native_queries,
            "timing_us":{"samples":micros.len(),"median":micros[50],"p95":micros[94],"max":micros[99]}
        }));
    }
    let results = json!({
        "kind":"policy-observation-correctness-inspection","training":false,
        "schema_version":1,"decision_hz":60,"physics_hz":60,
        "debug_assertions":cfg!(debug_assertions),"cases":cases,
        "timing_scope":"single-process full adapter, warmed unchanged scene; excludes physics, reset, serialization, replay and neural inference; local diagnostic only"
    });
    fs::write(
        args.out.join("summary.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    println!(
        "Four observation cases and policy replays passed: {}",
        args.out.display()
    );
    Ok(())
}
