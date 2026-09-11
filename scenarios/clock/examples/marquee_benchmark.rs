//! Fixed-tick recipe workload; no display, wall-time input or sleeps.
use engine_common::{ClockEventKind, ClockEventProfile, ClockMarqueePreset, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario, MARQUEE_TICKS};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn stats(label: &str, values: &mut [f64]) {
    values.sort_by(f64::total_cmp);
    println!(
        "{label}: p95={:.4}ms max={:.4}ms",
        values[values.len() * 95 / 100],
        values[values.len() - 1]
    );
}

fn main() {
    for (width, height) in [(800, 480), (480, 800), (1280, 720)] {
        for preset in ClockMarqueePreset::ALL {
            let mut steps = Vec::with_capacity(MARQUEE_TICKS as usize * 12);
            let mut frames = Vec::with_capacity(steps.capacity());
            let mut peak = 0;
            for seed in 0..12 {
                let mut state = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: width as f32 / height as f32,
                        marquee_preset: preset,
                        event_profile: ClockEventProfile::Off,
                        ..ClockConfig::default()
                    },
                    seed,
                );
                ClockScenario::step(
                    &mut state,
                    &[
                        ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                        ClockAction::preview_event(ClockEventKind::Marquee),
                    ],
                    Duration::ZERO,
                );
                for tick in 0..MARQUEE_TICKS {
                    // Include the once-per-second content refresh in step cost.
                    let reading = [ClockAction::set_reading(
                        ClockReading::new(8, 8, (tick / 60) as u8).unwrap(),
                    )];
                    let actions = if tick % 60 == 0 { &reading[..] } else { &[] };
                    let start = Instant::now();
                    ClockScenario::step(&mut state, actions, Duration::from_nanos(16_666_667));
                    steps.push(start.elapsed().as_secs_f64() * 1000.0);
                    let start = Instant::now();
                    let frame = black_box(ClockScenario::render_frame(&state));
                    frames.push(start.elapsed().as_secs_f64() * 1000.0);
                    peak = peak.max(
                        frame
                            .layers
                            .iter()
                            .map(|l| l.primitives.len())
                            .sum::<usize>(),
                    );
                }
                assert!(state.marquee_state().is_none());
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            }
            println!(
                "{width}x{height} {}: 12 events, {} ticks, peak primitives={peak}",
                preset.label(),
                steps.len()
            );
            stats("step", &mut steps);
            stats("draw list (not raster/presentation)", &mut frames);
        }
    }
}
