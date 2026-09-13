//! Water stepping only: no Clock, renderer, Rapier, or wall-clock assertions.
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld};
use std::{hint::black_box, time::Instant};

const DT: f64 = 1.0 / 60.0;
const TICKS: usize = 12_000;

fn main() {
    for columns in [32, 128, 512] {
        for cascade in [false, true] {
            let pools = if cascade {
                vec![
                    PoolSpec {
                        left: -150.0,
                        column_width: 120.0 / (columns / 2) as f64,
                        bed: vec![50.0; columns / 2],
                        boundaries: [Boundary::Closed, Boundary::Spill { lip: 50.0 }],
                    },
                    PoolSpec {
                        left: -150.0,
                        column_width: 600.0 / (columns / 2) as f64,
                        bed: vec![-100.0; columns / 2],
                        boundaries: [Boundary::Closed, Boundary::Spill { lip: -100.0 }],
                    },
                ]
            } else {
                vec![PoolSpec {
                    left: -150.0,
                    column_width: 300.0 / columns as f64,
                    bed: vec![0.0; columns],
                    boundaries: [Boundary::Closed; 2],
                }]
            };
            let mut water = WaterWorld::new(WaterConfig::default(), pools).unwrap();
            water.add_to_pool(0, -100.0, 1200.0).unwrap();
            let mut times = Vec::with_capacity(TICKS);
            let mut peak_parcels = 0;
            let mut max_error: f64 = 0.0;
            for tick in 0..TICKS + 600 {
                // Keep both fixtures active; source insertion and diagnostics
                // are deliberately outside the measured step interval.
                water
                    .add_to_pool(0, -140.0 + (tick % 80) as f64, 0.25)
                    .unwrap();
                let start = Instant::now();
                black_box(&mut water).step(DT).unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1.0e6;
                if tick >= 600 {
                    times.push(elapsed);
                }
                let stats = water.stats();
                peak_parcels = peak_parcels.max(stats.parcels);
                let error = (stats.injected
                    - stats.pooled
                    - stats.in_flight
                    - stats.drained
                    - stats.reclaimed)
                    .abs();
                assert!(error < stats.injected.max(1.0) * 1.0e-10);
                max_error = max_error.max(error);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{} columns={columns} ticks={TICKS} step_us p50={:.3} p95={:.3} p99={:.3} max={:.3} parcels_peak={peak_parcels} capacity_limited={} max_volume_error={max_error:.3e}",
                if cascade { "cascade" } else { "closed" },
                times[TICKS / 2],
                times[TICKS * 95 / 100],
                times[TICKS * 99 / 100],
                times[TICKS - 1],
                water.stats().capacity_limited_ticks
            );
        }
    }
}
