//! Deterministic fixed-slope and moving-floor lab, simulation only. No timing
//! assertion, sleep, renderer, or additional live scenario.
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld};
use scenario_clock::water_fixture::ResponsiveFloorFixture;
use std::{hint::black_box, time::Instant};

fn report(label: &str, times: &mut [f64]) {
    times.sort_by(f64::total_cmp);
    println!(
        "{label}: step_us p50={:.2} p95={:.2} p99={:.2}",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() * 99 / 100]
    );
}

fn main() {
    for n in [32, 128, 512] {
        for sloped in [false, true] {
            let dx = 160.0 / n as f64;
            let mut water = WaterWorld::new(
                WaterConfig::default(),
                vec![PoolSpec {
                    left: 0.0,
                    column_width: dx,
                    bed: vec![0.0; n],
                    boundaries: [Boundary::Closed; 2],
                }],
            )
            .unwrap();
            if sloped {
                let edges: Vec<_> = (0..n)
                    .map(|i| [8.0 * i as f64 / n as f64, 8.0 * (i + 1) as f64 / n as f64])
                    .collect();
                water.configure_sloped_bed(0, &edges).unwrap();
            }
            let columns: Vec<_> = water.pools()[0].columns().collect();
            for c in columns {
                water
                    .add_to_pool(0, c.left + dx * 0.5, c.area_below(12.0))
                    .unwrap();
            }
            let mut times = Vec::with_capacity(6000);
            for tick in 0..6600 {
                let start = Instant::now();
                black_box(&mut water).step(1.0 / 60.0).unwrap();
                let us = start.elapsed().as_secs_f64() * 1e6;
                if tick >= 600 {
                    times.push(us);
                }
            }
            report(&format!("columns={n} sloped={sloped}"), &mut times);
        }
    }
    for duck in [false, true] {
        let mut fixture = ResponsiveFloorFixture::new(7.0, duck);
        let mut times = Vec::with_capacity(1200);
        let mut peak = 0;
        for tick in 0..1200 {
            let start = Instant::now();
            black_box(&mut fixture).step(if tick < 600 { 1.0 } else { 0.0 });
            let us = start.elapsed().as_secs_f64() * 1e6;
            if tick >= 60 {
                times.push(us);
            }
            peak = peak.max(fixture.water.stats().parcels);
        }
        report(
            &format!(
                "responsive-floor columns=64 duck={duck} peak_parcels={peak} deferrals={} duck_exited={}",
                fixture.deferrals, fixture.duck_exited
            ),
            &mut times,
        );
        let s = fixture.water.stats();
        assert!((s.injected - s.pooled - s.in_flight - s.drained).abs() < 1e-7);
    }
}
