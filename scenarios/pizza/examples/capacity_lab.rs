//! Headless normal-gameplay stress run. See docs/pizza-capacity.md.
use engine_common::Scenario;
use scenario_pizza::{PizzaBounds, PizzaScenario, capacity_lab};
use serde_json::json;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 6 {
        return Err(
            "usage: capacity_lab POPULATION LIMIT [STEPS=120] [SEED=7] [WIDTH=800] [HEIGHT=480]"
                .into(),
        );
    }
    let number = |index: usize, default: u64| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(args
            .get(index)
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(default))
    };
    let population = number(0, 500)?;
    let limit = number(1, 500)?;
    let steps = number(2, 120)?;
    let seed = number(3, 7)?;
    let width = number(4, 800)?;
    let height = number(5, 480)?;
    if !(1..=3600).contains(&steps)
        || !(1..=8192).contains(&width)
        || !(1..=8192).contains(&height)
        || population > 5000
        || limit > 5000
    {
        return Err("steps must be 1..=3600, dimensions 1..=8192, populations <=5000".into());
    }
    let mut state = capacity_lab::mixed_size_fixture(
        population as usize,
        limit as usize,
        PizzaBounds::from_aspect_ratio(width as f32 / height as f32),
        seed,
    )?;
    let initial = state.population();
    let mut timings = Vec::with_capacity(steps as usize);
    let mut samples = Vec::with_capacity(steps as usize);
    let mut peak = state.balls.len();
    let mut contact_samples = 0;
    let mut explosion_steps = 0;
    let mut last_exploded = 0;
    for _ in 0..steps {
        let started = Instant::now();
        PizzaScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let micros = started.elapsed().as_secs_f64() * 1e6;
        timings.push(micros);
        let counts = state.population();
        assert_eq!(counts.balls, counts.physics_balls);
        assert_eq!(counts.balls, counts.ball_colliders);
        assert!(counts.balls <= counts.limit);
        peak = peak.max(counts.balls);
        explosion_steps += u64::from(counts.births.exploded > last_exploded);
        last_exploded = counts.births.exploded;
        let metrics = state.last_step_metrics;
        contact_samples += metrics.rapier.contacts;
        // Bounded in-memory trace, serialized only after the timed simulation.
        samples.push(
            json!({"tick":state.tick, "step_us":micros, "population":counts,
            "contacts":metrics.rapier.contacts, "contact_pairs":metrics.rapier.contact_pairs,
            "gravity_us":metrics.gravity_time.as_secs_f64()*1e6,
            "physics_us":metrics.physics_time.as_secs_f64()*1e6,
            "snapshot_us":metrics.snapshot_time.as_secs_f64()*1e6,
            "lifecycle_us":metrics.lifecycle_time.as_secs_f64()*1e6}),
        );
    }
    timings.sort_by(f64::total_cmp);
    let percentile =
        |p: f64| timings[((timings.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "fixture_version":capacity_lab::FIXTURE_VERSION, "seed":seed,
            "width":width,"height":height,"steps":steps,"fixed_dt_ns":16_666_667,
            "initial":initial,"final":state.population(),"peak_balls":peak,
            "contact_samples":contact_samples,"explosion_steps":explosion_steps,
            "step_us":{"p50":percentile(0.50),"p95":percentile(0.95),"p99":percentile(0.99),"worst":timings.last()},
            "scope":"headless simulation only; no renderer, frame pacing or Pi CPU qualification",
            "samples":samples,
        }))?
    );
    Ok(())
}
