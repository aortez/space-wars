//! Bounded desktop/Pi workload; no display, wall-clock input or sleeping.
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario, EVENT_CATALOG};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn summary(label: &str, values: &mut [f64]) {
    values.sort_by(f64::total_cmp);
    println!(
        "{label}: p95={:.4}ms max={:.4}ms",
        values[values.len() * 95 / 100],
        values[values.len() - 1]
    );
}

fn main() {
    let water_lab = std::env::args().skip(1).any(|a| a == "--water-lab");
    println!(
        "fixture={}",
        if water_lab {
            "buoyancy lab"
        } else {
            "normal meltdown"
        }
    );
    let duration = EVENT_CATALOG[ClockEventKind::Meltdown as usize].duration_ticks;
    for (width, height) in [(800, 480), (480, 800), (1280, 720)] {
        let mut steps = Vec::new();
        let mut frames = Vec::new();
        let mut peak_primitives = 0;
        let mut peak_cells = 0;
        let mut peak_columns = 0;
        let mut peak_spills = 0;
        let mut max_reclaimed = 0;
        let mut peak_bodies = 0;
        let mut peak_colliders = 0;
        for seed in 0..24 {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: width as f32 / height as f32,
                    event_profile: ClockEventProfile::Off,
                    water_lab,
                    ..ClockConfig::default()
                },
                seed,
            );
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                    ClockAction::trigger_event(ClockEventKind::Meltdown),
                ],
                Duration::ZERO,
            );
            for _ in 0..duration {
                let started = Instant::now();
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                steps.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                let frame = black_box(ClockScenario::render_frame(&state));
                frames.push(started.elapsed().as_secs_f64() * 1000.0);
                peak_bodies = peak_bodies.max(state.body_count());
                peak_colliders = peak_colliders.max(state.collider_count());
                peak_primitives = peak_primitives.max(
                    frame
                        .layers
                        .iter()
                        .map(|l| l.primitives.len())
                        .sum::<usize>(),
                );
                if let Some(m) = state.meltdown_state() {
                    let accounted = ((m.waiting_cells + m.airborne_cells) as u64) * 1_000_000
                        + m.pooled_microunits
                        + m.spilling_microunits
                        + m.drained_microunits
                        + m.reclaimed_microunits;
                    assert!(accounted.abs_diff(m.initial_cells as u64 * 1_000_000) <= 2);
                    peak_cells = peak_cells.max(m.waiting_cells + m.airborne_cells);
                    peak_columns = peak_columns.max(m.water_columns);
                    peak_spills = peak_spills.max(m.spill_parcels);
                    max_reclaimed = max_reclaimed.max(m.reclaimed_microunits);
                }
            }
            assert_eq!(state.meltdown_state(), None);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        }
        println!(
            "{width}x{height}: 24 events, {} ticks, peak cells={peak_cells} columns={peak_columns} spills={peak_spills} bodies={peak_bodies} colliders={peak_colliders} primitives={peak_primitives} reclaimed={:.6} cell-volumes",
            steps.len(),
            max_reclaimed as f64 / 1_000_000.0
        );
        summary("step", &mut steps);
        summary("draw list (not raster/presentation)", &mut frames);
    }
}
