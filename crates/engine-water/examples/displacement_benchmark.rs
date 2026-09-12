//! Matched closed tanks with/without prescribed box displacement. No Rapier or
//! rendering. Measures occupancy submission plus stepping; motion/statistics
//! are outside the timer. No wall-clock performance assertions.
use engine_core::Vec2;
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld, displacement::DisplacementBox};
use std::{hint::black_box, time::Instant};

const DT: f64 = 1.0 / 60.0;
const TICKS: usize = 12_000;
const WARMUP: usize = 600;

fn main() {
    for columns in [32, 128, 512] {
        for enabled in [false, true] {
            let mut water = WaterWorld::new(
                WaterConfig {
                    gravity: 40.0,
                    damping: 2.0,
                    ..WaterConfig::default()
                },
                vec![PoolSpec {
                    left: -50.0,
                    column_width: 100.0 / columns as f64,
                    bed: vec![0.0; columns],
                    boundaries: [Boundary::Closed; 2],
                }],
            )
            .unwrap();
            for i in 0..columns {
                water
                    .add_to_pool(
                        0,
                        -50.0 + (i as f64 + 0.5) * 100.0 / columns as f64,
                        2000.0 / columns as f64,
                    )
                    .unwrap();
            }
            let mut times = Vec::with_capacity(TICKS);
            let mut peak_displaced: f64 = 0.0;
            let mut max_error: f64 = 0.0;
            for tick in 0..TICKS + WARMUP {
                let phase = std::f64::consts::TAU * tick as f64 * DT / 4.0;
                let body = DisplacementBox {
                    center: Vec2::new(
                        (15.0 * phase.sin()) as f32,
                        (20.0 + 12.0 * phase.cos()) as f32,
                    ),
                    half_extents: Vec2::new(10.0, 5.0),
                };
                let start = Instant::now();
                black_box(&mut water)
                    .set_displacer(0, enabled.then_some(body))
                    .unwrap();
                black_box(&mut water).step(DT).unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1.0e6;
                if tick >= WARMUP {
                    times.push(elapsed);
                }
                let stats = water.stats();
                peak_displaced = peak_displaced.max(stats.displaced);
                let error = (stats.injected - stats.pooled).abs();
                assert!(error < 1.0e-7);
                assert_eq!(
                    (stats.in_flight, stats.drained, stats.reclaimed),
                    (0.0, 0.0, 0.0)
                );
                max_error = max_error.max(error);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{} columns={columns} ticks={TICKS} submit_and_step_us p50={:.3} p95={:.3} p99={:.3} max={:.3} displaced_peak={peak_displaced:.3} max_volume_error={max_error:.3e}",
                if enabled { "displacement" } else { "control" },
                times[TICKS / 2],
                times[TICKS * 95 / 100],
                times[TICKS * 99 / 100],
                times[TICKS - 1],
            );
        }
    }
}
