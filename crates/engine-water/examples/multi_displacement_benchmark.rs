//! Bounded mixed box/circle union costs, with matched empty-input controls.
//! Motion construction/statistics are outside the timer; occupancy submission
//! and water stepping are timed separately. No physics/rendering or timing asserts.
use engine_core::Vec2;
use engine_water::{
    Boundary, PoolSpec, WaterConfig, WaterWorld,
    displacement::{DisplacementBody, MAX_DISPLACERS},
    immersion::HullShape,
};
use std::{hint::black_box, time::Instant};

const TICKS: usize = 6000;
const WARMUP: usize = 600;
const DT: f64 = 1.0 / 60.0;

fn p95(values: &mut [f64]) -> f64 {
    values.sort_unstable_by(f64::total_cmp);
    values[values.len() * 95 / 100]
}

fn main() {
    for columns in [32, 128, 512] {
        for count in [1, 2, 4, MAX_DISPLACERS] {
            for overlap in [false, true] {
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
                    let mut submits = Vec::with_capacity(TICKS);
                    let mut steps = Vec::with_capacity(TICKS);
                    let mut combined = Vec::with_capacity(TICKS);
                    let mut max_error: f64 = 0.0;
                    let mut peak_displaced: f64 = 0.0;
                    for tick in 0..TICKS + WARMUP {
                        let phase = tick as f32 * DT as f32;
                        let bodies =
                            std::array::from_fn::<_, MAX_DISPLACERS, _>(|i| DisplacementBody {
                                center: Vec2::new(
                                    if overlap {
                                        (phase + i as f32).sin() * 3.0
                                    } else {
                                        -35.0 + i as f32 * 10.0
                                    },
                                    20.0 + (phase * 1.5
                                        + if overlap { i as f32 * 0.3 } else { i as f32 })
                                    .cos()
                                        * 8.0,
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
                        let start = Instant::now();
                        black_box(&mut water)
                            .set_displacers(0, if enabled { &bodies[..count] } else { &[] })
                            .unwrap();
                        let submitted = start.elapsed().as_secs_f64() * 1e6;
                        let start = Instant::now();
                        black_box(&mut water).step(DT).unwrap();
                        let stepped = start.elapsed().as_secs_f64() * 1e6;
                        if tick >= WARMUP {
                            submits.push(submitted);
                            steps.push(stepped);
                            combined.push(submitted + stepped);
                        }
                        let stats = water.stats();
                        max_error = max_error.max((stats.injected - stats.pooled).abs());
                        peak_displaced = peak_displaced.max(stats.displaced);
                        assert!(max_error < 1e-7);
                        assert_eq!(
                            (stats.in_flight, stats.drained, stats.reclaimed),
                            (0.0, 0.0, 0.0)
                        );
                    }
                    println!(
                        "columns={columns} bodies={count} overlap={overlap} feedback={enabled} ticks={TICKS} submit_p95_us={:.3} step_p95_us={:.3} combined_p95_us={:.3} displaced_peak={peak_displaced:.3} max_volume_error={max_error:.3e}",
                        p95(&mut submits),
                        p95(&mut steps),
                        p95(&mut combined)
                    );
                }
            }
        }
    }
}
