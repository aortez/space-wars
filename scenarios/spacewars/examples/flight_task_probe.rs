//! Reproducible task-generator inspection. No policy is trained or evaluated.
use clap::Parser;
use scenario_spacewars::surface_sortie::flight_environment::{
    FlightEnvironment,
    generation::{FlightCandidate, FlightTaskBatch, recipe_manifest},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = 20261022)]
    seed: u64,
    #[arg(long, default_value_t = 16)]
    per_family: usize,
    #[arg(long, default_value_t = 128)]
    max_rounds: u64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    fs::create_dir(&args.out)?;
    let start = Instant::now();
    let batch = FlightTaskBatch::generate(args.seed, args.per_family, args.max_rounds)?;
    let mut cases = Vec::new();
    let mut compact = Vec::new();
    let mut totals: BTreeMap<String, Value> = BTreeMap::new();
    for report in &batch.attempts {
        let candidate = &report.candidate;
        assert_eq!(
            *candidate,
            FlightCandidate::generate(args.seed, candidate.index)?
        );
        let mut reconstruction = false;
        if report.admitted {
            let decoded = serde_json::from_slice(&serde_json::to_vec(&candidate.task)?)?;
            let a = FlightEnvironment::new(candidate.task.clone())?;
            let b = FlightEnvironment::new(decoded)?;
            assert_eq!(a.policy_observation()?, b.policy_observation()?);
            reconstruction = true;
        }
        let family = serde_json::to_value(candidate.family)?
            .as_str()
            .unwrap()
            .to_owned();
        let row = totals.entry(family).or_insert(
            json!({"candidates":0,"admitted":0,"rejected":BTreeMap::<String,u64>::new()}),
        );
        row["candidates"] = json!(row["candidates"].as_u64().unwrap() + 1);
        if report.admitted {
            row["admitted"] = json!(row["admitted"].as_u64().unwrap() + 1);
        } else {
            let key = report.rejection.as_deref().unwrap();
            let n = row["rejected"][key].as_u64().unwrap_or(0);
            row["rejected"][key] = json!(n + 1);
        }
        let mut full = serde_json::to_value(report)?;
        full["serialized_reset_equal"] = json!(reconstruction);
        if candidate.family==scenario_spacewars::surface_sortie::flight_environment::generation::FlightFamily::Altered {
            let mut before=candidate.clone();
            before.task.terrain_edits.clear();
            before.task.obstacles.clear();
            let baseline=before.inspect();
            full["baseline_geometry"]=serde_json::to_value(baseline.geometry)?;
            full["baseline_direct_clear"]=json!(baseline.evidence.map(|e|e.direct_clear));
        }
        let evidence = report.evidence.as_ref();
        compact.push(json!({
            "index":candidate.index,"family":candidate.family,"recipe":candidate.recipe,"admitted":report.admitted,"rejection":report.rejection,
            "start_speed":candidate.task.start.relative_velocity.length(),"start_spin":candidate.task.start.spin,"heading":candidate.task.start.heading_offset,
            "gravity":candidate.task.start.gravity,"planet_spin":candidate.task.start.planet_spin,"goal_distance":candidate.task.goal_offset.length(),"target_speed":candidate.task.goal_velocity.length(),
            "reference_seconds":evidence.and_then(|e|e.reference_time_seconds),"route_length":evidence.map(|e|e.route_length),
            "first_leg_moves_away":evidence.map(|e|e.first_leg_moves_away),"object_count":evidence.map(|e|e.object_count),
            "hull_com_bound":evidence.map(|e|e.hull_com_radius),"gravity_sample_max":evidence.map(|e|e.gravity_samples.iter().map(|s|s.acceleration.length()).fold(0.0_f32,f32::max)),
            "serialized_reset_equal":reconstruction,"baseline_direct_clear":full.get("baseline_direct_clear")
        }));
        fs::write(
            args.out.join(format!("task-{:04}.json", candidate.index)),
            serde_json::to_vec_pretty(&full)?,
        )?;
        cases.push(full);
    }
    let results = json!({"kind":"task-generator-inspection","training":false,"seed":args.seed,"per_family":args.per_family,"max_rounds":args.max_rounds,"complete":batch.complete,"attempts":batch.attempts.len(),"manifest":recipe_manifest(),"families":totals,"elapsed_seconds":start.elapsed().as_secs_f64(),"cases":compact});
    fs::write(
        args.out.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    let data =
        serde_json::to_string(&json!({"results":results,"cases":cases}))?.replace('<', "\\u003c");
    let html = include_str!("flight_task_probe/viewer.html").replace("/*DATA*/null", &data);
    fs::write(args.out.join("index.html"), html)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"complete":batch.complete,"families":totals,"seconds":start.elapsed().as_secs_f64()})
        )?
    );
    if !batch.complete {
        return Err("task batch budget exhausted; partial evidence retained".into());
    }
    Ok(())
}
