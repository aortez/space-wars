//! Same repeated impacts, with surface response disabled/enabled. Simulation
//! only, no rendering/sleeping or wall-time correctness gates.
use engine_core::Vec2;
use engine_water::{Boundary, Parcel, PoolSpec, WaterConfig, WaterWorld};
use std::{hint::black_box, time::Instant};

fn main() {
    for columns in [32, 128, 512] {
        for response in [0.0, 0.25] {
            let dx = 300.0 / columns as f64;
            let mut water = WaterWorld::new(
                WaterConfig {
                    impact_response: response,
                    ..WaterConfig::default()
                },
                vec![PoolSpec {
                    left: -150.0,
                    column_width: dx,
                    bed: vec![0.0; columns],
                    boundaries: [Boundary::Closed; 2],
                }],
            )
            .unwrap();
            for i in 0..columns {
                water
                    .add_to_pool(0, -150.0 + (i as f64 + 0.5) * dx, dx * 6.0)
                    .unwrap();
            }
            let mut times = Vec::with_capacity(6000);
            let mut max_error: f64 = 0.0;
            for tick in 0..6600 {
                for source in 0..2 {
                    let i = (tick * 13 + source * 11) % columns;
                    let c = water.pools()[0].columns().nth(i).unwrap();
                    water
                        .add_falling(Parcel {
                            position: Vec2::new(
                                (c.left + c.width * 0.5) as f32,
                                c.surface as f32 + 1.0,
                            ),
                            velocity: Vec2::new(0.0, -180.0),
                            volume: 0.5,
                            duration: 1.0 / 60.0,
                            horizontal_bounds: None,
                        })
                        .unwrap();
                }
                let start = Instant::now();
                black_box(&mut water).step(1.0 / 60.0).unwrap();
                let us = start.elapsed().as_secs_f64() * 1e6;
                if tick >= 600 {
                    times.push(us);
                }
                let stats = water.stats();
                max_error = max_error.max(
                    (stats.injected
                        - stats.pooled
                        - stats.in_flight
                        - stats.drained
                        - stats.reclaimed)
                        .abs(),
                );
            }
            let stats = water.stats();
            assert_eq!(
                stats.impact_transfers,
                if response == 0.0 { 0 } else { 13_200 }
            );
            assert!(max_error < stats.injected * 1e-10);
            times.sort_by(f64::total_cmp);
            println!(
                "columns={columns} response={response} transfers={} max_volume_error={max_error:.3e} step_us p50={:.3} p95={:.3} p99={:.3}",
                stats.impact_transfers, times[3000], times[5700], times[5940]
            );
        }
    }
}
