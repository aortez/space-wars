//! Water stepping only: no Clock, renderer, Rapier, or wall-clock assertions.
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld};
use std::{hint::black_box, time::Instant};

const DT: f64 = 1.0 / 60.0;
const TICKS: usize = 12_000;

fn main() {
    opposed();
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

fn opposed() {
    for depth in [5.0, 15.0] {
        for mix_spills in [false, true] {
            let mut water = WaterWorld::new(
                WaterConfig {
                    mix_spills,
                    exit_y: -240.0,
                    max_parcels: 128,
                    spill_channel: Some([-18.0, 18.0]),
                    ..WaterConfig::default()
                },
                vec![
                    PoolSpec {
                        left: -58.0,
                        column_width: 40.0 / 64.0,
                        bed: vec![0.0; 64],
                        boundaries: [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
                    },
                    PoolSpec {
                        left: 18.0,
                        column_width: 40.0 / 64.0,
                        bed: vec![0.0; 64],
                        boundaries: [Boundary::Spill { lip: 0.0 }, Boundary::Closed],
                    },
                ],
            )
            .unwrap();
            let mut times = Vec::with_capacity(TICKS);
            let mut peak = 0;
            for tick in 0..TICKS + 600 {
                for pool in 0..2 {
                    for col in 0..64 {
                        let c = water.pools()[pool].columns().nth(col).unwrap();
                        let missing = depth * c.width - c.volume;
                        if missing > 0.0 {
                            water
                                .add_to_pool(pool, c.left + c.width * 0.5, missing)
                                .unwrap();
                        }
                    }
                }
                let start = Instant::now();
                black_box(&mut water).step(DT).unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1e6;
                if tick >= 600 {
                    times.push(elapsed);
                }
                let s = water.stats();
                peak = peak.max(s.parcels);
                assert!(
                    (s.injected - s.pooled - s.in_flight - s.drained).abs() < s.injected * 1e-10
                );
            }
            times.sort_by(f64::total_cmp);
            let s = water.stats();
            println!(
                "opposed columns=128 depth={depth} mixing={mix_spills} ticks={TICKS} step_us p50={:.3} p95={:.3} p99={:.3} parcels_peak={peak} capacity_limited={} merges={} pair_checks={}",
                times[TICKS / 2],
                times[TICKS * 95 / 100],
                times[TICKS * 99 / 100],
                s.capacity_limited_ticks,
                s.spill_merges,
                s.mixing_pair_checks
            );
            assert_eq!(s.spill_merges > 0, mix_spills);
        }
    }
}
