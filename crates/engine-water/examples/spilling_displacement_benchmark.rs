//! Matched open/closed basins, with and without displacement feedback.
//! Reset the fixture every ten simulated seconds OUTSIDE the timer so outflow
//! remains exercised. No hidden refill, physics, rendering or timing assertions.
use engine_core::Vec2;
use engine_water::{
    Boundary, PoolSpec, WaterConfig, WaterWorld,
    displacement::{DisplacementBody, MAX_DISPLACERS},
    immersion::HullShape,
};
use std::{hint::black_box, time::Instant};

const CYCLE: usize = 600;
const TICKS: usize = 6000;
const DT: f64 = 1.0 / 60.0;

fn fixture(columns: usize, open: bool) -> WaterWorld {
    let per_pool = columns / 2;
    let mut water = WaterWorld::new(
        WaterConfig {
            gravity: 40.0,
            damping: 2.0,
            exit_y: -120.0,
            max_parcels: 512,
            ..WaterConfig::default()
        },
        vec![
            PoolSpec {
                left: -50.0,
                column_width: 100.0 / per_pool as f64,
                bed: vec![0.0; per_pool],
                boundaries: [
                    Boundary::Closed,
                    if open {
                        Boundary::Spill { lip: 20.0 }
                    } else {
                        Boundary::Closed
                    },
                ],
            },
            PoolSpec {
                left: -200.0,
                column_width: 400.0 / per_pool as f64,
                bed: vec![-80.0; per_pool],
                boundaries: [Boundary::Closed; 2],
            },
        ],
    )
    .unwrap();
    for i in 0..per_pool {
        water
            .add_to_pool(
                0,
                -50.0 + (i as f64 + 0.5) * 100.0 / per_pool as f64,
                2000.0 / per_pool as f64,
            )
            .unwrap();
    }
    water
}

fn p95(values: &mut [f64]) -> f64 {
    values.sort_unstable_by(f64::total_cmp);
    values[values.len() * 95 / 100]
}

fn main() {
    for columns in [32, 128, 512] {
        for count in [1, 4, MAX_DISPLACERS] {
            for open in [false, true] {
                for enabled in [false, true] {
                    let mut water = fixture(columns, open);
                    let mut submits = Vec::with_capacity(TICKS);
                    let mut steps = Vec::with_capacity(TICKS);
                    let mut combined = Vec::with_capacity(TICKS);
                    let mut emitting_ticks = 0;
                    let mut peak_collected: f64 = 0.0;
                    let mut peak_parcels = 0;
                    let mut max_error: f64 = 0.0;
                    // The first complete cycle is warm-up, excluded from timing.
                    for tick in 0..TICKS + CYCLE {
                        let cycle_tick = tick % CYCLE;
                        if cycle_tick == 0 && tick > 0 {
                            water = fixture(columns, open);
                        }
                        let phase = cycle_tick as f32 * DT as f32;
                        let bodies =
                            std::array::from_fn::<_, MAX_DISPLACERS, _>(|i| DisplacementBody {
                                center: Vec2::new(
                                    -35.0 + i as f32 * 10.0,
                                    20.0 + (phase * 1.5 + i as f32 * 0.3).cos() * 8.0,
                                ),
                                angle: phase + i as f32,
                                shape: if i % 2 == 0 {
                                    HullShape::Box {
                                        half_width: 2.0,
                                        half_height: 1.0,
                                    }
                                } else {
                                    HullShape::Circle { radius: 2.0 }
                                },
                            });
                        let source_before: f64 = water.pools()[0].columns().map(|c| c.volume).sum();
                        let start = Instant::now();
                        for pool in 0..2 {
                            black_box(&mut water)
                                .set_displacers(pool, if enabled { &bodies[..count] } else { &[] })
                                .unwrap();
                        }
                        let submitted = start.elapsed().as_secs_f64() * 1e6;
                        let start = Instant::now();
                        black_box(&mut water).step(DT).unwrap();
                        let stepped = start.elapsed().as_secs_f64() * 1e6;
                        if tick >= CYCLE {
                            submits.push(submitted);
                            steps.push(stepped);
                            combined.push(submitted + stepped);
                            let source: f64 = water.pools()[0].columns().map(|c| c.volume).sum();
                            emitting_ticks += usize::from(source < source_before - 1e-9);
                        }
                        let stats = water.stats();
                        let error = (stats.injected
                            - stats.pooled
                            - stats.in_flight
                            - stats.drained
                            - stats.reclaimed)
                            .abs();
                        max_error = max_error.max(error);
                        peak_collected = peak_collected
                            .max(water.pools()[1].columns().map(|c| c.volume).sum::<f64>());
                        peak_parcels = peak_parcels.max(water.parcels().len());
                        assert!(error < 1e-7);
                        assert_eq!((stats.drained, stats.reclaimed), (0.0, 0.0));
                        assert_eq!(stats.capacity_limited_ticks, 0);
                    }
                    if open && enabled {
                        assert!(emitting_ticks > 0 && peak_collected > 0.0);
                    } else {
                        assert_eq!((emitting_ticks, peak_collected), (0, 0.0));
                    }
                    println!(
                        "columns={columns} bodies={count} open={open} feedback={enabled} ticks={TICKS} emitting_ticks={emitting_ticks} submit_p95_us={:.3} step_p95_us={:.3} combined_p95_us={:.3} collected_peak={peak_collected:.3} parcels_peak={peak_parcels} max_volume_error={max_error:.3e}",
                        p95(&mut submits),
                        p95(&mut steps),
                        p95(&mut combined)
                    );
                }
            }
        }
    }
}
