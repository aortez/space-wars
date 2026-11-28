//! Single-worker decision latency and closed-loop Clock throughput.
use engine_common::{
    ClockDuckCoursePattern,
    brain_controls::{BodyProfile, CreatureControls},
};
use scenario_clock::duck_training::{DuckCase, DuckControls, DuckDriver, DuckEpisode};
use spacewars_ai::c3::{C3Genome, DECISION_TICKS};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let genome = C3Genome::random(42, 0);
    println!("genome {}", genome.id());
    let mut samples = Vec::new();
    let mut ticks = 0;
    let start = Instant::now();
    const EPISODES: usize = 4;
    for i in 0..EPISODES {
        let case = DuckCase {
            pattern: ClockDuckCoursePattern::Platforms,
            entrance_right: !i.is_multiple_of(2),
            ..DuckCase::seeded(i as u64 / 2)
        };
        let mut episode = DuckEpisode::new(case, DuckDriver::External)?;
        let sensor = episode.sensor()?;
        let mut held = DuckControls::default();
        loop {
            if episode.controls_active()
                && episode
                    .progress()
                    .tick
                    .is_multiple_of(u64::from(DECISION_TICKS))
            {
                let observation = episode.observation(&sensor)?;
                let start = Instant::now();
                let actions = genome.infer(&observation)?;
                samples.push(start.elapsed().as_secs_f64() * 1e6);
                held = CreatureControls::decode(&actions, &BodyProfile::duck()).controls;
            }
            let progress = episode.step(held);
            if progress.outcome.is_some() {
                ticks += progress.tick;
                break;
            }
        }
    }
    let seconds = start.elapsed().as_secs_f64();
    samples.sort_by(f64::total_cmp);
    println!(
        "{} parameters, {} decisions, median {:.1} µs, p95 {:.1} µs; {} episodes, {} ticks in {:.3} s, {:.3} episodes/s",
        genome.parameter_count(),
        samples.len(),
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100],
        EPISODES,
        ticks,
        seconds,
        EPISODES as f64 / seconds,
    );
    Ok(())
}
