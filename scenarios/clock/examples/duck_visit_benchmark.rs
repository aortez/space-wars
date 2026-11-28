//! Headless cost of the timed visit with scripted and external controls.
use engine_common::{ClockDuckCoursePattern, ClockDuckOutcome};
use scenario_clock::duck_training::{DuckCase, DuckControls, DuckDriver, DuckEpisode};
use std::time::Instant;

fn main() -> Result<(), &'static str> {
    let case = DuckCase {
        pattern: ClockDuckCoursePattern::Platforms,
        entrance_right: false,
        ..DuckCase::seeded(42)
    };
    for driver in [DuckDriver::Scripted, DuckDriver::External] {
        let mut episode = DuckEpisode::new(case, driver)?;
        let start = Instant::now();
        let progress = loop {
            let p = episode.step(DuckControls::default());
            if p.outcome.is_some() {
                break p;
            }
        };
        println!(
            "{driver:?} {:?}, {} ticks, {:.1} µs/tick",
            progress.outcome,
            progress.tick,
            start.elapsed().as_secs_f64() * 1e6 / progress.tick as f64
        );
        if driver == DuckDriver::External {
            assert_eq!(progress.outcome, Some(ClockDuckOutcome::TimedOut));
        }
    }
    Ok(())
}
