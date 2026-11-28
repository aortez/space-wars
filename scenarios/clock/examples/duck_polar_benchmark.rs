//! Full observation cost while a duck crosses the Platforms course. Times
//! exclude Clock stepping, sensor construction and rendering.
use engine_common::{ClockDuckCoursePattern, ClockEventKind, ClockEventProfile, Scenario};
use engine_core::Vec2;
use engine_rapier::world::polar::policy::CreatureContext;
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn main() -> Result<(), &'static str> {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            duck_course_pattern: Some(ClockDuckCoursePattern::Platforms),
            aspect_ratio: 1.6,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(10, 8, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Duck),
        ],
        Duration::ZERO,
    );
    let sensor = state.duck_polar_sensor()?;
    let mut samples = Vec::new();
    for tick in 1..=720 {
        ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        if tick > 60 && tick % 6 == 0 {
            let context = CreatureContext {
                goal: Vec2::ZERO,
                goal_velocity: Vec2::ZERO,
                remaining_seconds: 35. - tick as f32 / 60.,
                previous_move: 0.,
                previous_jump: false,
                previous_run: false,
                previous_dive: false,
            };
            let start = Instant::now();
            black_box(state.duck_polar_observation(&sensor, context)?);
            samples.push(start.elapsed().as_secs_f64() * 1e6);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{} observations, median {:.1} µs, p95 {:.1} µs",
        samples.len(),
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100]
    );
    Ok(())
}
